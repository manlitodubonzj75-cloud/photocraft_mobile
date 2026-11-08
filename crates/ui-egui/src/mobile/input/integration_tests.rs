//! Integration tests for mobile touch router, stroke transaction, and canvas arbitration.

use egui::{Rect, pos2};
use photocraft_engine::Session;
use serde_json::json;

use super::canvas_nav::TwoFingerNavTracker;
use super::pointer::{ContactKey, PointerKind, PointerPhase, PointerSample};
use super::router::{CanvasGestureState, LayoutSnapshot, TouchAction, TouchRouter};
use super::stroke_adapter::StrokeAdapter;
use crate::{PhotocraftApp, Services};

fn test_app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(Session::new(), Services::default());
    app.ui.mobile.enabled = true;
    app.ui.tool = crate::state::Tool::Brush;
    let _ = app.run("file.new", json!({"width": 200, "height": 200}));
    app.sync_views();
    if let Some(view) = app.ui.views.get_mut(0) {
        view.zoom = 1.0;
        view.center = [100.0, 100.0];
        view.fit_pending = false;
    }
    app.last_canvas_rect = Rect::from_min_max(pos2(0.0, 50.0), pos2(400.0, 750.0));
    app
}

fn test_layout() -> LayoutSnapshot {
    LayoutSnapshot {
        canvas_rect: Rect::from_min_max(pos2(0.0, 50.0), pos2(400.0, 750.0)),
        ui_exclusion_rects: vec![
            Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 50.0)),    // top bar
            Rect::from_min_max(pos2(0.0, 750.0), pos2(400.0, 800.0)), // bottom nav
            Rect::from_min_max(pos2(0.0, 500.0), pos2(400.0, 750.0)), // bottom sheet
        ],
        sheet_handle_rect: Some(Rect::from_min_max(pos2(0.0, 500.0), pos2(400.0, 548.0))),
        modal_open: false,
    }
}

#[test]
fn ac1_ui_interactions_produce_zero_canvas_actions_or_document_mutations() {
    let mut app = test_app();
    let mut router = TouchRouter::new();
    let mut adapter = StrokeAdapter::new();
    let layout = test_layout();
    let ctx = egui::Context::default();

    let initial_rev = app.session.active().unwrap().revision;
    let initial_journal_len = app.session.journal.len();

    // Touch on sheet handle (y = 520)
    let a1 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Down, pos2(200.0, 520.0), 100), &layout);
    adapter.apply_actions(&mut app, a1, &ctx);

    // Drag down to canvas area
    let a2 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Move, pos2(200.0, 300.0), 120), &layout);
    adapter.apply_actions(&mut app, a2, &ctx);

    // Release on canvas
    let a3 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Up, pos2(200.0, 300.0), 140), &layout);
    adapter.apply_actions(&mut app, a3, &ctx);

    assert_eq!(app.session.active().unwrap().revision, initial_rev);
    assert_eq!(app.session.journal.len(), initial_journal_len);
    assert!(app.drag.is_none());
    assert!(app.live_stroke.is_none());
}

#[test]
fn ac2_cross_boundary_canvas_to_ui_cancels_stroke_without_leak() {
    let mut app = test_app();
    let mut router = TouchRouter::new();
    let mut adapter = StrokeAdapter::new();
    let layout = test_layout();
    let ctx = egui::Context::default();

    let initial_rev = app.session.active().unwrap().revision;

    // Finger 1 down on canvas (y = 200)
    let a1 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Down, pos2(100.0, 200.0), 100), &layout);
    adapter.apply_actions(&mut app, a1, &ctx);

    // Move to Drawing
    let a2 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Move, pos2(120.0, 200.0), 120), &layout);
    adapter.apply_actions(&mut app, a2, &ctx);
    assert!(app.drag.is_some());

    // Finger 2 lands on UI top bar (y = 20)
    let a3 = router.process_sample(PointerSample::new(2, PointerKind::Finger, PointerPhase::Down, pos2(200.0, 20.0), 140), &layout);
    adapter.apply_actions(&mut app, a3, &ctx);

    // In-flight stroke must be completely cancelled!
    assert!(app.drag.is_none());
    assert!(app.live_stroke.is_none());
    assert_eq!(app.session.active().unwrap().revision, initial_rev);

    // Late up of finger 1: no commit
    let a4 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Up, pos2(120.0, 200.0), 160), &layout);
    adapter.apply_actions(&mut app, a4, &ctx);
    assert_eq!(app.session.active().unwrap().revision, initial_rev);
}

