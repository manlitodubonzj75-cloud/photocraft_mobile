//! The mobile chrome: top bar, bottom navigation, tool rail and tool options.
//!
//! Every control here does a real thing through the existing command registry or the existing
//! panels. Nothing is a placeholder:
//!
//! - Undo / Redo dispatch `edit.undo` / `edit.redo` and are enabled by the *same*
//!   [`crate::menus::is_enabled`] predicate the desktop menus use, so they light up and grey out in
//!   lockstep with the menu bar.
//! - Save dispatches `file.save`; New / Open / Export dispatch their existing ids.
//! - The tool options strip is the **existing** `panels::options_bar`, wrapped in a horizontal
//!   scroll area with touch-friendly spacing so wide option sets never clip or overflow on phones.
//! - The tablet tool rail is the **existing** `panels::toolbar` column, the authoritative
//!   `TOOL_SECTIONS` grouping with its flyouts, long-press and last-used-tool memory.
//! - The phone tool sheet lays the same `visible_sections()` slots out in a grid and selects
//!   through `crate::tool_memory::select`, the same path the toolbar uses.

use egui::{CornerRadius, Rect, Sense, Stroke, pos2, vec2};

use crate::dock::Group;
use crate::mobile::panels::HostKind;
use crate::mobile::shell::metrics;
use crate::state::Tool;
use crate::theme::{self, Tokens};
use crate::{PhotocraftApp, icons};

/// A toolbar-style icon button sized for touch: at least [`metrics::MIN_TOUCH_TARGET`] on a side.
fn touch_icon(ui: &mut egui::Ui, name: &str, enabled: bool, selected: bool, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let size = metrics::MIN_TOUCH_TARGET;
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), if enabled { Sense::click() } else { Sense::hover() });
    let tint = if !enabled {
        t.text_faint
    } else if selected {
        t.accent
    } else if resp.hovered() || resp.is_pointer_button_down_on() {
        t.text
    } else {
        t.text_dim
    };
    if selected || resp.hovered() {
        let bg = if selected { t.accent_soft } else { t.hover };
        ui.painter().rect_filled(rect.shrink(3.0), CornerRadius { nw: 6, ne: 6, sw: 6, se: 6 }, bg);
    }
    icons::paint(ui, rect, name, 20.0, tint);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
    if !tip.is_empty() {
        resp.clone().on_hover_text(tip);
    }
    resp
}

/// The mobile top bar: Back (to Home), the document title, Undo, Redo, Save and an overflow menu.
///
/// Respects safe area top padding (status bar and notch) by painting chrome background across
/// the full rect and placing controls in the bottom 44 pt.
pub fn top_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    if rect.height() < 8.0 {
        return;
    }
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::ZERO, t.chrome);
    painter.line_segment([pos2(rect.left(), rect.bottom()), pos2(rect.right(), rect.bottom())], Stroke::new(1.0, t.separator));

    let bar_rect =
        if rect.height() > metrics::TOP_BAR_HEIGHT { Rect::from_min_max(pos2(rect.left(), rect.bottom() - metrics::TOP_BAR_HEIGHT), rect.max) } else { rect };

    let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(bar_rect));
    ui.set_clip_rect(bar_rect.intersect(ui.clip_rect()));
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let has_doc = app.session.active().is_some();

        // Back: leave the editor for Home (File › Home is the existing command's neighbour).
        if touch_icon(ui, "chevron-left", true, false, tl!("Home")).clicked() {
            app.ui.chrome.home = Some(app.session.documents().len());
            ui.ctx().request_repaint();
        }

        // Undo / Redo through the registry, gated by the menu's own predicate.
        let undo_on = has_doc && crate::menus::is_enabled(app, "edit.undo");
        if touch_icon(ui, "arrow-u-turn-left", undo_on, false, tl!("Undo")).clicked() && undo_on {
            let _ = app.run("edit.undo", serde_json::json!({}));
        }
        let redo_on = has_doc && crate::menus::is_enabled(app, "edit.redo");
        if touch_icon(ui, "arrow-u-turn-right", redo_on, false, tl!("Redo")).clicked() && redo_on {
            let _ = app.run("edit.redo", serde_json::json!({}));
        }

        // Title: document name plus a modified dot, exactly as the desktop title bar composes it.
        let (title, dirty) = match app.session.active() {
            Some(d) => (d.doc.name.clone(), d.is_dirty()),
            None => (String::from("PhotoCraft"), false),
        };
        let avail = (bar_rect.width() - 6.0 * metrics::MIN_TOUCH_TARGET).max(40.0);
        let label_r = Rect::from_center_size(ui.available_rect_before_wrap().center(), vec2(avail.min(ui.available_width()), bar_rect.height()));
        let text = if dirty { format!("{title} •") } else { title };
        let galley = crate::widgets::tab_strip::elided(ui, &text, theme::medium(14.0), t.text, label_r.width());
        ui.painter().galley(pos2(label_r.center().x - galley.size().x / 2.0, label_r.center().y - galley.size().y / 2.0), galley, t.text);
        ui.allocate_space(vec2(avail.min(ui.available_width()), bar_rect.height()));

        // Save and More, right-aligned.
        let save_on = has_doc && crate::menus::is_enabled(app, "file.save");
        if touch_icon(ui, "save", save_on, false, tl!("Save")).clicked() && save_on {
            let _ = app.run("file.save", serde_json::json!({}));
        }
        if touch_icon(ui, "ellipsis-vertical", true, false, tl!("More")).clicked() {
            app.ui.mobile.show(Group::Navigator);
        }
    });
}

