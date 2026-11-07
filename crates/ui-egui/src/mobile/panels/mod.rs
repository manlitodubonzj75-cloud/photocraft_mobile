//! Hosting the existing inspector panels in the mobile shell's sheet (phone) or dock (tablet).
//!
//! Applies touch ergonomics (`touch_style`) and interactive drag snapping to the bottom sheet.

pub mod touch_style;

use egui::{CornerRadius, Rect, Sense, Stroke, pos2, vec2};

use crate::dock::Group;
use crate::mobile::shell::metrics;
use crate::theme::{self, Tokens};
use crate::{PhotocraftApp, icons};
pub use touch_style::{PanelPresentation, apply_touch_style, with_touch_style};

/// The localized tab labels of a group, in the group's own order.
pub fn group_tabs(group: Group, pro: bool) -> Vec<&'static str> {
    group.tabs(pro).iter().map(|n| tl!(n)).collect()
}

/// Draw one panel body from the shared registry with touch-friendly widget spacing.
fn panel_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, group: Group, tab: usize) {
    with_touch_style(ui, |ui| {
        if group.scrolls_itself(tab) {
            ui.set_min_height(ui.available_height().max(0.0));
            crate::panels::dock_body(app, ui, group, tab);
        } else {
            let pro = Tokens::get(ui.ctx()).pro;
            crate::panels::offer_color_field_fill(ui, pro, group, tab);
            ui.spacing_mut().scroll.fade.strength = 0.0;
            egui::ScrollArea::vertical()
                .id_salt(("mobile-panel-body", group.key(), tab))
                .auto_shrink([false, false])
                .show(ui, |ui| crate::panels::dock_body(app, ui, group, tab));
        }
    });
}

/// Where the shell is drawing a panel host. Kept as a parameter rather than a global so the same
/// code serves the phone sheet and the tablet dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    BottomSheet,
    RightDock,
}

/// Resolve the snap point for a given sheet fraction upon release:
/// - Under 0.25: closes sheet (returns `None`).
/// - Between 0.25 and 0.65: snaps to standard half-height (`0.45`).
/// - Above 0.65: expands to full sheet (`0.85`).
pub fn resolve_sheet_snap(fraction: f32) -> Option<f32> {
    if fraction < 0.25 {
        None
    } else if fraction < 0.65 {
        Some(0.45)
    } else {
        Some(0.85)
    }
}

/// The frame the host draws around the panel: a rounded sheet pulled up from the bottom, or a flat
/// dock at the side. The sheet gets an interactive grab handle that can be dragged to resize or snap.
fn draw_host_frame(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect, kind: HostKind, t: &Tokens) {
    let radius = match kind {
        HostKind::BottomSheet => CornerRadius { nw: t.radius_lg as u8, ne: t.radius_lg as u8, sw: 0, se: 0 },
        HostKind::RightDock => CornerRadius::ZERO,
    };
    ui.painter().rect_filled(rect, radius, t.dock);
    let edge = match kind {
        HostKind::BottomSheet => [pos2(rect.left(), rect.top()), pos2(rect.right(), rect.top())],
        HostKind::RightDock => [pos2(rect.left(), rect.top()), pos2(rect.left(), rect.bottom())],
    };
    ui.painter().line_segment(edge, Stroke::new(1.0, t.separator));

    if kind == HostKind::BottomSheet {
        // Interactive drag handle area: 44pt touch strip at the top of the sheet
        let handle_strip = Rect::from_min_max(pos2(rect.left(), rect.top()), pos2(rect.right(), rect.top() + 28.0));
        let handle_resp = ui.interact(handle_strip, egui::Id::unique("sheet-drag-handle"), Sense::click_and_drag());

        let bar_w = 44.0;
        let bar_h = 5.0;
        let handle = Rect::from_center_size(pos2(rect.center().x, rect.top() + 10.0), vec2(bar_w, bar_h));
        let bar_color = if handle_resp.hovered() || handle_resp.dragged() { t.text } else { t.text_faint };
        ui.painter().rect_filled(handle, CornerRadius { nw: 2, ne: 2, sw: 2, se: 2 }, bar_color);

        // Handle dragging and snap-points (0% close, 45% half, 85% full)
        if handle_resp.dragged() {
            let dy = handle_resp.drag_delta().y;
            let screen_h = ui.ctx().content_rect().height().max(1.0);
            let df = -dy / screen_h;
            app.ui.mobile.sheet_fraction = (app.ui.mobile.sheet_fraction + df).clamp(0.10, 0.90);
        }
        if handle_resp.drag_stopped() {
            match resolve_sheet_snap(app.ui.mobile.sheet_fraction) {
                Some(snap) => app.ui.mobile.sheet_fraction = snap,
                None => {
                    app.ui.mobile.hide();
                    app.ui.mobile.sheet_fraction = 0.45;
                }
            }
        }
    }
}