#[test]
fn ac3_single_tap_creates_one_commit_and_stroke_commits_once() {
    let mut app = test_app();
    let mut router = TouchRouter::new();
    let mut adapter = StrokeAdapter::new();
    let layout = test_layout();
    let ctx = egui::Context::default();

    let initial_rev = app.session.active().unwrap().revision;

    // Tap on canvas: Down + Up without moving > 6pt
    let a1 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Down, pos2(100.0, 200.0), 100), &layout);
    adapter.apply_actions(&mut app, a1, &ctx);

    let a2 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Up, pos2(101.0, 200.0), 130), &layout);
    adapter.apply_actions(&mut app, a2, &ctx);

    // Exactly one command executed!
    assert_eq!(app.session.active().unwrap().revision, initial_rev + 1);
    assert!(app.drag.is_none());
}

#[test]
fn ac4_second_finger_cancels_active_drawing_without_undoing_previous_history() {
    let mut app = test_app();
    let mut router = TouchRouter::new();
    let mut adapter = StrokeAdapter::new();
    let layout = test_layout();
    let ctx = egui::Context::default();

    // 1. Commit one real operation first so history is not empty
    app.run("paint.stroke", json!({"points": [[50.0, 50.0, 1.0]], "size": 10, "color": "#000000"})).unwrap();
    let committed_rev = app.session.active().unwrap().revision;
    let committed_journal_len = app.session.journal.len();

    // 2. Start a new finger stroke and draw > 6pt
    let a1 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Down, pos2(100.0, 200.0), 100), &layout);
    adapter.apply_actions(&mut app, a1, &ctx);

    let a2 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Move, pos2(120.0, 200.0), 120), &layout);
    adapter.apply_actions(&mut app, a2, &ctx);
    assert!(app.drag.is_some());

    // 3. Second finger touches down after 500 ms (well into drawing)
    let a3 = router.process_sample(PointerSample::new(2, PointerKind::Finger, PointerPhase::Down, pos2(200.0, 200.0), 620), &layout);
    adapter.apply_actions(&mut app, a3, &ctx);

    // Stroke preview is dropped
    assert!(app.drag.is_none());
    assert!(app.live_stroke.is_none());

    // Committed revision and journal are untouched (NOT undone, NOT corrupted)
    assert_eq!(app.session.active().unwrap().revision, committed_rev);
    assert_eq!(app.session.journal.len(), committed_journal_len);

    // Both lift
    let a4 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Up, pos2(120.0, 200.0), 700), &layout);
    adapter.apply_actions(&mut app, a4, &ctx);
    let a5 = router.process_sample(PointerSample::new(2, PointerKind::Finger, PointerPhase::Up, pos2(200.0, 200.0), 720), &layout);
    adapter.apply_actions(&mut app, a5, &ctx);

    // Still untouched: drawing was active, so 2-finger release does NOT trigger Undo
    assert_eq!(app.session.active().unwrap().revision, committed_rev);
    assert_eq!(app.session.journal.len(), committed_journal_len);
}

#[test]
fn ac5_two_finger_clean_tap_triggers_one_undo() {
    let mut app = test_app();
    let mut router = TouchRouter::new();
    let mut adapter = StrokeAdapter::new();
    let layout = test_layout();
    let ctx = egui::Context::default();

    // 1. Commit one operation
    app.run("paint.stroke", json!({"points": [[50.0, 50.0, 1.0]], "size": 10, "color": "#000000"})).unwrap();
    assert!(!app.session.active().unwrap().history.can_redo());
    let past_len = app.session.active().unwrap().history.past_len();

    // 2. Perform a clean 2-finger tap (both down within 50ms, released within 80ms, travel < 2pt)
    let a1 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Down, pos2(100.0, 200.0), 100), &layout);
    adapter.apply_actions(&mut app, a1, &ctx);

    let a2 = router.process_sample(PointerSample::new(2, PointerKind::Finger, PointerPhase::Down, pos2(180.0, 200.0), 120), &layout);
    adapter.apply_actions(&mut app, a2, &ctx);

    let a3 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Up, pos2(101.0, 200.0), 160), &layout);
    adapter.apply_actions(&mut app, a3, &ctx);

    let a4 = router.process_sample(PointerSample::new(2, PointerKind::Finger, PointerPhase::Up, pos2(181.0, 200.0), 170), &layout);
    adapter.apply_actions(&mut app, a4, &ctx);

    // Undo was executed!
    assert!(app.session.active().unwrap().history.can_redo());
    assert_eq!(app.session.active().unwrap().history.past_len(), past_len - 1);
}

