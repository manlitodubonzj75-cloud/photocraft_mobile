//! Bridge connecting TouchRouter actions to PhotocraftApp session, live strokes, and view navigation.

use photocraft_doc::DocId;

use super::pointer::ContactKey;
use super::router::TouchAction;
use crate::PhotocraftApp;
use crate::canvas::drag::Drag;
use crate::canvas::gesture::finish_release;
use crate::canvas::strokes::{begin_live_stroke, feed_live_stroke};
use crate::canvas::view_xform::ViewXform;
use crate::state::Tool;

/// Lifecycle phase of a mobile stroke transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokePhase {
    /// In arbitration delay: buffering points without altering document or live preview.
    Pending,
    /// Live preview active: streaming points to engine/compositor.
    Previewing,
}

/// Transaction representing an in-flight mobile canvas stroke.
#[derive(Clone, Debug)]
pub struct StrokeTransaction {
    pub doc_id: DocId,
    pub base_revision: u64,
    pub tool: Tool,
    pub pointer: ContactKey,
    pub phase: StrokePhase,
    pub points: Vec<[f64; 3]>,
}

impl StrokeTransaction {
    pub fn new(doc_id: DocId, base_revision: u64, tool: Tool, pointer: ContactKey) -> Self {
        Self { doc_id, base_revision, tool, pointer, phase: StrokePhase::Pending, points: Vec::new() }
    }
}

/// Adapter maintaining stroke transactions and executing router actions on PhotocraftApp.
#[derive(Clone, Debug, Default)]
pub struct StrokeAdapter {
    pub transaction: Option<StrokeTransaction>,
}