/// Draw the sheet/dock host for `group` into `rect`, with a tab strip and the shared panel body.
pub fn panel_host(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect, kind: HostKind, group: Group) {
    if rect.width() < 8.0 || rect.height() < 8.0 {
        return;
    }
    let t = Tokens::get(ui.ctx());
    draw_host_frame(app, ui, rect, kind, &t);

    let inner = match kind {
        HostKind::BottomSheet => rect.shrink2(vec2(0.0, 0.0)).translate(vec2(0.0, 12.0)),
        HostKind::RightDock => rect,
    };
    if inner.width() < 8.0 || inner.height() < 8.0 {
        return;
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));

    let pro = t.pro;
    let tabs = group_tabs(group, pro);
    let strip_h = metrics::TOP_BAR_HEIGHT.min(inner.height());
    let strip = Rect::from_min_size(inner.min, vec2(inner.width() - 44.0, strip_h));
    let sel = group.tab_mut(&mut app.ui.dock_tabs);
    let before = *sel;
    crate::widgets::tab_strip::pill_tabs(&mut child, egui::Id::unique(group.key()), strip, &tabs, sel);
    let picked = *sel != before;
    let used = *group.tab_mut(&mut app.ui.dock_tabs);

    // Close button at top right
    let close_rect = Rect::from_min_max(pos2(inner.right() - 44.0, inner.top()), pos2(inner.right(), inner.top() + strip_h));
    if host_close_button(&mut child, close_rect) {
        app.ui.mobile.hide();
    }

    let body_rect = Rect::from_min_max(pos2(inner.left(), strip.bottom()), pos2(inner.right(), inner.bottom()));
    if body_rect.height() > 1.0 {
        let mut body = child.new_child(egui::UiBuilder::new().max_rect(body_rect));
        body.set_clip_rect(body_rect.intersect(ui.clip_rect()));
        panel_body(app, &mut body, group, used);
    }
    if picked {
        ui.ctx().request_repaint();
    }
}

/// The close affordance for the sheet/dock header, drawn at the host's trailing edge. Returns
/// `true` when tapped. Uses the shared `x` icon so it matches the rest of the chrome.
pub fn host_close_button(ui: &mut egui::Ui, rect: Rect) -> bool {
    let t = Tokens::get(ui.ctx());
    let r = Rect::from_center_size(rect.center(), vec2(36.0, 36.0));
    let resp = ui.interact(r, egui::Id::unique("mobile-host-close"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius { nw: 6, ne: 6, sw: 6, se: 6 }, t.hover);
    }
    icons::paint(ui, r, "x", 18.0, if resp.hovered() { t.text } else { t.text_dim });
    resp.clicked()
}

/// A titled host header: the group's rail icon, the group's name, and a close button at the
/// trailing edge. Returns `true` when the close button was tapped.
pub fn host_header(ui: &mut egui::Ui, rect: Rect, group: Group, title: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    if rect.height() < 8.0 {
        return false;
    }
    let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(rect));
    ui.set_clip_rect(rect.intersect(ui.clip_rect()));
    let icon_r = Rect::from_center_size(rect.left_center() + vec2(24.0, 0.0), vec2(16.0, 16.0));
    icons::paint(ui, icon_r, group_rail_icon(group), 16.0, t.text_dim);
    let label_r = Rect::from_min_max(pos2(icon_r.right() + 8.0, rect.top()), pos2(rect.right() - 40.0, rect.bottom()));
    if label_r.width() > 1.0 {
        let galley = crate::widgets::tab_strip::elided(ui, title, theme::medium(13.0), t.text, label_r.width());
        ui.painter().galley(label_r.left_center(), galley, t.text);
    }
    host_close_button(ui, rect)
}

/// The icon the desktop rail shows for a group (`crates/ui-egui/src/panels/dock.rs:49`).
pub fn group_rail_icon(group: Group) -> &'static str {
    match group {
        Group::Properties => "sliders-horizontal",
        Group::Navigator => "navigation",
        Group::Color => "palette",
        Group::Layers => "layers",
        Group::History => "clock",
        Group::Brushes => "brush",
        Group::Character => "type",
    }
}

/// The shell's label for a panel group (the desktop rail's wording).
pub fn group_label(group: Group) -> &'static str {
    match group {
        Group::Color => tl!("Color & Swatches"),
        Group::Properties => tl!("Properties"),
        Group::Character => tl!("Character"),
        Group::Navigator => tl!("Navigator"),
        Group::History => tl!("History"),
        Group::Layers => tl!("Layers"),
        Group::Brushes => tl!("Brushes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sheet_snap_points() {
        // Dragged down near bottom closes sheet
        assert_eq!(resolve_sheet_snap(0.15), None);
        assert_eq!(resolve_sheet_snap(0.24), None);

        // Released in middle snaps to half-height
        assert_eq!(resolve_sheet_snap(0.25), Some(0.45));
        assert_eq!(resolve_sheet_snap(0.50), Some(0.45));
        assert_eq!(resolve_sheet_snap(0.64), Some(0.45));

        // Released near top snaps to full height
        assert_eq!(resolve_sheet_snap(0.65), Some(0.85));
        assert_eq!(resolve_sheet_snap(0.90), Some(0.85));
    }

    #[test]
    fn test_presentation_strategies() {
        assert_eq!(PanelPresentation::for_group(Group::Layers, 0), PanelPresentation::MobileSpecialized);
        assert_eq!(PanelPresentation::for_group(Group::Color, 0), PanelPresentation::MobileSpecialized);
        assert_eq!(PanelPresentation::for_group(Group::History, 0), PanelPresentation::AdaptiveLegacy);
    }
}
