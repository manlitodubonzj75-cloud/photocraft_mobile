//! The responsive shell: a region model for the mobile arrangement.
//!
//! The shell does not draw anything itself in this module. It answers two questions for the mobile
//! host app, both as plain data so they can be unit-tested without an egui frame:
//!
//! 1. *Given the device class and the current UI state, which regions exist and how are they
//!    sized?* ([`MobileShellLayout`], [`regions`]).
//! 2. *Which panels live in which host?* ([`SheetHost`]).
//!
//! ## Why a separate region model
//!
//! The desktop shell lays out title bar -> options bar -> status bar -> toolbar -> dock around the
//! canvas (`crate::app::frame::ui_body`). The phone shell needs a different order - top bar, then
//! the canvas, then the active tool's options, then the bottom navigation, with panels in an
//! overlay sheet - and the tablet shell needs a third (left rail, canvas, right dock). Expressing
//! those as a small, testable region list keeps the drawing code in the host app thin and keeps
//! the "no feature silently disappears" invariant checkable in a test rather than by eye.
//!
//! ## State preservation
//!
//! The layout is recomputed every frame from live state and owns no editor state of its own. The
//! only thing it keeps is *where the panels are being shown* ([`MobileShell::open_group`]), and
//! that field is deliberately carried across device-class changes: rotating a tablet from landscape
//! to portrait, or dragging a window across the breakpoint, must not close the panel the user has
//! open. This mirrors the task's "Preserve editor state when panels open and close".

use egui::{Rect, Vec2};

use crate::dock::Group;
use crate::mobile::device::{DeviceClass, Orientation};
use crate::state::UiState;

/// Which host shows the inspector panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetHost {
    /// No panels are shown.
    Hidden,
    /// The overlay bottom sheet (phones).
    BottomSheet,
    /// The permanent right dock (tablets).
    RightDock,
}

/// Which panel group the sheet/dock is showing, if any.
///
/// This is the shell's only piece of state, and it is intentionally not serialised: it is a view of
/// the moment, like `UiState::panels::full_screen_chrome`. The *panel contents and their own state*
/// live in [`UiState`] and are what persists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpenPanels {
    pub group: Option<Group>,
    /// Sheet height as a fraction of the viewport (phones). Clamped to [`SHEET_MIN_FRACTION`] ..
    /// [`SHEET_MAX_FRACTION`].
    pub sheet_fraction: f32,
}

/// The smallest fraction of the viewport a draggable sheet may shrink to.
pub const SHEET_MIN_FRACTION: f32 = 0.15;
/// The largest fraction of the viewport the sheet may grow to, so the canvas is never fully hidden
/// (task: "Avoid canvas obstruction").
pub const SHEET_MAX_FRACTION: f32 = 0.85;
/// Default sheet height when first opened.
pub const SHEET_DEFAULT_FRACTION: f32 = 0.45;

impl Default for OpenPanels {
    fn default() -> Self {
        Self { group: None, sheet_fraction: SHEET_DEFAULT_FRACTION }
    }
}

impl OpenPanels {
    pub fn host(&self, class: DeviceClass) -> SheetHost {
        match (class, self.group) {
            (_, None) => SheetHost::Hidden,
            (DeviceClass::Tablet, Some(_)) => SheetHost::RightDock,
            (DeviceClass::Phone, Some(_)) => SheetHost::BottomSheet,
        }
    }

    /// The sheet height in points for a viewport of the given size, clamped to the allowed range.
    pub fn sheet_height(&self, viewport: Vec2) -> f32 {
        let f = self.sheet_fraction.clamp(SHEET_MIN_FRACTION, SHEET_MAX_FRACTION);
        (viewport.y * f).min(viewport.y)
    }

    /// Open (or focus) a panel group. Recomputes nothing else, so switching groups keeps the sheet
    /// height the user dragged it to.
    pub fn show(&mut self, group: Group) {
        self.group = Some(group);
    }

    /// Close the sheet/dock panel host.
    pub fn hide(&mut self) {
        self.group = None;
    }
}

