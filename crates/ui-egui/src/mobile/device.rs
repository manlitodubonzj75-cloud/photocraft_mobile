//! Device classification for the mobile shell.
//!
//! The shell needs exactly one responsive decision: is this a phone-sized surface or a
//! tablet-sized one? Everything else follows from that (see [`super::shell`]).
//!
//! The classification is a *pure function of the egui viewport size in logical points*, so it is
//! re-evaluated every frame and needs no platform query, no feature detection and no persisted
//! state. That is what makes a foldable work: when its window resizes across the breakpoint the
//! class simply changes, and because the shell keeps all editor state in the shared
//! [`crate::state::UiState`] (not in the shell), nothing is lost across the transition.
//!
//! The breakpoint follows the short side of the viewport, not the width alone, so a phone held in
//! landscape is still a phone: an iPhone in landscape is ~926 pts wide but only ~390 pts tall, and
//! it must keep the phone arrangement (canvas-first, bottom sheets) rather than suddenly growing a
//! permanent side rail it has no room for.

use egui::Vec2;

/// Which arrangement the shell should use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceClass {
    /// Smartphones and other narrow surfaces: canvas-first, bottom sheets, no permanent side panel.
    Phone,
    /// Tablets and other wide surfaces: left tool rail, central canvas, right inspector dock.
    Tablet,
}

/// Whether the viewport is taller or wider than it is tall.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Short-side length in logical points at or above which the surface is treated as a tablet.
///
/// This is the one breakpoint in the module. 600 pt is the conventional phone/tablet boundary
/// (it is Android's `sw600dp` smallest-width qualifier): a 6.9" phone's short side stays under it
/// (a 932x430 pt viewport has a 430 pt short side), while every common tablet clears it in both
/// orientations (an iPad mini is 744x1133 pt; in landscape its short side is 744 pt). Keeping the
/// test on the short side is what keeps a landscape phone classified as a phone.
pub const TABLET_MIN_SHORT_SIDE: f32 = 600.0;

impl DeviceClass {
    /// Classify a viewport given its size in logical points.
    ///
    /// A degenerate viewport (zero, negative, NaN or infinite on either side, e.g. a minimised or
    /// not-yet-sized window) is a phone: the phone arrangement is the one that stays usable in the
    /// least space, so it is the safe fallback.
    pub fn classify(size: Vec2) -> Self {
        if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            return Self::Phone;
        }
        if size.x.min(size.y) >= TABLET_MIN_SHORT_SIDE { Self::Tablet } else { Self::Phone }
    }

    pub fn orientation(size: Vec2) -> Orientation {
        if size.x > size.y { Orientation::Landscape } else { Orientation::Portrait }
    }

    /// Whether this class shows the permanent left tool rail.
    pub fn has_tool_rail(self) -> bool {
        matches!(self, Self::Tablet)
    }

    /// Whether this class shows the permanent right inspector dock. Phones never do: their panels
    /// live in a bottom sheet (task: "No permanently visible side panels").
    pub fn has_inspector_dock(self) -> bool {
        matches!(self, Self::Tablet)
    }

    /// Whether panels are hosted in an overlay sheet rather than in place.
    pub fn uses_bottom_sheet(self) -> bool {
        matches!(self, Self::Phone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    /// A landscape phone has a wide viewport but a small short side; it must stay a phone or it
    /// would try to show a rail and dock it has no room for.
    #[test]
    fn landscape_phone_is_still_a_phone() {
        assert_eq!(DeviceClass::classify(vec2(932.0, 430.0)), DeviceClass::Phone);
        assert_eq!(DeviceClass::orientation(vec2(932.0, 430.0)), Orientation::Landscape);
    }

    #[test]
    fn narrow_phone_is_a_phone_in_both_orientations() {
        assert_eq!(DeviceClass::classify(vec2(390.0, 844.0)), DeviceClass::Phone);
        assert_eq!(DeviceClass::classify(vec2(844.0, 390.0)), DeviceClass::Phone);
    }

    #[test]
    fn ipad_mini_is_a_tablet_in_both_orientations() {
        // 744 x 1133 pt logical.
        assert_eq!(DeviceClass::classify(vec2(744.0, 1133.0)), DeviceClass::Tablet);
        assert_eq!(DeviceClass::classify(vec2(1133.0, 744.0)), DeviceClass::Tablet);
    }

    #[test]
    fn breakpoint_is_inclusive_at_the_tablet_threshold() {
        assert_eq!(DeviceClass::classify(vec2(TABLET_MIN_SHORT_SIDE, 1000.0)), DeviceClass::Tablet);
        assert_eq!(DeviceClass::classify(vec2(TABLET_MIN_SHORT_SIDE - 1.0, 1000.0)), DeviceClass::Phone);
    }

    #[test]
    fn degenerate_viewport_falls_back_to_phone() {
        assert_eq!(DeviceClass::classify(vec2(0.0, 0.0)), DeviceClass::Phone);
        assert_eq!(DeviceClass::classify(vec2(-10.0, 800.0)), DeviceClass::Phone);
        assert_eq!(DeviceClass::classify(vec2(f32::NAN, 800.0)), DeviceClass::Phone);
        assert_eq!(DeviceClass::classify(vec2(f32::INFINITY, f32::INFINITY)), DeviceClass::Phone);
    }

    #[test]
    fn capabilities_follow_the_class() {
        assert!(!DeviceClass::Phone.has_tool_rail());
        assert!(!DeviceClass::Phone.has_inspector_dock());
        assert!(DeviceClass::Phone.uses_bottom_sheet());

        assert!(DeviceClass::Tablet.has_tool_rail());
        assert!(DeviceClass::Tablet.has_inspector_dock());
        assert!(!DeviceClass::Tablet.uses_bottom_sheet());
    }
}
