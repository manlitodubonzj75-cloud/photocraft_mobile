//! Touch Router: deterministic arbitration, ownership tracking, and multi-contact FSM.
//!
//! # Core Invariants
//!
//! 1. **Capture from Down to Up/Cancel**: An [`InputOwner`] is decided once at `PointerPhase::Down`
//!    and persists until that contact is cleanly lifted or cancelled.
//! 2. **UI Safety & Exclusion**: Touches landing in UI exclusion zones (sheets, top bar, nav bar,
//!    popups, modal dialogs) are owned by [`InputOwner::Ui`] or [`InputOwner::SheetHandle`]. They
//!    never produce canvas actions. While any UI contact is active, new canvas contacts are suppressed.
//! 3. **Stroke Cancellation**: A pending or active live drawing stroke is immediately discarded
//!    (without mutating document, history, or revision) when a 2nd contact arrives or context changes.
//! 4. **Navigation Lockdown**: Navigating (2-finger Pan/Zoom) locks out drawing until ALL participating
//!    contacts have been lifted off the screen (DrainUntilAllUp).
//! 5. **Two-Finger Tap Undo**: Recognized only if both fingers touch and release within tight time
//!    (<250 ms) and distance (<8 pt) thresholds, and ONLY if the gesture cycle was not previously
//!    in active Drawing.

use std::collections::HashMap;
use std::time::Duration;

use egui::{Pos2, Rect, Vec2};

use super::canvas_nav::{GestureDelta, TwoFingerNavTracker};
use super::pointer::{ContactKey, PointerKind, PointerPhase, PointerSample};

/// Subsystem that owns an active touch contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputOwner {
    /// UI widgets, buttons, sliders, top bar, navigation bar, modal dialogs.
    Ui,
    /// The bottom sheet drag handle.
    SheetHandle,
    /// Interactive canvas tool or navigation.
    Canvas,
    /// Suppressed touch (e.g. palm contact or finger touching while UI/stylus is active).
    Suppressed,
}

/// Tracking state for one active physical contact.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactState {
    pub kind: PointerKind,
    pub owner: InputOwner,
    pub down_pos: Pos2,
    pub previous_pos: Pos2,
    pub current_pos: Pos2,
    pub down_ms: u64,
    pub up_ms: Option<u64>,
    pub max_distance_pt: f32,
}

/// Global gesture arbitration state for canvas interactions.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasGestureState {
    /// No canvas contacts active.
    Idle,
    /// 1 finger down on canvas; waiting for movement > slop or arbitration deadline.
    PendingStroke { pointer: ContactKey, deadline_ms: u64 },
    /// Confirmed drawing: streaming points to the live stroke adapter.
    Drawing { pointer: ContactKey },
    /// 2 fingers active on canvas; candidate for 2-finger tap Undo or navigation.
    TwoFingerCandidate { pair: [ContactKey; 2] },
    /// 2 fingers actively navigating (Pan / Zoom). Drawing is locked out.
    Navigating { pair: [ContactKey; 2] },
    /// Waiting for all contacts to lift before permitting any new strokes.
    DrainUntilAllUp,
}

/// Configuration thresholds for the touch router state machine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureConfig {
    /// Arbitration window in milliseconds before a stationary finger transitions to Drawing.
    pub arbitration_ms: u64,
    /// Movement threshold in logical points to transition from Pending to Drawing.
    pub stroke_slop_pt: f32,
    /// Movement threshold in logical points for tap recognition (including final Up position).
    pub tap_slop_pt: f32,
    /// Maximum time gap in milliseconds between two Down events for a 2-finger chord.
    pub two_finger_chord_gap_ms: u64,
    /// Maximum total duration from first Down to final Up for tap recognition.
    pub total_tap_duration_ms: u64,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self { arbitration_ms: 70, stroke_slop_pt: 6.0, tap_slop_pt: 8.0, two_finger_chord_gap_ms: 120, total_tap_duration_ms: 250 }
    }
}