#[test]
fn ac6_drain_until_all_up_locks_out_drawing_until_zero_fingers() {
    let mut router = TouchRouter::new();
    let layout = test_layout();

    // 2 fingers down and navigate
    let _ = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Down, pos2(100.0, 200.0), 100), &layout);
    let _ = router.process_sample(PointerSample::new(2, PointerKind::Finger, PointerPhase::Down, pos2(200.0, 200.0), 120), &layout);
    let _ = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Move, pos2(130.0, 200.0), 140), &layout);
    assert!(matches!(router.fsm, CanvasGestureState::Navigating { .. }));

    // Finger 2 lifts -> DrainUntilAllUp
    let _ = router.process_sample(PointerSample::new(2, PointerKind::Finger, PointerPhase::Up, pos2(200.0, 200.0), 160), &layout);
    assert_eq!(router.fsm, CanvasGestureState::DrainUntilAllUp);

    // Finger 1 continues to move: NO actions
    let a_move = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Move, pos2(180.0, 200.0), 180), &layout);
    assert!(a_move.is_empty());

    // Finger 3 touches down: suppressed
    let a_down = router.process_sample(PointerSample::new(3, PointerKind::Finger, PointerPhase::Down, pos2(220.0, 200.0), 200), &layout);
    assert!(a_down.is_empty());

    // Finger 1 lifts: still 1 finger (finger 3) on screen
    let _ = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Up, pos2(180.0, 200.0), 220), &layout);
    assert_eq!(router.fsm, CanvasGestureState::DrainUntilAllUp);

    // Finger 3 lifts: 0 fingers remaining -> Idle!
    let _ = router.process_sample(PointerSample::new(3, PointerKind::Finger, PointerPhase::Up, pos2(220.0, 200.0), 240), &layout);
    assert_eq!(router.fsm, CanvasGestureState::Idle);
}

#[test]
fn ac7_order_invariance_and_anchor_preservation() {
    let k1 = ContactKey::new(1, 7);
    let k2 = ContactKey::new(2, 42);

    let p1 = pos2(100.0, 100.0);
    let p2 = pos2(300.0, 100.0);

    let mut t1 = TwoFingerNavTracker::new(k1, p1, k2, p2);
    let mut t2 = TwoFingerNavTracker::new(k2, p2, k1, p1);

    let d1 = t1.update_contact(k1, pos2(120.0, 110.0)).unwrap();
    let d2 = t2.update_contact(k1, pos2(120.0, 110.0)).unwrap();

    assert_eq!(d1, d2);
    assert!((d1.translation.x - 10.0).abs() < 1e-4);
    assert!((d1.translation.y - 5.0).abs() < 1e-4);
}

#[test]
fn ac9_stylus_priority_cancels_finger_and_finger_does_not_revive() {
    let mut router = TouchRouter::new();
    let layout = test_layout();

    // Finger touches down
    let a1 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Down, pos2(100.0, 200.0), 100), &layout);
    assert!(matches!(a1[0], TouchAction::StartPendingStroke { .. }));

    // Stylus touches down while finger is pending
    let a2 = router.process_sample(PointerSample::new(2, PointerKind::Stylus, PointerPhase::Down, pos2(150.0, 250.0), 120), &layout);
    assert_eq!(a2[0], TouchAction::CancelStroke);
    assert!(matches!(a2[1], TouchAction::BeginLiveStroke { .. }));
    assert_eq!(router.fsm, CanvasGestureState::Drawing { pointer: ContactKey::new(0, 2) });

    // Stylus lifts
    let a3 = router.process_sample(PointerSample::new(2, PointerKind::Stylus, PointerPhase::Up, pos2(160.0, 260.0), 150), &layout);
    assert!(matches!(a3[0], TouchAction::CommitStroke { .. }));

    // Finger moves: suppressed, does not revive drawing!
    let a4 = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Move, pos2(120.0, 200.0), 160), &layout);
    assert!(a4.is_empty());
}

#[test]
fn ac10_focus_loss_or_cancel_cleans_up_all_state() {
    let mut router = TouchRouter::new();
    let layout = test_layout();

    let _ = router.process_sample(PointerSample::new(1, PointerKind::Finger, PointerPhase::Down, pos2(100.0, 200.0), 100), &layout);
    let a_cancel = router.cancel_all();
    assert_eq!(a_cancel.len(), 1);
    assert_eq!(a_cancel[0], TouchAction::CancelStroke);
    assert_eq!(router.fsm, CanvasGestureState::Idle);
    assert!(router.contacts.is_empty());
}
