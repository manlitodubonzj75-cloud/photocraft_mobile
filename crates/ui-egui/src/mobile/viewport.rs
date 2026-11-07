//! Canonical Mobile Viewport: the single source of truth for mobile geometry and hit testing.
//!
//! # Architecture
//!
//! On mobile devices, window dimensions, safe areas (status bar, home bar, display cutouts/notches),
//! on-screen keyboards (IME), and open overlay sheets all compete for space.
//!
//! Rather than letting `shell.rs` compute one abstract layout while `panels.rs::mobile_body`
//! imperatively adds panels with independent coordinates, [`MobileViewport`] serves as the **sole
//! source of truth** for:
//!
//! 1. Window bounds and safe content insets (`safe_area`).
//! 2. Concrete region rectangles (`top_bar`, `bottom_nav`, `tool_options`, `sheet`, `canvas`).
//! 3. **Hit-testing & touch exclusion**: accurately determining whether a pointer touch belongs
//!    to an overlay UI element (top bar, sheet, nav bar) or to the interactive canvas.

use egui::{Margin, Pos2, Rect, pos2};

use super::device::{DeviceClass, Orientation};
use super::shell::{OpenPanels, SHEET_MAX_FRACTION, SheetHost, metrics};

/// The canonical geometry and touch-routing map of the mobile surface for the current frame.
#[derive(Clone, Debug, PartialEq)]
pub struct MobileViewport {
    /// Full viewport rectangle in logical points (typically `(0, 0) .. (width, height)`).
    pub window: Rect,
    /// Safe content area after applying system safe area insets (notches, system bars, home pill).
    pub safe_content: Rect,
    /// Device classification (Phone vs Tablet).
    pub class: DeviceClass,
    /// Screen orientation.
    pub orientation: Orientation,
    /// The top bar rectangle (if visible).
    pub top_bar: Option<Rect>,
    /// The bottom navigation rectangle (Phone only, if visible).
    pub bottom_nav: Option<Rect>,
    /// The left tool rail rectangle (Tablet only, if visible).
    pub tool_rail: Option<Rect>,
    /// The active tool's contextual options bar (if visible).
    pub tool_options: Option<Rect>,
    /// The inspector dock rectangle (Tablet only, when a group is open).
    pub inspector_dock: Option<Rect>,
    /// The overlay bottom sheet rectangle (Phone only, when a group is open).
    pub sheet: Option<Rect>,
    /// The interactive canvas area.
    pub canvas: Rect,
    /// Rectangles that must intercept touches before the canvas can see them.
    /// Listed from top-most (overlay sheet, modal headers) to chrome bars.
    pub touch_exclusion: Vec<Rect>,
}