/// Snapshot of the screen layout and exclusion zones for the current frame.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutSnapshot {
    pub canvas_rect: Rect,
    pub ui_exclusion_rects: Vec<Rect>,
    pub sheet_handle_rect: Option<Rect>,
    pub modal_open: bool,
}

impl LayoutSnapshot {
    pub fn is_touch_excluded(&self, pos: Pos2) -> bool {
        self.modal_open || self.ui_exclusion_rects.iter().any(|r| r.contains(pos))
    }

    pub fn hits_sheet_handle(&self, pos: Pos2) -> bool {
        !self.modal_open && self.sheet_handle_rect.as_ref().is_some_and(|r| r.contains(pos))
    }

    pub fn hits_canvas(&self, pos: Pos2) -> bool {
        !self.modal_open && self.canvas_rect.contains(pos) && !self.is_touch_excluded(pos)
    }
}

/// Actions dispatched by the [`TouchRouter`] to the canvas stroke adapter.
#[derive(Clone, Debug, PartialEq)]
pub enum TouchAction {
    /// Start tentative pending stroke buffer without live preview.
    StartPendingStroke { key: ContactKey, pos: Pos2, pressure: f32 },
    /// Promote stroke to confirmed live preview, replaying buffered points.
    BeginLiveStroke { key: ContactKey, points: Vec<(Pos2, f32)> },
    /// Feed point to the active live stroke preview.
    FeedLiveStroke { key: ContactKey, pos: Pos2, pressure: f32 },
    /// Commit the active live stroke to document and history.
    CommitStroke { key: ContactKey, pos: Pos2 },
    /// Stationary single finger tap: commit a single dab through tool command.
    DotStroke { key: ContactKey, pos: Pos2, pressure: f32 },
    /// Discard transient live stroke preview completely without altering document/history.
    CancelStroke,
    /// Apply 2-finger Pan/Zoom delta to active canvas view.
    Navigate(GestureDelta),
    /// Two-finger tap recognized: trigger one Undo command.
    TwoFingerTapUndo,
    /// Request context repaint after the specified duration.
    RequestRepaintAfter(Duration),
}

/// Stateful Touch Router.
#[derive(Clone, Debug)]
pub struct TouchRouter {
    pub config: GestureConfig,
    pub fsm: CanvasGestureState,
    pub contacts: HashMap<ContactKey, ContactState>,
    pub nav_tracker: Option<TwoFingerNavTracker>,
    pub pending_stroke_points: Vec<(Pos2, f32)>,
    pub was_ever_drawing_in_cycle: bool,
    pub cycle_first_down_ms: u64,
    pub candidate_pair: Option<[ContactKey; 2]>,
    pub lifted_candidate_keys: Vec<(ContactKey, Pos2, u64)>,
    pub down_order: Vec<ContactKey>,
    pub last_processed_sample_timestamp_ms: u64,
}

impl Default for TouchRouter {
    fn default() -> Self {
        Self {
            config: GestureConfig::default(),
            fsm: CanvasGestureState::Idle,
            contacts: HashMap::new(),
            nav_tracker: None,
            pending_stroke_points: Vec::new(),
            was_ever_drawing_in_cycle: false,
            cycle_first_down_ms: 0,
            candidate_pair: None,
            lifted_candidate_keys: Vec::new(),
            down_order: Vec::new(),
            last_processed_sample_timestamp_ms: 0,
        }
    }
}