impl StrokeAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether there is an active stroke transaction in progress.
    pub fn has_active_stroke(&self) -> bool {
        self.transaction.is_some()
    }

    /// Cancel in-flight stroke transaction and live preview.
    pub fn cancel_stroke(&mut self, app: &mut PhotocraftApp) {
        self.cancel_transaction(app);
    }

    /// Discard any live preview without modifying document, revision, or history.
    pub fn discard_preview(app: &mut PhotocraftApp) {
        app.drag = None;
        app.live_stroke = None;
        app.retouch_from_live = false;
        app.session.clear_live_stroke();
    }

    /// Cancel in-flight transaction and active preview.
    pub fn cancel_transaction(&mut self, app: &mut PhotocraftApp) {
        self.transaction = None;
        Self::discard_preview(app);
    }

    /// Apply an ordered sequence of router actions to the application state.
    pub fn apply_actions(&mut self, app: &mut PhotocraftApp, actions: Vec<TouchAction>, ctx: &egui::Context) {
        for action in actions {
            self.apply_action(app, action, ctx);
        }
    }

    /// Apply a single TouchAction to PhotocraftApp.
    pub fn apply_action(&mut self, app: &mut PhotocraftApp, action: TouchAction, ctx: &egui::Context) {
        match action {
            TouchAction::StartPendingStroke { key, pos, pressure } => {
                let Some(st) = app.session.active() else { return };
                let (doc_id, revision) = (st.doc.id, st.revision);
                Self::discard_preview(app);

                let doc_p = ViewXform::active(app).map_or([pos.x as f64, pos.y as f64], |xf| xf.to_doc(pos));
                let mut tx = StrokeTransaction::new(doc_id, revision, app.ui.tool, key);
                tx.points.push([doc_p[0], doc_p[1], pressure as f64]);
                self.transaction = Some(tx);
            }

            TouchAction::BeginLiveStroke { key, points } => {
                let Some(st) = app.session.active() else {
                    self.cancel_transaction(app);
                    return;
                };
                let Some(tx) = self.transaction.as_mut() else { return };
                if tx.pointer != key || tx.doc_id != st.doc.id || tx.base_revision != st.revision || tx.tool != app.ui.tool {
                    self.cancel_transaction(app);
                    return;
                }

                if let Some(xf) = ViewXform::active(app) {
                    for (p, pressure) in points {
                        let doc_p = xf.to_doc(p);
                        tx.points.push([doc_p[0], doc_p[1], pressure as f64]);
                    }
                }
                if tx.points.is_empty() {
                    return;
                }

                let start_pos = [tx.points[0][0], tx.points[0][1]];
                let drag = Drag::new(app.ui.tool, start_pos, tx.points.clone(), egui::Modifiers::NONE, false);
                app.drag = Some(drag);
                app.live_stroke = begin_live_stroke(app);
                feed_live_stroke(app);
                tx.phase = StrokePhase::Previewing;
            }

            TouchAction::FeedLiveStroke { key, pos, pressure } => {
                let Some(st) = app.session.active() else {
                    self.cancel_transaction(app);
                    return;
                };
                let Some(tx) = self.transaction.as_mut() else { return };
                if tx.pointer != key || tx.doc_id != st.doc.id || tx.base_revision != st.revision || tx.tool != app.ui.tool {
                    self.cancel_transaction(app);
                    return;
                }
                if tx.phase != StrokePhase::Previewing {
                    return;
                }

                let doc_p = ViewXform::active(app).map_or([pos.x as f64, pos.y as f64], |xf| xf.to_doc(pos));
                let pt = [doc_p[0], doc_p[1], pressure as f64];
                tx.points.push(pt);

                if let Some(drag) = app.drag.as_mut() {
                    drag.points.push(pt);
                    feed_live_stroke(app);
                }
            }

            TouchAction::CommitStroke { key, pos } => {
                let Some(st) = app.session.active() else {
                    self.cancel_transaction(app);
                    return;
                };
                let Some(tx) = self.transaction.take() else {
                    Self::discard_preview(app);
                    return;
                };
                if tx.pointer != key || tx.doc_id != st.doc.id || tx.base_revision != st.revision || tx.tool != app.ui.tool {
                    Self::discard_preview(app);
                    return;
                }

                if tx.phase == StrokePhase::Previewing {
                    let mut drag = app.drag.take();
                    if let (Some(d), Some(xf)) = (drag.as_mut(), ViewXform::active(app)) {
                        let doc_p = xf.to_doc(pos);
                        d.points.push([doc_p[0], doc_p[1], 1.0]);
                    }
                    if let Some(d) = drag {
                        finish_release(app, d, false);
                    }
                }
                Self::discard_preview(app);
            }

            TouchAction::DotStroke { key, pos, pressure } => {
                let Some(st) = app.session.active() else {
                    self.cancel_transaction(app);
                    return;
                };
                let Some(tx) = self.transaction.take() else {
                    Self::discard_preview(app);
                    return;
                };
                if tx.pointer != key || tx.doc_id != st.doc.id || tx.base_revision != st.revision || tx.tool != app.ui.tool {
                    Self::discard_preview(app);
                    return;
                }
                Self::discard_preview(app);

                let doc_p = ViewXform::active(app).map_or([pos.x as f64, pos.y as f64], |xf| xf.to_doc(pos));
                let pt = [doc_p[0], doc_p[1], pressure as f64];
                let drag = Drag::new(app.ui.tool, [doc_p[0], doc_p[1]], vec![pt], egui::Modifiers::NONE, false);
                finish_release(app, drag, true);
            }

            TouchAction::CancelStroke => {
                self.cancel_transaction(app);
            }

            TouchAction::Navigate(delta) => {
                let Some(active_idx) = app.session.active_index() else { return };
                let Some(st) = app.session.active() else { return };
                let doc_size = [st.doc.size.width, st.doc.size.height];
                let Some(xf) = ViewXform::active(app) else { return };
                let ppp = app.canvas_ppp();
                let focal_screen = delta.centroid;
                let focal_doc = xf.to_doc(focal_screen);

                if let Some(view) = app.ui.views.get_mut(active_idx) {
                    let target_zoom = crate::zoom_levels::clamp(view.zoom * delta.scale, doc_size);
                    view.zoom = target_zoom;
                    view.fit_pending = false;
                    view.zoom_animating = false;
                    view.zoom_transition = None;
                    view.zoom_anchor = None;

                    let new_screen_pos = focal_screen + delta.translation;
                    let offset_screen = new_screen_pos - xf.rect.center();
                    let offset_doc = xf.unmap_vec(offset_screen / (view.zoom / ppp));
                    let new_center = [(focal_doc[0] - offset_doc.x as f64) as f32, (focal_doc[1] - offset_doc.y as f64) as f32];
                    if new_center[0].is_finite() && new_center[1].is_finite() {
                        view.center = new_center;
                    }
                    view.display_camera = Some((view.zoom, view.center));
                }
            }

            TouchAction::TwoFingerTapUndo => {
                if app.session.is_enabled("edit.undo") {
                    let _ = app.run("edit.undo", serde_json::json!({}));
                }
            }

            TouchAction::RequestRepaintAfter(duration) => {
                ctx.request_repaint_after(duration);
            }
        }
    }
}
