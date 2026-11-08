//! Mobile application lifecycle FSM and effect reducer (§Stage 2).
//!
//! Tracks app-level lifecycle status without duplicating ownership of the native
//! `winit`/`eframe` window Surface. Handles transitions across suspend, resume,
//! surface resizing, device recreation, and recovery coordination.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleState {
    Active,
    Suspended,
    AwaitingNonZeroSize,
    RecoveringDevice,
    GraphicsUnavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleEvent {
    Suspend,
    Resume,
    SurfaceResized { width: u32, height: u32 },
    SurfaceLost,
    SurfaceOutdated,
    SurfaceTimeout,
    DeviceLost { reason: String },
    DeviceRestored,
    GraphicsFatalError(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleEffect {
    /// Cancel in-flight stroke previews and clear active touches on suspend.
    CancelActiveStrokeAndTouches,
    /// Bounded flush of dirty documents to crash recovery.
    FlushAutosaves,
    /// Surface needs reconfiguring with non-zero dimensions.
    ReconfigureSurface { width: u32, height: u32 },
    /// Invalidate device-bound textures/buffers/bind groups when device generation changes.
    InvalidateDeviceResources { device_generation: u64 },
    /// Ask the UI host for a redraw.
    RequestRepaint,
    /// Skip this frame without busy-looping (e.g. on surface timeout).
    SkipFrame,
}

/// Token identifying a device-bound GPU job / resource to verify device generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuJobToken {
    pub device_generation: u64,
    pub doc_id: u64,
    pub revision: u64,
}

impl GpuJobToken {
    #[must_use]
    pub fn new(device_generation: u64, doc_id: u64, revision: u64) -> Self {
        Self { device_generation, doc_id, revision }
    }

    /// True if the token belongs to the given device generation.
    #[must_use]
    pub fn is_valid_for(&self, current_device_gen: u64) -> bool {
        self.device_generation == current_device_gen
    }
}

/// Tracks the application lifecycle state and manages window & GPU device generations.
#[derive(Debug, Clone)]
pub struct LifecycleTracker {
    state: LifecycleState,
    window_generation: u64,
    device_generation: u64,
    last_size: (u32, u32),
}

impl Default for LifecycleTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleTracker {
    #[must_use]
    pub fn new() -> Self {
        Self { state: LifecycleState::AwaitingNonZeroSize, window_generation: 0, device_generation: 0, last_size: (0, 0) }
    }

    #[must_use]
    pub fn with_size(width: u32, height: u32) -> Self {
        let (state, win_gen) = if width > 0 && height > 0 { (LifecycleState::Active, 1) } else { (LifecycleState::AwaitingNonZeroSize, 0) };
        Self { state, window_generation: win_gen, device_generation: 0, last_size: (width, height) }
    }

    #[must_use]
    pub fn state(&self) -> &LifecycleState {
        &self.state
    }

    #[must_use]
    pub fn window_generation(&self) -> u64 {
        self.window_generation
    }

    #[must_use]
    pub fn device_generation(&self) -> u64 {
        self.device_generation
    }

    #[must_use]
    pub fn last_size(&self) -> (u32, u32) {
        self.last_size
    }

    /// Pure reducer transitioning state and returning required effects.
    pub fn reduce(&mut self, event: LifecycleEvent) -> Vec<LifecycleEffect> {
        let mut effects = Vec::new();
        match event {
            LifecycleEvent::Suspend => {
                // Idempotent: duplicate suspend does not re-emit effects or alter state.
                if self.state == LifecycleState::Suspended {
                    return effects;
                }
                self.state = LifecycleState::Suspended;
                effects.push(LifecycleEffect::CancelActiveStrokeAndTouches);
                effects.push(LifecycleEffect::FlushAutosaves);
            }
            LifecycleEvent::Resume => {
                // Idempotent: duplicate resume when active does nothing.
                if self.state == LifecycleState::Active {
                    return effects;
                }
                if self.last_size.0 == 0 || self.last_size.1 == 0 {
                    self.state = LifecycleState::AwaitingNonZeroSize;
                } else {
                    self.state = LifecycleState::Active;
                    effects.push(LifecycleEffect::RequestRepaint);
                }
            }
            LifecycleEvent::SurfaceResized { width, height } => {
                self.last_size = (width, height);
                if width == 0 || height == 0 {
                    if self.state == LifecycleState::Active {
                        self.state = LifecycleState::AwaitingNonZeroSize;
                    }
                } else {
                    self.window_generation = self.window_generation.saturating_add(1);
                    match self.state {
                        LifecycleState::AwaitingNonZeroSize => {
                            self.state = LifecycleState::Active;
                            effects.push(LifecycleEffect::ReconfigureSurface { width, height });
                            effects.push(LifecycleEffect::RequestRepaint);
                        }
                        LifecycleState::Active => {
                            effects.push(LifecycleEffect::ReconfigureSurface { width, height });
                            effects.push(LifecycleEffect::RequestRepaint);
                        }
                        LifecycleState::Suspended | LifecycleState::RecoveringDevice | LifecycleState::GraphicsUnavailable { .. } => {
                            // Dimensions updated, but state remains inactive until resume or recovery.
                        }
                    }
                }
            }
            LifecycleEvent::SurfaceLost => {
                self.window_generation = self.window_generation.saturating_add(1);
                if self.state == LifecycleState::Active {
                    effects.push(LifecycleEffect::RequestRepaint);
                }
            }
            LifecycleEvent::SurfaceOutdated => {
                if self.state == LifecycleState::Active && self.last_size.0 > 0 && self.last_size.1 > 0 {
                    effects.push(LifecycleEffect::ReconfigureSurface { width: self.last_size.0, height: self.last_size.1 });
                    effects.push(LifecycleEffect::RequestRepaint);
                }
            }
            LifecycleEvent::SurfaceTimeout => {
                // Drop frame without spinning.
                effects.push(LifecycleEffect::SkipFrame);
            }
            LifecycleEvent::DeviceLost { reason } => {
                self.device_generation = self.device_generation.saturating_add(1);
                self.state = LifecycleState::RecoveringDevice;
                effects.push(LifecycleEffect::InvalidateDeviceResources { device_generation: self.device_generation });
                effects.push(LifecycleEffect::FlushAutosaves);
                effects.push(LifecycleEffect::CancelActiveStrokeAndTouches);
                log::warn!("Device lost: {reason}, device generation incremented to {}", self.device_generation);
            }
            LifecycleEvent::DeviceRestored => {
                if self.state == LifecycleState::RecoveringDevice {
                    if self.last_size.0 > 0 && self.last_size.1 > 0 {
                        self.state = LifecycleState::Active;
                        effects.push(LifecycleEffect::RequestRepaint);
                    } else {
                        self.state = LifecycleState::AwaitingNonZeroSize;
                    }
                }
            }
            LifecycleEvent::GraphicsFatalError(reason) => {
                self.state = LifecycleState::GraphicsUnavailable { reason };
                effects.push(LifecycleEffect::CancelActiveStrokeAndTouches);
                effects.push(LifecycleEffect::FlushAutosaves);
            }
        }
        effects
    }

    /// Apply lifecycle effects directly to `PhotocraftApp`.
    pub fn apply_to_app(&mut self, app: &mut crate::PhotocraftApp, event: LifecycleEvent, ctx: Option<&egui::Context>) {
        let effects = self.reduce(event);
        for effect in effects {
            match effect {
                LifecycleEffect::CancelActiveStrokeAndTouches => {
                    app.stroke_adapter.transaction = None;
                    crate::mobile::input::StrokeAdapter::discard_preview(app);
                    app.touch_router.reset();
                }
                LifecycleEffect::FlushAutosaves => {
                    if let Some(flush) = app.services.flush_autosaves.as_mut() {
                        let _ = flush();
                    }
                }
                LifecycleEffect::InvalidateDeviceResources { .. } => {
                    // Cached GPU canvases / textures / bind groups must be rebuilt on new device.
                    app.canvases.clear();
                }
                LifecycleEffect::RequestRepaint => {
                    if let Some(ctx) = ctx {
                        ctx.request_repaint();
                    }
                }
                LifecycleEffect::ReconfigureSurface { .. } | LifecycleEffect::SkipFrame => {
                    // Handled by eframe/renderer.
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PhotocraftApp;
    use crate::mobile::input::pointer::{PointerKind, PointerPhase, PointerSample};
    use crate::mobile::input::router::{CanvasGestureState, LayoutSnapshot};
    use crate::mobile::input::stroke_adapter::StrokeAdapter;
    use crate::state::Tool;
    use photocraft_color::{Color, ColorMode, SampleType};
    use photocraft_doc::{Document, Size};
    use photocraft_engine::Session;
    use photocraft_format::{RecoveryStore, list_recovery, recover};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn make_test_app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(Session::new(), Default::default());
        app.ui.mobile.enabled = true;
        let mut doc = Document::with_background("TestDoc", Size::new(800, 600), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        doc.layers[0].surface_mut().unwrap().write_pixel(10, 10, &[1.0, 0.0, 0.0, 1.0]);
        app.session.open_document(doc, None);
        app.ui.tool = Tool::Brush;
        app
    }

    fn test_layout() -> LayoutSnapshot {
        LayoutSnapshot {
            canvas_rect: egui::Rect::from_min_max(egui::pos2(0.0, 50.0), egui::pos2(400.0, 750.0)),
            ui_exclusion_rects: vec![
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 50.0)),
                egui::Rect::from_min_max(egui::pos2(0.0, 750.0), egui::pos2(400.0, 800.0)),
            ],
            sheet_handle_rect: None,
            modal_open: false,
        }
    }

    #[test]
    fn test_duplicate_suspend_resume_idempotent() {
        let mut tracker = LifecycleTracker::with_size(1080, 1920);
        assert_eq!(*tracker.state(), LifecycleState::Active);

        // Suspend
        let eff1 = tracker.reduce(LifecycleEvent::Suspend);
        assert_eq!(*tracker.state(), LifecycleState::Suspended);
        assert_eq!(eff1, vec![LifecycleEffect::CancelActiveStrokeAndTouches, LifecycleEffect::FlushAutosaves,]);

        // Duplicate suspend is a no-op
        let eff2 = tracker.reduce(LifecycleEvent::Suspend);
        assert_eq!(*tracker.state(), LifecycleState::Suspended);
        assert!(eff2.is_empty(), "duplicate suspend must emit zero effects");

        // Resume
        let eff3 = tracker.reduce(LifecycleEvent::Resume);
        assert_eq!(*tracker.state(), LifecycleState::Active);
        assert_eq!(eff3, vec![LifecycleEffect::RequestRepaint]);

        // Duplicate resume is a no-op
        let eff4 = tracker.reduce(LifecycleEvent::Resume);
        assert_eq!(*tracker.state(), LifecycleState::Active);
        assert!(eff4.is_empty(), "duplicate resume must emit zero effects");
    }

    #[test]
    fn test_resume_with_zero_size_awaits_nonzero_resize() {
        let mut tracker = LifecycleTracker::new();
        assert_eq!(*tracker.state(), LifecycleState::AwaitingNonZeroSize);

        // Resume with (0, 0)
        let eff = tracker.reduce(LifecycleEvent::Resume);
        assert_eq!(*tracker.state(), LifecycleState::AwaitingNonZeroSize);
        assert!(eff.is_empty(), "must not request repaint when size is zero");

        // Non-zero resize activates and requests repaint
        let eff = tracker.reduce(LifecycleEvent::SurfaceResized { width: 1080, height: 2400 });
        assert_eq!(*tracker.state(), LifecycleState::Active);
        assert_eq!(eff, vec![LifecycleEffect::ReconfigureSurface { width: 1080, height: 2400 }, LifecycleEffect::RequestRepaint,]);
        assert_eq!(tracker.window_generation(), 1);

        // Resize back to 0×0 drops back to AwaitingNonZeroSize without spin
        let eff = tracker.reduce(LifecycleEvent::SurfaceResized { width: 0, height: 0 });
        assert_eq!(*tracker.state(), LifecycleState::AwaitingNonZeroSize);
        assert!(eff.is_empty());
    }

    #[test]
    fn test_suspend_in_all_input_states_cancels_cleanly_zero_commits() {
        let mut app = make_test_app();
        let initial_rev = app.session.active().unwrap().revision;
        let layout = test_layout();
        let ctx = egui::Context::default();
        let mut adapter = StrokeAdapter::new();

        // 1. Drawing state
        let s_down = PointerSample::new(10, PointerKind::Finger, PointerPhase::Down, egui::pos2(100.0, 100.0), 0);
        let a1 = app.touch_router.process_sample(s_down, &layout);
        adapter.apply_actions(&mut app, a1, &ctx);

        let s_move = PointerSample::new(10, PointerKind::Finger, PointerPhase::Move, egui::pos2(120.0, 120.0), 20);
        let a2 = app.touch_router.process_sample(s_move, &layout);
        adapter.apply_actions(&mut app, a2, &ctx);

        app.stroke_adapter = adapter.clone();
        assert!(matches!(app.touch_router.state(), CanvasGestureState::Drawing { .. }));
        assert!(app.stroke_adapter.has_active_stroke());

        // Suspend
        let mut tracker = LifecycleTracker::with_size(800, 600);
        tracker.apply_to_app(&mut app, LifecycleEvent::Suspend, None);
        adapter = app.stroke_adapter.clone();

        // Verifications: 0 commits, active stroke cancelled, router idle, session intact
        assert_eq!(app.session.active().unwrap().revision, initial_rev, "suspend must make ZERO commits");
        assert!(!app.stroke_adapter.has_active_stroke(), "stroke preview must be cancelled");
        assert_eq!(*app.touch_router.state(), CanvasGestureState::Idle);
        assert_eq!(*tracker.state(), LifecycleState::Suspended);

        // Resume before next test case
        tracker.apply_to_app(&mut app, LifecycleEvent::Resume, None);
        assert_eq!(*tracker.state(), LifecycleState::Active);

        // 2. TwoFingerCandidate state
        let s_down1 = PointerSample::new(20, PointerKind::Finger, PointerPhase::Down, egui::pos2(200.0, 200.0), 100);
        let a1 = app.touch_router.process_sample(s_down1, &layout);
        adapter.apply_actions(&mut app, a1, &ctx);

        let s_down2 = PointerSample::new(21, PointerKind::Finger, PointerPhase::Down, egui::pos2(250.0, 200.0), 120);
        let a2 = app.touch_router.process_sample(s_down2, &layout);
        adapter.apply_actions(&mut app, a2, &ctx);

        app.stroke_adapter = adapter.clone();
        assert!(matches!(app.touch_router.state(), CanvasGestureState::TwoFingerCandidate { .. }));

        tracker.apply_to_app(&mut app, LifecycleEvent::Suspend, None);
        adapter = app.stroke_adapter.clone();
        assert_eq!(*app.touch_router.state(), CanvasGestureState::Idle);
        assert_eq!(app.session.active().unwrap().revision, initial_rev);

        // Resume before next test case
        tracker.apply_to_app(&mut app, LifecycleEvent::Resume, None);
        assert_eq!(*tracker.state(), LifecycleState::Active);

        // 3. Navigating state
        let s_down1 = PointerSample::new(30, PointerKind::Finger, PointerPhase::Down, egui::pos2(200.0, 200.0), 200);
        let a1 = app.touch_router.process_sample(s_down1, &layout);
        adapter.apply_actions(&mut app, a1, &ctx);

        let s_down2 = PointerSample::new(31, PointerKind::Finger, PointerPhase::Down, egui::pos2(250.0, 200.0), 220);
        let a2 = app.touch_router.process_sample(s_down2, &layout);
        adapter.apply_actions(&mut app, a2, &ctx);

        let s_move1 = PointerSample::new(30, PointerKind::Finger, PointerPhase::Move, egui::pos2(220.0, 220.0), 240);
        let a3 = app.touch_router.process_sample(s_move1, &layout);
        adapter.apply_actions(&mut app, a3, &ctx);

        app.stroke_adapter = adapter.clone();
        assert!(matches!(app.touch_router.state(), CanvasGestureState::Navigating { .. }));

        tracker.apply_to_app(&mut app, LifecycleEvent::Suspend, None);
        assert_eq!(*app.touch_router.state(), CanvasGestureState::Idle);
        assert_eq!(app.session.active().unwrap().revision, initial_rev);
    }

    #[test]
    fn test_recovery_store_committed_checkpoint_and_resilient_to_failures() {
        static CTR: AtomicUsize = AtomicUsize::new(0);
        let temp_dir = std::env::temp_dir().join(format!("photocraft-mobile-recovery-test-{}", CTR.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut doc = Document::with_background("MobileArt", Size::new(16, 16), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        doc.layers[0].surface_mut().unwrap().write_pixel(5, 5, &[0.0, 1.0, 0.0, 1.0]);

        // 1. Initial committed checkpoint
        let mut store = RecoveryStore::new(&temp_dir);
        let doc_arc = Arc::new(doc.clone());
        let res = store.autosave_checked(&doc_arc, 1, Some(temp_dir.join("MobileArt.pcraft").to_string_lossy().into_owned()));
        assert!(res.is_ok());

        let completed = store.flush();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].0, doc.id.0);
        assert_eq!(completed[0].1, 1);
        assert!(completed[0].2.is_ok());

        // Verify recovery lists the valid checkpoint
        let entries = list_recovery(&temp_dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].info.document_name, "MobileArt");
        let restored = recover(&entries[0]).unwrap();
        assert_eq!(restored.layers[0].surface().unwrap().pixel(5, 5), vec![0.0, 1.0, 0.0, 1.0]);

        // 2. Corrupted snapshot attempt must NOT destroy the previous completed checkpoint
        let dummy_corrupt_bundle = temp_dir.join("doc-corrupted.pcraft");
        std::fs::create_dir_all(&dummy_corrupt_bundle).unwrap();
        std::fs::write(dummy_corrupt_bundle.join("manifest.json"), b"invalid json").unwrap();

        // Valid checkpoint is still intact and loadable
        let entries_after = list_recovery(&temp_dir);
        assert!(entries_after.iter().any(|e| e.info.key == entries[0].info.key));
        let recovered_again = recover(&entries[0]).unwrap();
        assert_eq!(recovered_again.layers[0].surface().unwrap().pixel(5, 5), vec![0.0, 1.0, 0.0, 1.0]);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_surface_and_device_generation_separation() {
        let mut tracker = LifecycleTracker::with_size(1080, 1920);
        assert_eq!(tracker.window_generation(), 1);
        assert_eq!(tracker.device_generation(), 0);

        let job_token = GpuJobToken::new(tracker.device_generation(), 42, 1);
        assert!(job_token.is_valid_for(tracker.device_generation()));

        // Surface recreate / reset increments window_generation ONLY
        tracker.reduce(LifecycleEvent::SurfaceLost);
        assert_eq!(tracker.window_generation(), 2);
        assert_eq!(tracker.device_generation(), 0);
        assert!(job_token.is_valid_for(tracker.device_generation()), "surface reset must NOT invalidate device token");

        // Device lost increments device_generation
        let eff = tracker.reduce(LifecycleEvent::DeviceLost { reason: "GPU driver crash".into() });
        assert_eq!(tracker.device_generation(), 1);
        assert_eq!(*tracker.state(), LifecycleState::RecoveringDevice);
        assert!(eff.contains(&LifecycleEffect::InvalidateDeviceResources { device_generation: 1 }), "device lost must invalidate device resources");
        assert!(!job_token.is_valid_for(tracker.device_generation()), "old GPU job token must be rejected after device generation changes");

        // Device restore
        let eff = tracker.reduce(LifecycleEvent::DeviceRestored);
        assert_eq!(*tracker.state(), LifecycleState::Active);
        assert_eq!(eff, vec![LifecycleEffect::RequestRepaint]);
    }
}
