//! Hosting the existing inspector panels in the mobile shell's sheet (phone) or dock (tablet).
//!
//! This is deliberately thin. It draws **nothing** of its own beyond the touch-sized tab strip and
//! the sheet/dock frame: every panel body is the same `crate::panels::dock_body` the desktop dock
//! calls (`crates/ui-egui/src/panels/dock.rs:98`), dispatched with the same
//! [`crate::dock::Group`]/tab pair. Reusing the dispatch means a panel that works on the desktop
//! works here with no parallel implementation and nothing silently dropped: if `dock_body` learns a
//! new tab, the mobile host gets it for free.
//!
//! The only genuinely new code is the *host*: a bounded region (a bottom sheet on phones, a side
//! dock on tablets) with a scroll policy that matches `panel_docking::body`
//! (`crates/ui-egui/src/panels/panel_docking.rs:826`), so a panel behaves identically whether it is
//! shown in the desktop dock or the mobile sheet.

use egui::{CornerRadius, Rect, Sense, Stroke, pos2, vec2};

use crate::dock::Group;
use crate::mobile::shell::metrics;
use crate::theme::{self, Tokens};
use crate::{PhotocraftApp, icons};

/// The localized tab labels of a group, in the group's own order.
///
/// Reads [`Group::tabs`] so the tab list (and the Color/Swatches order that swaps with the theme)
/// stays exactly the desktop's; only the labels are translated, as the desktop tab strip does.
pub fn group_tabs(group: Group, pro: bool) -> Vec<&'static str> {
    group.tabs(pro).iter().map(|n| tl!(n)).collect()
}

/// Draw one panel body from the shared registry, bounded to `ui` and scrolled like the desktop
/// dock does. `group`/`tab` index into the same dispatch the desktop uses.
fn panel_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, group: Group, tab: usize) {
    if group.scrolls_itself(tab) {
        ui.set_min_height(ui.available_height().max(0.0));
        crate::panels::dock_body(app, ui, group, tab);
    } else {
        let pro = Tokens::get(ui.ctx()).pro;
        crate::panels::offer_color_field_fill(ui, pro, group, tab);
        // Match the desktop: no scroll fade, since it darkens real colours and previews.
        ui.spacing_mut().scroll.fade.strength = 0.0;
        egui::ScrollArea::vertical()
            .id_salt(("mobile-panel-body", group.key(), tab))
            .auto_shrink([false, false])
            .show(ui, |ui| crate::panels::dock_body(app, ui, group, tab));
    }
}

/// Where the shell is drawing a panel host. Kept as a parameter rather than a global so the same
/// code serves the phone sheet and the tablet dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    BottomSheet,
    RightDock,
}

/// The frame the host draws around the panel: a rounded sheet pulled up from the bottom, or a flat
/// dock at the side. The sheet gets a grab handle so its drag affordance is obvious.
fn draw_host_frame(ui: &egui::Ui, rect: Rect, kind: HostKind, t: &Tokens) {
    let radius = match kind {
        HostKind::BottomSheet => CornerRadius { nw: t.radius_lg as u8, ne: t.radius_lg as u8, sw: 0, se: 0 },
        HostKind::RightDock => CornerRadius::ZERO,
    };
    ui.painter().rect_filled(rect, radius, t.dock);
    // A hairline edge so the host reads as a separate surface over the canvas.
    let edge = match kind {
        HostKind::BottomSheet => [pos2(rect.left(), rect.top()), pos2(rect.right(), rect.top())],
        HostKind::RightDock => [pos2(rect.left(), rect.top()), pos2(rect.left(), rect.bottom())],
    };
    ui.painter().line_segment(edge, Stroke::new(1.0, t.separator));
    if kind == HostKind::BottomSheet {
        let handle = Rect::from_center_size(pos2(rect.center().x, rect.top() + 10.0), vec2(36.0, 4.0));
        ui.painter().rect_filled(handle, CornerRadius { nw: 2, ne: 2, sw: 2, se: 2 }, t.text_faint);
    }
}

/// Draw the sheet/dock host for `group` into `rect`, with a tab strip and the shared panel body.
///
/// Returns `true` when the drawn content changed the active tab, so the caller can request a
/// repaint. This is the one entry point both the phone shell and the tablet shell call; only
/// `kind` and `rect` differ.
pub fn panel_host(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect, kind: HostKind, group: Group) {
    if rect.width() < 8.0 || rect.height() < 8.0 {
        return;
    }
    let t = Tokens::get(ui.ctx());
    draw_host_frame(ui, rect, kind, &t);

    let inner = match kind {
        HostKind::BottomSheet => rect.shrink2(vec2(0.0, 0.0)).translate(vec2(0.0, 6.0)),
        HostKind::RightDock => rect,
    };
    if inner.width() < 8.0 || inner.height() < 8.0 {
        return;
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));

    // Reserve the tab strip, then hand the rest to the panel body.
    let pro = t.pro;
    let tabs = group_tabs(group, pro);
    let strip_h = metrics::TOP_BAR_HEIGHT.min(inner.height());
    let strip = Rect::from_min_size(inner.min, vec2(inner.width(), strip_h));
    let sel = group.tab_mut(&mut app.ui.dock_tabs);
    let before = *sel;
    crate::widgets::tab_strip::pill_tabs(&mut child, egui::Id::unique(group.key()), strip, &tabs, sel);
    let picked = *sel != before;
    let used = *group.tab_mut(&mut app.ui.dock_tabs);

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
    let r = Rect::from_center_size(rect.right_center() - vec2(rect.height() * 0.5, 0.0), vec2(32.0, 32.0));
    let resp = ui.interact(r, egui::Id::unique("mobile-host-close"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius { nw: 6, ne: 6, sw: 6, se: 6 }, t.hover);
    }
    icons::paint(ui, r, "x", 16.0, if resp.hovered() { t.text } else { t.text_dim });
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