/// A named rectangle in the shell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub id: RegionId,
    pub rect: Rect,
}

/// The regions the shell lays out. Named so tests can assert presence/absence by identity rather
/// than by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegionId {
    /// Top bar: back, document title, undo/redo, save, overflow menu.
    TopBar,
    /// The canvas: always present, always the largest interactive region.
    Canvas,
    /// Left tool rail (tablet only).
    ToolRail,
    /// Right inspector dock (tablet only, and only when a group is open).
    InspectorDock,
    /// Contextual active-tool options (phone: a strip above the bottom nav).
    ToolOptions,
    /// Bottom navigation bar (phone only).
    BottomNav,
    /// The overlay bottom sheet (phone only, and only when a group is open).
    BottomSheet,
}

/// Fixed shell metrics in logical points. Kept here so every host app and every test agrees on
/// them, and so a narrow phone cannot accidentally get controls smaller than a fingertip.
pub mod metrics {
    /// Minimum touch target edge, per the iOS HIG (44 pt) and Android (48 dp). The larger of the
    /// two is used so both platforms feel right.
    pub const MIN_TOUCH_TARGET: f32 = 48.0;
    /// Height of the phone top bar.
    pub const TOP_BAR_HEIGHT: f32 = 44.0;
    /// Height of the phone bottom navigation bar.
    pub const BOTTOM_NAV_HEIGHT: f32 = 56.0;
    /// Width of the tablet left tool rail.
    pub const TOOL_RAIL_WIDTH: f32 = 56.0;
    /// Default width of the tablet right inspector dock.
    pub const INSPECTOR_DOCK_WIDTH: f32 = 320.0;
    /// Strip height of the contextual tool options on a phone.
    pub const TOOL_OPTIONS_HEIGHT: f32 = 48.0;
}

/// The complete region model for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct MobileShellLayout {
    pub class: DeviceClass,
    pub orientation: Orientation,
    pub regions: Vec<Region>,
}

impl MobileShellLayout {
    pub fn region(&self, id: RegionId) -> Option<Rect> {
        self.regions.iter().find(|r| r.id == id).map(|r| r.rect)
    }

    pub fn has(&self, id: RegionId) -> bool {
        self.regions.iter().any(|r| r.id == id)
    }

    /// The canvas rect. Always present, but `None` only if the layout was built from a degenerate
    /// viewport that leaves no room at all.
    pub fn canvas(&self) -> Option<Rect> {
        self.region(RegionId::Canvas)
    }
}