impl MobileViewport {
    /// Compute the canonical viewport from window bounds, safe area insets, device class,
    /// open panels state, and full screen flag.
    ///
    /// - `window`: The total window rect (e.g. from `ctx.content_rect()` or viewport).
    /// - `safe_insets`: System margins (top notch, bottom home indicator, etc.).
    /// - `class`: Phone or Tablet.
    /// - `panels`: Current state of open sheets/docks.
    /// - `full_screen`: When true, all chrome is dropped and the canvas fills the entire window.
    /// - `tool_options_visible`: Whether tool options should be shown (e.g. when a document is open).
    pub fn compute(window: Rect, safe_insets: Margin, class: DeviceClass, panels: &OpenPanels, full_screen: bool, tool_options_visible: bool) -> Self {
        let size = window.size();
        let orientation = DeviceClass::orientation(size);

        if full_screen || window.width() <= 0.0 || window.height() <= 0.0 {
            return Self {
                window,
                safe_content: window,
                class,
                orientation,
                top_bar: None,
                bottom_nav: None,
                tool_rail: None,
                tool_options: None,
                inspector_dock: None,
                sheet: None,
                canvas: window,
                touch_exclusion: Vec::new(),
            };
        }

        // Clamp safe insets to sensible bounds so degenerate insets can't crush the viewport.
        let max_inset_x = (window.width() * 0.25).max(0.0);
        let max_inset_y = (window.height() * 0.25).max(0.0);
        let safe_top = (safe_insets.top as f32).clamp(0.0, max_inset_y);
        let safe_bottom = (safe_insets.bottom as f32).clamp(0.0, max_inset_y);
        let safe_left = (safe_insets.left as f32).clamp(0.0, max_inset_x);
        let safe_right = (safe_insets.right as f32).clamp(0.0, max_inset_x);

        let safe_content =
            Rect::from_min_max(pos2(window.min.x + safe_left, window.min.y + safe_top), pos2(window.max.x - safe_right, window.max.y - safe_bottom));

        let mut touch_exclusion = Vec::with_capacity(6);

        match class {
            DeviceClass::Phone => {
                // Top Bar: spans the full width of window, but height accommodates safe top.
                let top_h = metrics::TOP_BAR_HEIGHT + safe_top;
                let top_bar = Rect::from_min_max(window.min, pos2(window.max.x, window.min.y + top_h));
                touch_exclusion.push(top_bar);

                // Bottom Nav: spans full width of window, height accommodates safe bottom.
                let nav_h = metrics::BOTTOM_NAV_HEIGHT + safe_bottom;
                let bottom_nav = Rect::from_min_max(pos2(window.min.x, window.max.y - nav_h), window.max);
                touch_exclusion.push(bottom_nav);

                // Tool Options: sits directly above the bottom nav within safe horizontal bounds.
                let tool_options = if tool_options_visible {
                    let opts = Rect::from_min_max(pos2(window.min.x, bottom_nav.min.y - metrics::TOOL_OPTIONS_HEIGHT), pos2(window.max.x, bottom_nav.min.y));
                    touch_exclusion.push(opts);
                    Some(opts)
                } else {
                    None
                };

                // Canvas: bounded by top bar and bottom tool options (or nav).
                let canvas_bottom = tool_options.map_or(bottom_nav.min.y, |o| o.min.y);
                let canvas = Rect::from_min_max(pos2(window.min.x, top_bar.max.y), pos2(window.max.x, canvas_bottom));

                // Bottom Sheet (if open): overlay that floats over the canvas.
                // Its height is based on the canvas height to avoid covering 100% of the screen.
                let sheet = if panels.host(class) == SheetHost::BottomSheet {
                    let h = panels.sheet_height(canvas.size()).min(canvas.height() * SHEET_MAX_FRACTION);
                    let sheet_rect = Rect::from_min_max(pos2(window.min.x, canvas_bottom - h), pos2(window.max.x, canvas_bottom));
                    // Sheet touch exclusion is inserted at index 0 so it has the highest hit priority.
                    touch_exclusion.insert(0, sheet_rect);
                    Some(sheet_rect)
                } else {
                    None
                };

                Self {
                    window,
                    safe_content,
                    class,
                    orientation,
                    top_bar: Some(top_bar),
                    bottom_nav: Some(bottom_nav),
                    tool_rail: None,
                    tool_options,
                    inspector_dock: None,
                    sheet,
                    canvas,
                    touch_exclusion,
                }
            }
            DeviceClass::Tablet => {
                // Top Bar: full width at top with safe top.
                let top_h = metrics::TOP_BAR_HEIGHT + safe_top;
                let top_bar = Rect::from_min_max(window.min, pos2(window.max.x, window.min.y + top_h));
                touch_exclusion.push(top_bar);

                // Left Tool Rail: on tablet, sits below top bar on the left edge.
                let rail_w = metrics::TOOL_RAIL_WIDTH + safe_left;
                let tool_rail = Rect::from_min_max(pos2(window.min.x, top_bar.max.y), pos2(window.min.x + rail_w, window.max.y));
                touch_exclusion.push(tool_rail);

                // Inspector Dock (if open): on the right edge.
                let dock_w = if panels.host(class) == SheetHost::RightDock {
                    // Maximum 1/3 of the screen width, plus right safe inset.
                    (metrics::INSPECTOR_DOCK_WIDTH + safe_right).min(window.width() / 3.0)
                } else {
                    0.0
                };

                let inspector_dock = if dock_w > 0.0 {
                    let dock_rect = Rect::from_min_max(pos2(window.max.x - dock_w, top_bar.max.y), window.max);
                    touch_exclusion.insert(0, dock_rect);
                    Some(dock_rect)
                } else {
                    None
                };

                let dock_left = inspector_dock.map_or(window.max.x - safe_right, |d| d.min.x);

                // Tool Options strip: on tablet, sits at the bottom between rail and dock.
                let tool_options = if tool_options_visible {
                    let opts = Rect::from_min_max(
                        pos2(tool_rail.max.x, window.max.y - metrics::TOOL_OPTIONS_HEIGHT - safe_bottom),
                        pos2(dock_left, window.max.y - safe_bottom),
                    );
                    touch_exclusion.push(opts);
                    Some(opts)
                } else {
                    None
                };

                let canvas_bottom = tool_options.map_or(window.max.y - safe_bottom, |o| o.min.y);
                let canvas = Rect::from_min_max(pos2(tool_rail.max.x, top_bar.max.y), pos2(dock_left, canvas_bottom));

                Self {
                    window,
                    safe_content,
                    class,
                    orientation,
                    top_bar: Some(top_bar),
                    bottom_nav: None,
                    tool_rail: Some(tool_rail),
                    tool_options,
                    inspector_dock,
                    sheet: None,
                    canvas,
                    touch_exclusion,
                }
            }
        }
    }