/// The bottom navigation: Layers, Adjustments, Color, History, More. Each opens the corresponding
/// real host or panel group; none is decorative.
pub fn bottom_nav(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    if rect.height() < 8.0 {
        return;
    }
    ui.painter().rect_filled(rect, CornerRadius::ZERO, t.chrome);
    ui.painter().line_segment([pos2(rect.left(), rect.top()), pos2(rect.right(), rect.top())], Stroke::new(1.0, t.separator));

    let entries: [(&str, &str, Group); 5] = [
        ("layers", "Layers", Group::Layers),
        ("sliders-horizontal", "Adjustments", Group::Properties),
        ("palette", "Color", Group::Color),
        ("clock", "History", Group::History),
        ("ellipsis-horizontal", "More", Group::Navigator),
    ];
    let n = entries.len() as f32;
    let w = rect.width() / n;
    let open = app.ui.mobile.open();
    for (i, (icon, label, group)) in entries.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(rect.left() + w * i as f32, rect.top()), vec2(w, rect.height()));
        let resp = ui.interact(cell, egui::Id::unique((*label, "nav")), Sense::click());
        let on = open == Some(*group);
        let tint = if on { t.accent } else { t.text_dim };
        let icon_r = Rect::from_center_size(pos2(cell.center().x, cell.top() + 20.0), vec2(20.0, 20.0));
        icons::paint(ui, icon_r, icon, 20.0, tint);
        let galley = crate::widgets::tab_strip::elided(ui, tl!(*label), theme::medium(10.0), tint, cell.width() - 8.0);
        ui.painter().galley(pos2(cell.center().x - galley.size().x / 2.0, cell.bottom() - 18.0), galley, tint);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
        if resp.clicked() {
            if on {
                app.ui.mobile.hide();
            } else {
                app.ui.mobile.show(*group);
            }
            ui.ctx().request_repaint();
        }
    }
}

/// The tablet tool rail: the existing desktop toolbar column, drawn into the rail region. Reusing
/// `panels::toolbar` keeps the authoritative `TOOL_SECTIONS` order, flyouts, long-press and
/// last-used-tool memory with no parallel implementation.
pub fn tool_rail(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect) {
    if rect.width() < 8.0 || rect.height() < 8.0 {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, t.chrome);
    ui.painter().line_segment([pos2(rect.right(), rect.top()), pos2(rect.right(), rect.bottom())], Stroke::new(1.0, t.separator));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(2.0, 6.0))));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    crate::panels::toolbar(app, &mut child);
}