/// Build the region model for the current frame.
///
/// `viewport` is the full window rect in logical points. `full_screen` is the editor's
/// "Full Screen Mode" (F), which the shell honours by dropping all chrome so the canvas is
/// edge-to-edge, exactly as the desktop does (`UiState::view::hides_chrome`).
pub fn regions(viewport: Rect, class: DeviceClass, panels: &OpenPanels, full_screen: bool) -> MobileShellLayout {
    let size = viewport.size();
    let orientation = DeviceClass::orientation(size);
    let mut regions = Vec::with_capacity(6);

    if full_screen {
        regions.push(Region { id: RegionId::Canvas, rect: viewport });
        return MobileShellLayout { class, orientation, regions };
    }

    match class {
        DeviceClass::Phone => {
            let top = Rect::from_min_max(viewport.min, egui::pos2(viewport.max.x, viewport.min.y + metrics::TOP_BAR_HEIGHT));
            regions.push(Region { id: RegionId::TopBar, rect: top });

            let nav = Rect::from_min_max(egui::pos2(viewport.min.x, viewport.max.y - metrics::BOTTOM_NAV_HEIGHT), viewport.max);
            regions.push(Region { id: RegionId::BottomNav, rect: nav });

            // Contextual tool options sit directly above the bottom navigation, so the active
            // tool's controls are reachable with the thumb while the canvas stays central.
            let opts = Rect::from_min_max(egui::pos2(viewport.min.x, nav.min.y - metrics::TOOL_OPTIONS_HEIGHT), egui::pos2(viewport.max.x, nav.min.y));
            regions.push(Region { id: RegionId::ToolOptions, rect: opts });

            let canvas = Rect::from_min_max(egui::pos2(viewport.min.x, top.max.y), egui::pos2(viewport.max.x, opts.min.y));
            regions.push(Region { id: RegionId::Canvas, rect: canvas });

            if panels.host(class) == SheetHost::BottomSheet {
                let h = panels.sheet_height(size);
                let sheet = Rect::from_min_max(egui::pos2(viewport.min.x, viewport.max.y - h), viewport.max);
                regions.push(Region { id: RegionId::BottomSheet, rect: sheet });
            }
        }
        DeviceClass::Tablet => {
            let top = Rect::from_min_max(viewport.min, egui::pos2(viewport.max.x, viewport.min.y + metrics::TOP_BAR_HEIGHT));
            regions.push(Region { id: RegionId::TopBar, rect: top });

            let rail = Rect::from_min_max(egui::pos2(viewport.min.x, top.max.y), egui::pos2(viewport.min.x + metrics::TOOL_RAIL_WIDTH, viewport.max.y));
            regions.push(Region { id: RegionId::ToolRail, rect: rail });

            let dock_width = if panels.host(class) == SheetHost::RightDock {
                // Never take more than a third of the width, so the canvas keeps a clear majority.
                metrics::INSPECTOR_DOCK_WIDTH.min(viewport.width() / 3.0)
            } else {
                0.0
            };
            let dock_right = viewport.max.x;
            let dock_left = dock_right - dock_width;

            let opts = Rect::from_min_max(egui::pos2(rail.max.x, top.max.y), egui::pos2(dock_left, top.max.y + metrics::TOOL_OPTIONS_HEIGHT));
            regions.push(Region { id: RegionId::ToolOptions, rect: opts });

            let canvas = Rect::from_min_max(egui::pos2(rail.max.x, opts.max.y), egui::pos2(dock_left, viewport.max.y));
            regions.push(Region { id: RegionId::Canvas, rect: canvas });

            if dock_width > 0.0 {
                let dock = Rect::from_min_max(egui::pos2(dock_left, top.max.y), egui::pos2(dock_right, viewport.max.y));
                regions.push(Region { id: RegionId::InspectorDock, rect: dock });
            }
        }
    }

    MobileShellLayout { class, orientation, regions }
}

/// Tracks the shell's per-frame state and the previous device class, so a class change can be
/// detected and reported (for a one-off repaint or a preference write) while the shared
/// [`UiState`] carries everything that must survive.
#[derive(Clone, Debug, Default)]
pub struct MobileShell {
    pub open: OpenPanels,
    /// The class seen last frame, so the host can react to a change (e.g. request a repaint).
    pub last_class: Option<DeviceClass>,
}

impl MobileShell {
    /// Observe the current device class, returning `true` when it changed since the last frame.
    /// The open-panel state is intentionally left untouched across the change.
    pub fn observe_class(&mut self, class: DeviceClass) -> bool {
        let changed = self.last_class != Some(class);
        self.last_class = Some(class);
        changed
    }

    /// The layout for this frame.
    pub fn layout(&self, viewport: Rect, class: DeviceClass, ui: &UiState) -> MobileShellLayout {
        regions(viewport, class, &self.open, ui.view.hides_chrome())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2};