    /// Hit-test whether a screen point falls onto any UI element that excludes canvas interactions.
    ///
    /// Returns `true` if the point touches an active chrome bar, sheet, or inspector dock.
    /// When this returns `true`, gestures must NOT be routed to the document canvas or drawing tools.
    #[inline]
    pub fn is_touch_excluded(&self, pos: Pos2) -> bool {
        self.touch_exclusion.iter().any(|r| r.contains(pos))
    }

    /// Check if a position is strictly within the interactable canvas area and not occluded
    /// by any overlay sheet or chrome.
    #[inline]
    pub fn hits_canvas(&self, pos: Pos2) -> bool {
        self.canvas.contains(pos) && !self.is_touch_excluded(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::Group;
    use egui::vec2;

    fn test_margin() -> Margin {
        Margin { left: 0, right: 0, top: 48, bottom: 34 }
    }

    #[test]
    fn phone_viewport_respects_safe_insets_and_orders_regions() {
        let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(390.0, 844.0));
        let panels = OpenPanels::default();
        let vp = MobileViewport::compute(window, test_margin(), DeviceClass::Phone, &panels, false, true);

        assert_eq!(vp.class, DeviceClass::Phone);
        assert_eq!(vp.orientation, Orientation::Portrait);

        let top = vp.top_bar.unwrap();
        assert_eq!(top.min.y, 0.0);
        assert!(top.height() >= metrics::TOP_BAR_HEIGHT + 48.0);

        let nav = vp.bottom_nav.unwrap();
        assert_eq!(nav.max.y, 844.0);
        assert!(nav.height() >= metrics::BOTTOM_NAV_HEIGHT + 34.0);

        let opts = vp.tool_options.unwrap();
        assert_eq!(opts.max.y, nav.min.y);

        assert_eq!(vp.canvas.min.y, top.max.y);
        assert_eq!(vp.canvas.max.y, opts.min.y);

        // Check hit testing:
        assert!(vp.is_touch_excluded(top.center()));
        assert!(vp.is_touch_excluded(nav.center()));
        assert!(vp.is_touch_excluded(opts.center()));
        assert!(vp.hits_canvas(vp.canvas.center()));
    }

    #[test]
    fn bottom_sheet_occludes_canvas_touch_area() {
        let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(390.0, 844.0));
        let mut panels = OpenPanels::default();
        panels.show(Group::Layers);
        let vp = MobileViewport::compute(window, Margin::ZERO, DeviceClass::Phone, &panels, false, true);

        let sheet = vp.sheet.expect("sheet must be present");
        assert!(sheet.height() > 50.0);

        // Points inside the sheet must be touch-excluded from canvas!
        assert!(vp.is_touch_excluded(sheet.center()));
        assert!(!vp.hits_canvas(sheet.center()));
    }

    #[test]
    fn tablet_viewport_allocates_rail_and_dock() {
        let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(1024.0, 768.0));
        let mut panels = OpenPanels::default();
        panels.show(Group::Layers);
        let vp = MobileViewport::compute(window, Margin::ZERO, DeviceClass::Tablet, &panels, false, true);

        assert_eq!(vp.class, DeviceClass::Tablet);
        let rail = vp.tool_rail.unwrap();
        let dock = vp.inspector_dock.unwrap();

        assert_eq!(rail.min.x, 0.0);
        assert_eq!(dock.max.x, 1024.0);
        assert_eq!(vp.canvas.min.x, rail.max.x);
        assert_eq!(vp.canvas.max.x, dock.min.x);
    }
}