impl TouchRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether there is an active touch contact on the canvas or an active canvas FSM gesture.
    pub fn has_active_canvas_touch(&self) -> bool {
        !matches!(self.fsm, CanvasGestureState::Idle) || self.contacts.values().any(|c| c.owner == InputOwner::Canvas && c.up_ms.is_none())
    }

    /// Process a normalized pointer sample against the current layout snapshot.
    pub fn process_sample(&mut self, sample: PointerSample, layout: &LayoutSnapshot) -> Vec<TouchAction> {
        self.last_processed_sample_timestamp_ms = sample.timestamp_ms;

        match sample.phase {
            PointerPhase::Down => self.handle_down(sample, layout),
            PointerPhase::Move => self.handle_move(sample),
            PointerPhase::Up => self.handle_up(sample),
            PointerPhase::Cancel => self.cancel_all(),
        }
    }

    /// Periodic tick (called each frame) to trigger arbitration deadlines.
    pub fn tick(&mut self, now_ms: u64) -> Vec<TouchAction> {
        if let CanvasGestureState::PendingStroke { pointer, deadline_ms } = self.fsm {
            if now_ms >= deadline_ms {
                self.fsm = CanvasGestureState::Drawing { pointer };
                self.was_ever_drawing_in_cycle = true;
                let points = std::mem::take(&mut self.pending_stroke_points);
                return vec![TouchAction::BeginLiveStroke { key: pointer, points }];
            }
        }
        Vec::new()
    }

    /// Abort all in-flight gestures, clean up state, and cancel uncommitted strokes.
    pub fn cancel_all(&mut self) -> Vec<TouchAction> {
        let mut actions = Vec::new();
        if matches!(self.fsm, CanvasGestureState::PendingStroke { .. } | CanvasGestureState::Drawing { .. }) {
            actions.push(TouchAction::CancelStroke);
        }
        self.fsm = CanvasGestureState::Idle;
        self.contacts.clear();
        self.pending_stroke_points.clear();
        self.nav_tracker = None;
        self.was_ever_drawing_in_cycle = false;
        self.candidate_pair = None;
        self.lifted_candidate_keys.clear();
        self.down_order.clear();
        actions
    }

    /// Return reference to current canvas gesture FSM state.
    pub fn state(&self) -> &CanvasGestureState {
        &self.fsm
    }

    /// Reset router state completely to Idle.
    pub fn reset(&mut self) {
        let _ = self.cancel_all();
    }

    fn handle_down(&mut self, sample: PointerSample, layout: &LayoutSnapshot) -> Vec<TouchAction> {
        let key = sample.key();
        let now = sample.timestamp_ms;
        let mut actions = Vec::new();

        if self.contacts.is_empty() {
            self.cycle_first_down_ms = now;
            self.was_ever_drawing_in_cycle = false;
            self.candidate_pair = None;
            self.lifted_candidate_keys.clear();
            self.down_order.clear();
        }
        self.down_order.push(key);

        // Determine owner for this contact
        let owner = if layout.modal_open {
            InputOwner::Ui
        } else if layout.hits_sheet_handle(sample.position) {
            InputOwner::SheetHandle
        } else if layout.is_touch_excluded(sample.position) {
            InputOwner::Ui
        } else if layout.hits_canvas(sample.position) {
            // If any active contact already belongs to UI or SheetHandle, new canvas contacts are suppressed
            let ui_active = self.contacts.values().any(|c| (c.owner == InputOwner::Ui || c.owner == InputOwner::SheetHandle) && c.up_ms.is_none());
            if ui_active || self.fsm == CanvasGestureState::DrainUntilAllUp {
                InputOwner::Suppressed
            } else if matches!(sample.kind, PointerKind::Stylus | PointerKind::Eraser) {
                // Stylus takes immediate priority over finger contacts
                InputOwner::Canvas
            } else {
                // If a stylus contact is currently down on canvas, suppress fingers (palm rejection)
                let stylus_down = self.contacts.values().any(|c| matches!(c.kind, PointerKind::Stylus | PointerKind::Eraser) && c.up_ms.is_none());
                if stylus_down { InputOwner::Suppressed } else { InputOwner::Canvas }
            }
        } else {
            InputOwner::Suppressed
        };

        // Record contact state
        self.contacts.insert(
            key,
            ContactState {
                kind: sample.kind,
                owner,
                down_pos: sample.position,
                previous_pos: sample.position,
                current_pos: sample.position,
                down_ms: now,
                up_ms: None,
                max_distance_pt: 0.0,
            },
        );

        // Route based on owner and FSM
        match owner {
            InputOwner::Ui | InputOwner::SheetHandle => {
                // If a canvas stroke was in pending or drawing, cancel it immediately
                if matches!(self.fsm, CanvasGestureState::PendingStroke { .. } | CanvasGestureState::Drawing { .. }) {
                    actions.push(TouchAction::CancelStroke);
                    self.fsm = CanvasGestureState::DrainUntilAllUp;
                }
            }
            InputOwner::Suppressed => {
                // Suppressed contact does not drive canvas FSM
            }
            InputOwner::Canvas => {
                if matches!(sample.kind, PointerKind::Stylus | PointerKind::Eraser) {
                    // Stylus cancels any active finger stroke and starts drawing immediately
                    if matches!(self.fsm, CanvasGestureState::PendingStroke { .. } | CanvasGestureState::Drawing { .. }) {
                        actions.push(TouchAction::CancelStroke);
                    }
                    self.fsm = CanvasGestureState::Drawing { pointer: key };
                    self.was_ever_drawing_in_cycle = true;
                    actions.push(TouchAction::BeginLiveStroke { key, points: vec![(sample.position, sample.pressure.unwrap_or(1.0))] });
                } else {
                    match self.fsm {
                        CanvasGestureState::Idle => {
                            self.fsm = CanvasGestureState::PendingStroke { pointer: key, deadline_ms: now + self.config.arbitration_ms };
                            let pressure = sample.pressure.unwrap_or(1.0);
                            self.pending_stroke_points = vec![(sample.position, pressure)];
                            actions.push(TouchAction::StartPendingStroke { key, pos: sample.position, pressure });
                            actions.push(TouchAction::RequestRepaintAfter(Duration::from_millis(self.config.arbitration_ms)));
                        }
                        CanvasGestureState::PendingStroke { pointer: first_key, .. } => {
                            // 2nd finger down before arbitration deadline: cancel pending stroke and enter TwoFingerCandidate
                            actions.push(TouchAction::CancelStroke);
                            let first_pos = self.contacts.get(&first_key).map_or(sample.position, |c| c.current_pos);
                            let pair = [first_key, key];
                            self.candidate_pair = Some(pair);
                            self.nav_tracker = Some(TwoFingerNavTracker::new(first_key, first_pos, key, sample.position));
                            self.fsm = CanvasGestureState::TwoFingerCandidate { pair };
                        }
                        CanvasGestureState::Drawing { pointer: first_key } => {
                            // 2nd finger down after drawing started: cancel active drawing stroke and enter TwoFingerCandidate
                            self.was_ever_drawing_in_cycle = true;
                            actions.push(TouchAction::CancelStroke);
                            let first_pos = self.contacts.get(&first_key).map_or(sample.position, |c| c.current_pos);
                            let pair = [first_key, key];
                            self.candidate_pair = Some(pair);
                            self.nav_tracker = Some(TwoFingerNavTracker::new(first_key, first_pos, key, sample.position));
                            self.fsm = CanvasGestureState::TwoFingerCandidate { pair };
                        }
                        CanvasGestureState::TwoFingerCandidate { .. } => {
                            // 3rd finger arrived: cancel tap candidate (invalidates Undo), retain existing pair for navigation
                            self.candidate_pair = None;
                        }
                        CanvasGestureState::Navigating { .. } | CanvasGestureState::DrainUntilAllUp => {
                            // Extra fingers do not alter established navigation pair
                        }
                    }
                }
            }
        }

        actions
    }

    fn handle_move(&mut self, sample: PointerSample) -> Vec<TouchAction> {
        let key = sample.key();
        let mut actions = Vec::new();

        let Some(contact) = self.contacts.get_mut(&key) else {
            return actions;
        };

        contact.previous_pos = contact.current_pos;
        contact.current_pos = sample.position;
        let dist = (sample.position - contact.down_pos).length();
        contact.max_distance_pt = contact.max_distance_pt.max(dist);

        if contact.owner != InputOwner::Canvas {
            return actions;
        }

        match self.fsm {
            CanvasGestureState::PendingStroke { pointer, .. } if pointer == key => {
                let pressure = sample.pressure.unwrap_or(1.0);
                self.pending_stroke_points.push((sample.position, pressure));
                if contact.max_distance_pt > self.config.stroke_slop_pt {
                    self.fsm = CanvasGestureState::Drawing { pointer };
                    self.was_ever_drawing_in_cycle = true;
                    let points = std::mem::take(&mut self.pending_stroke_points);
                    actions.push(TouchAction::BeginLiveStroke { key: pointer, points });
                }
            }
            CanvasGestureState::Drawing { pointer } if pointer == key => {
                let pressure = sample.pressure.unwrap_or(1.0);
                actions.push(TouchAction::FeedLiveStroke { key: pointer, pos: sample.position, pressure });
            }
            CanvasGestureState::TwoFingerCandidate { pair } => {
                if pair.contains(&key) {
                    if !self.lifted_candidate_keys.is_empty() {
                        self.fsm = CanvasGestureState::DrainUntilAllUp;
                        return actions;
                    }
                    if let Some(tracker) = self.nav_tracker.as_mut() {
                        let _ = tracker.update_contact(key, sample.position);
                        if tracker.max_travel_from_initial() > self.config.stroke_slop_pt {
                            tracker.reset_baseline();
                            self.fsm = CanvasGestureState::Navigating { pair };
                            self.candidate_pair = None; // Invalidate tap Undo permanently
                        }
                    }
                }
            }
            CanvasGestureState::Navigating { pair } => {
                if pair.contains(&key) {
                    if let Some(tracker) = self.nav_tracker.as_mut() {
                        if let Some(delta) = tracker.update_contact(key, sample.position) {
                            if delta.translation != Vec2::ZERO || (delta.scale - 1.0).abs() > 1e-5 {
                                actions.push(TouchAction::Navigate(delta));
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        actions
    }

    fn handle_up(&mut self, sample: PointerSample) -> Vec<TouchAction> {
        let key = sample.key();
        let now = sample.timestamp_ms;
        let mut actions = Vec::new();

        let Some(contact) = self.contacts.get_mut(&key) else {
            return actions;
        };

        contact.up_ms = Some(now);
        contact.previous_pos = contact.current_pos;
        contact.current_pos = sample.position;
        let dist = (sample.position - contact.down_pos).length();
        contact.max_distance_pt = contact.max_distance_pt.max(dist);
        let owner = contact.owner;

        if owner == InputOwner::Canvas {
            match self.fsm {
                CanvasGestureState::PendingStroke { pointer, .. } if pointer == key => {
                    // Tap on canvas without exceeding slop or deadline: Dot stroke!
                    self.fsm = CanvasGestureState::Idle;
                    let pressure = sample.pressure.unwrap_or(1.0);
                    actions.push(TouchAction::DotStroke { key: pointer, pos: sample.position, pressure });
                }
                CanvasGestureState::Drawing { pointer } if pointer == key => {
                    self.fsm = CanvasGestureState::Idle;
                    actions.push(TouchAction::CommitStroke { key: pointer, pos: sample.position });
                }
                CanvasGestureState::TwoFingerCandidate { pair } => {
                    if pair.contains(&key) {
                        self.lifted_candidate_keys.push((key, sample.position, now));
                        if self.lifted_candidate_keys.len() == 2 {
                            // Both fingers of the pair have lifted!
                            let (k0, p0, t0) = self.lifted_candidate_keys[0];
                            let (k1, p1, t1) = self.lifted_candidate_keys[1];
                            let c0 = self.contacts.get(&k0);
                            let c1 = self.contacts.get(&k1);

                            let chord_gap_ok = t1.abs_diff(t0) <= self.config.two_finger_chord_gap_ms;
                            let total_duration = t1.max(t0).saturating_sub(self.cycle_first_down_ms);
                            let duration_ok = total_duration <= self.config.total_tap_duration_ms;

                            let travel0 = c0.map_or(0.0, |c| (p0 - c.down_pos).length().max(c.max_distance_pt));
                            let travel1 = c1.map_or(0.0, |c| (p1 - c.down_pos).length().max(c.max_distance_pt));
                            let travel_ok = travel0 <= self.config.tap_slop_pt && travel1 <= self.config.tap_slop_pt;

                            if chord_gap_ok && duration_ok && travel_ok && !self.was_ever_drawing_in_cycle {
                                actions.push(TouchAction::TwoFingerTapUndo);
                            }
                            self.fsm = CanvasGestureState::Idle;
                        }
                    }
                }
                CanvasGestureState::Navigating { pair } => {
                    if pair.contains(&key) {
                        self.fsm = CanvasGestureState::DrainUntilAllUp;
                    }
                }
                _ => {}
            }
        }

        // Clean up when all active contacts have lifted
        let all_up = self.contacts.values().all(|c| c.up_ms.is_some());
        if all_up {
            self.fsm = CanvasGestureState::Idle;
            self.contacts.clear();
            self.nav_tracker = None;
            self.was_ever_drawing_in_cycle = false;
            self.candidate_pair = None;
            self.lifted_candidate_keys.clear();
            self.down_order.clear();
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn sample_down(id: u64, x: f32, y: f32, t: u64) -> PointerSample {
        PointerSample::new(id, PointerKind::Finger, PointerPhase::Down, pos2(x, y), t)
    }

    fn sample_move(id: u64, x: f32, y: f32, t: u64) -> PointerSample {
        PointerSample::new(id, PointerKind::Finger, PointerPhase::Move, pos2(x, y), t)
    }

    fn sample_up(id: u64, x: f32, y: f32, t: u64) -> PointerSample {
        PointerSample::new(id, PointerKind::Finger, PointerPhase::Up, pos2(x, y), t)
    }

    fn default_layout() -> LayoutSnapshot {
        LayoutSnapshot {
            canvas_rect: Rect::from_min_max(pos2(0.0, 50.0), pos2(400.0, 750.0)),
            ui_exclusion_rects: vec![
                Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 50.0)),    // top bar
                Rect::from_min_max(pos2(0.0, 750.0), pos2(400.0, 800.0)), // bottom nav
            ],
            sheet_handle_rect: None,
            modal_open: false,
        }
    }

    #[test]
    fn touch_on_ui_exclusion_zone_emits_no_canvas_actions() {
        let mut router = TouchRouter::new();
        let layout = default_layout();

        // Down on top bar (y = 20)
        let actions = router.process_sample(sample_down(1, 200.0, 20.0, 100), &layout);
        assert!(actions.is_empty());
        assert_eq!(router.contacts.get(&ContactKey::new(0, 1)).unwrap().owner, InputOwner::Ui);

        // Move to canvas area: still UI owner, no canvas actions
        let actions = router.process_sample(sample_move(1, 200.0, 200.0, 120), &layout);
        assert!(actions.is_empty());

        // Up: no canvas actions
        let actions = router.process_sample(sample_up(1, 200.0, 200.0, 150), &layout);
        assert!(actions.is_empty());
    }

    #[test]
    fn single_finger_tap_emits_dot_stroke() {
        let mut router = TouchRouter::new();
        let layout = default_layout();

        // Down on canvas (y = 200)
        let a1 = router.process_sample(sample_down(1, 100.0, 200.0, 100), &layout);
        assert_eq!(a1.len(), 2);
        assert!(matches!(a1[0], TouchAction::StartPendingStroke { .. }));

        // Quick release without significant movement (< 8 pt) and before 70 ms
        let a2 = router.process_sample(sample_up(1, 101.0, 201.0, 140), &layout);
        assert_eq!(a2.len(), 1);
        assert!(matches!(a2[0], TouchAction::DotStroke { .. }));
        assert_eq!(router.fsm, CanvasGestureState::Idle);
    }

    #[test]
    fn single_finger_stroke_exceeding_slop_transitions_to_drawing_and_commits_on_up() {
        let mut router = TouchRouter::new();
        let layout = default_layout();

        // Down on canvas
        let _ = router.process_sample(sample_down(1, 100.0, 200.0, 100), &layout);

        // Move > 6.0 pt
        let a = router.process_sample(sample_move(1, 110.0, 200.0, 120), &layout);
        assert_eq!(a.len(), 1);
        assert!(matches!(a[0], TouchAction::BeginLiveStroke { .. }));
        assert_eq!(router.fsm, CanvasGestureState::Drawing { pointer: ContactKey::new(0, 1) });

        // Feed move
        let a = router.process_sample(sample_move(1, 120.0, 200.0, 130), &layout);
        assert_eq!(a.len(), 1);
        assert!(matches!(a[0], TouchAction::FeedLiveStroke { .. }));

        // Up
        let a = router.process_sample(sample_up(1, 120.0, 200.0, 150), &layout);
        assert_eq!(a.len(), 1);
        assert!(matches!(a[0], TouchAction::CommitStroke { .. }));
        assert_eq!(router.fsm, CanvasGestureState::Idle);
    }

    #[test]
    fn second_finger_cancels_active_stroke_and_enables_undo_on_tap() {
        let mut router = TouchRouter::new();
        let layout = default_layout();

        // First finger down
        let _ = router.process_sample(sample_down(1, 100.0, 200.0, 100), &layout);

        // Second finger down at t = 150 (gap 50 ms <= 120 ms)
        let a = router.process_sample(sample_down(2, 200.0, 200.0, 150), &layout);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0], TouchAction::CancelStroke);
        assert!(matches!(router.fsm, CanvasGestureState::TwoFingerCandidate { .. }));

        // Both lift without movement
        let a1 = router.process_sample(sample_up(1, 100.0, 200.0, 200), &layout);
        assert!(a1.is_empty()); // 1 lifted, waiting for 2nd

        let a2 = router.process_sample(sample_up(2, 200.0, 200.0, 220), &layout);
        assert_eq!(a2.len(), 1);
        assert_eq!(a2[0], TouchAction::TwoFingerTapUndo);
        assert_eq!(router.fsm, CanvasGestureState::Idle);
    }

    #[test]
    fn second_finger_during_drawing_cancels_stroke_but_does_not_emit_undo_tap() {
        let mut router = TouchRouter::new();
        let layout = default_layout();

        // First finger down and moves > slop -> Drawing
        let _ = router.process_sample(sample_down(1, 100.0, 200.0, 100), &layout);
        let _ = router.process_sample(sample_move(1, 110.0, 200.0, 120), &layout);
        assert!(matches!(router.fsm, CanvasGestureState::Drawing { .. }));

        // Second finger down: must cancel the live stroke!
        let a = router.process_sample(sample_down(2, 200.0, 200.0, 150), &layout);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0], TouchAction::CancelStroke);

        // Release both: because Drawing was active, Undo MUST NOT be emitted!
        let _ = router.process_sample(sample_up(1, 110.0, 200.0, 200), &layout);
        let a2 = router.process_sample(sample_up(2, 200.0, 200.0, 220), &layout);
        assert!(a2.is_empty());
    }

    #[test]
    fn remaining_finger_after_navigation_does_not_draw() {
        let mut router = TouchRouter::new();
        let layout = default_layout();

        let _ = router.process_sample(sample_down(1, 100.0, 200.0, 100), &layout);
        let _ = router.process_sample(sample_down(2, 200.0, 200.0, 120), &layout);

        // Move > slop -> Navigating
        let _ = router.process_sample(sample_move(1, 120.0, 200.0, 140), &layout);
        assert!(matches!(router.fsm, CanvasGestureState::Navigating { .. }));

        // Finger 2 lifts
        let _ = router.process_sample(sample_up(2, 200.0, 200.0, 160), &layout);
        assert_eq!(router.fsm, CanvasGestureState::DrainUntilAllUp);

        // Remaining finger 1 moves: NO drawing actions
        let a = router.process_sample(sample_move(1, 150.0, 200.0, 180), &layout);
        assert!(a.is_empty());

        // Additional finger down: suppressed
        let a_down = router.process_sample(sample_down(3, 160.0, 200.0, 190), &layout);
        assert!(a_down.is_empty());
        assert_eq!(router.contacts.get(&ContactKey::new(0, 3)).unwrap().owner, InputOwner::Suppressed);

        // All fingers lift: returns to Idle
        let _ = router.process_sample(sample_up(1, 150.0, 200.0, 200), &layout);
        let _ = router.process_sample(sample_up(3, 160.0, 200.0, 210), &layout);
        assert_eq!(router.fsm, CanvasGestureState::Idle);
    }
}