    fn phone() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(390.0, 844.0))
    }

    fn tablet() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(1024.0, 768.0))
    }

    #[test]
    fn phone_has_no_permanent_side_panels() {
        let l = regions(phone(), DeviceClass::Phone, &OpenPanels::default(), false);
        assert!(!l.has(RegionId::ToolRail));
        assert!(!l.has(RegionId::InspectorDock));
        assert!(l.has(RegionId::TopBar));
        assert!(l.has(RegionId::BottomNav));
        assert!(l.has(RegionId::ToolOptions));
        assert!(l.canvas().is_some());
    }

    #[test]
    fn closed_sheet_leaves_no_sheet_region() {
        let mut open = OpenPanels::default();
        open.hide();
        let l = regions(phone(), DeviceClass::Phone, &open, false);
        assert!(!l.has(RegionId::BottomSheet));
        open.show(Group::Layers);
        let l = regions(phone(), DeviceClass::Phone, &open, false);
        assert!(l.has(RegionId::BottomSheet));
    }

    #[test]
    fn tablet_has_rail_and_dock_only_when_open() {
        let mut open = OpenPanels::default();
        open.hide();
        let closed = regions(tablet(), DeviceClass::Tablet, &open, false);
        assert!(closed.has(RegionId::ToolRail));
        assert!(!closed.has(RegionId::InspectorDock));
        assert!(!closed.has(RegionId::BottomNav));

        open.show(Group::Layers);
        let opened = regions(tablet(), DeviceClass::Tablet, &open, false);
        assert!(opened.has(RegionId::InspectorDock));
    }

    #[test]
    fn canvas_stays_the_majority_on_both_classes() {
        let mut open = OpenPanels::default();
        open.show(Group::Layers);
        for viewport in [phone(), tablet()] {
            let class = DeviceClass::classify(viewport.size());
            let l = regions(viewport, class, &open, false);
            let canvas = l.canvas().expect("canvas present");
            assert!(canvas.width() > 0.0 && canvas.height() > 0.0);
            assert!(canvas.area() > viewport.area() * 0.4, "canvas {canvas:?} of {viewport:?}");
        }
    }

    #[test]
    fn full_screen_is_canvas_only() {
        let mut open = OpenPanels::default();
        open.show(Group::Layers);
        let l = regions(phone(), DeviceClass::Phone, &open, true);
        assert_eq!(l.regions.len(), 1);
        assert_eq!(l.regions[0].id, RegionId::Canvas);
        assert_eq!(l.regions[0].rect, phone());
    }

    #[test]
    fn tool_options_never_overlap_the_canvas_or_nav() {
        let class = DeviceClass::Phone;
        let l = regions(phone(), class, &OpenPanels::default(), false);
        let canvas = l.canvas().unwrap();
        let opts = l.region(RegionId::ToolOptions).unwrap();
        let nav = l.region(RegionId::BottomNav).unwrap();
        assert!(canvas.max.y <= opts.min.y + f32::EPSILON);
        assert!(opts.max.y <= nav.min.y + f32::EPSILON);
    }

    #[test]
    fn sheet_height_is_clamped_and_never_covers_the_whole_screen() {
        let mut open = OpenPanels::default();
        open.sheet_fraction = 5.0;
        open.show(Group::Layers);
        let v = phone().size();
        let h = open.sheet_height(v);
        assert!(h <= v.y * SHEET_MAX_FRACTION + f32::EPSILON);
        open.sheet_fraction = 0.0;
        assert!(open.sheet_height(v) >= v.y * SHEET_MIN_FRACTION - f32::EPSILON);
    }

    #[test]
    fn open_panel_survives_a_device_class_change() {
        // The whole point of keeping `open` outside the recomputed layout: rotating or resizing
        // across the breakpoint must not close the user's panel.
        let mut shell = MobileShell::default();
        shell.open.show(Group::Layers);
        assert!(shell.observe_class(DeviceClass::Tablet));
        assert!(!shell.observe_class(DeviceClass::Tablet));
        assert!(shell.observe_class(DeviceClass::Phone));
        assert_eq!(shell.open.group, Some(Group::Layers));
        // And the host changes: dock on tablet, sheet on phone, same group.
        assert_eq!(shell.open.host(DeviceClass::Tablet), SheetHost::RightDock);
        assert_eq!(shell.open.host(DeviceClass::Phone), SheetHost::BottomSheet);
    }

    #[test]
    fn dock_never_takes_more_than_a_third_of_a_tablet() {
        let mut open = OpenPanels::default();
        open.show(Group::Layers);
        let vp = tablet();
        let l = regions(vp, DeviceClass::Tablet, &open, false);
        let dock = l.region(RegionId::InspectorDock).unwrap();
        assert!(dock.width() <= vp.width() / 3.0 + f32::EPSILON);
    }
}