/// The contextual tool options strip. Reuses the existing options bar with horizontal scrolling
/// and touch spacing so wide options sets never clip or overflow on phones.
pub fn tool_options(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect) {
    if rect.width() < 8.0 || rect.height() < 8.0 {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, t.chrome);
    ui.painter().line_segment([pos2(rect.left(), rect.bottom()), pos2(rect.right(), rect.bottom())], Stroke::new(1.0, t.separator));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));

    egui::ScrollArea::horizontal().id_salt("mobile-tool-options-scroll").auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        crate::panels::options_bar(app, ui);
    });
}

/// The phone tool sheet: a touch-sized grid of the visible tool slots, laid out from the same
/// `visible_sections()` the toolbar uses. Selecting a slot selects its shown tool through
/// `tool_memory::select`, so per-tool memory behaves identically to the desktop.
pub fn tool_sheet(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, t.dock);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(8.0, 8.0))));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let sections = crate::panels::visible_sections(&app.session.prefs().toolbar.hidden);
    let cell = 52.0;
    let cols = ((child.available_width() + 6.0) / (cell + 6.0)).floor().max(1.0) as usize;
    egui::ScrollArea::vertical().id_salt("mobile-tool-sheet").auto_shrink([false, false]).show(&mut child, |ui| {
        let mut i = 0usize;
        for section in &sections {
            for (slot_index, slot) in section {
                let key = egui::Id::unique(("mobile-tool-slot", *slot_index));
                let tool = slot.iter().copied().find(|t| *t == app.ui.tool).unwrap_or(slot[0]);
                // Remember the slot's shown tool so a flyout has a stable anchor later.
                ui.data_mut(|d| d.insert_temp(key, tool));
                let col = i % cols;
                let row = i / cols;
                let x = ui.max_rect().left() + col as f32 * (cell + 6.0);
                let y = ui.max_rect().top() + row as f32 * (cell + 6.0);
                let r = Rect::from_min_size(pos2(x, y), vec2(cell, cell));
                let sel = slot.contains(&app.ui.tool);
                let resp = ui.interact(r, key.with("btn"), Sense::click());
                let tint = if sel {
                    t.accent
                } else if resp.hovered() || resp.is_pointer_button_down_on() {
                    t.text
                } else {
                    t.text_dim
                };
                if sel {
                    ui.painter().rect_filled(r.shrink(2.0), CornerRadius { nw: 8, ne: 8, sw: 8, se: 8 }, t.accent_soft);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r.shrink(2.0), CornerRadius { nw: 8, ne: 8, sw: 8, se: 8 }, t.hover);
                }
                icons::paint(ui, r, icons::tool_icon(tool), 24.0, tint);
                if slot.len() > 1 {
                    let d = Rect::from_min_size(pos2(r.right() - 8.0, r.bottom() - 8.0), vec2(4.0, 4.0));
                    ui.painter().rect_filled(d, CornerRadius { nw: 2, ne: 2, sw: 2, se: 2 }, t.text_faint);
                }
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!(tool.label())));
                let resp = resp.on_hover_text(tl!(tool.label()));
                if resp.clicked() {
                    crate::tool_memory::select(app, tool);
                }
                i += 1;
            }
        }
        let rows = i.div_ceil(cols);
        ui.allocate_space(vec2(ui.available_width(), rows as f32 * (cell + 6.0)));
    });
}

/// The current device class for this frame, honouring the `mobile.forceClass` override.
pub fn device_class(app: &PhotocraftApp, ctx: &egui::Context) -> crate::mobile::DeviceClass {
    app.ui.mobile.class(ctx.content_rect().size())
}

/// Whether a viewport is best served by the sheet host (phone) or the dock host (tablet).
pub fn host_kind(class: crate::mobile::DeviceClass) -> HostKind {
    if class.uses_bottom_sheet() { HostKind::BottomSheet } else { HostKind::RightDock }
}

/// A tool whose button is currently selected, for tests and the rail.
pub fn is_active(app: &PhotocraftApp, tool: Tool) -> bool {
    slot_contains_tool(app, tool)
}

fn slot_contains_tool(app: &PhotocraftApp, tool: Tool) -> bool {
    let sections = crate::panels::visible_sections(&app.session.prefs().toolbar.hidden);
    sections.iter().flatten().any(|(_, slot)| slot.contains(&tool) && slot.contains(&app.ui.tool))
}
