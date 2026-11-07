//! Touch-compatible styling and presentation policies for mobile panels.
//!
//! Provides:
//! 1. [`apply_touch_style`]: adjusts an `egui::Ui` scope so that standard checkboxes,
//!    buttons, sliders and fields receive mobile touch targets (minimum 44-48 pt).
//! 2. [`PanelPresentation`]: categorizes panels into legacy-adapted, specialized mobile views,
//!    or full-screen dialogs.

use egui::{Style, Vec2, vec2};

use crate::dock::Group;

/// Presentation strategy for a panel on mobile devices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelPresentation {
    /// Render standard desktop body wrapped in a touch-friendly `egui::Style` scope.
    AdaptiveLegacy,
    /// Render a specialized touch-first UI (e.g. Mobile Layers with swipe actions, Touch Color Wheel).
    MobileSpecialized,
    /// Requires full-screen modal focus (e.g. Curves, Camera Raw).
    FullscreenEditor,
}

impl PanelPresentation {
    /// Determine presentation strategy for a given group and tab index.
    pub fn for_group(group: Group, _tab: usize) -> Self {
        match group {
            Group::Layers => Self::MobileSpecialized,
            Group::Color => Self::MobileSpecialized,
            Group::History => Self::AdaptiveLegacy,
            Group::Properties => Self::AdaptiveLegacy,
            _ => Self::AdaptiveLegacy,
        }
    }
}

/// Minimum dimensions for finger-friendly touch targets.
pub const TOUCH_INTERACT_SIZE: Vec2 = vec2(48.0, 48.0);
pub const TOUCH_ITEM_SPACING: Vec2 = vec2(10.0, 10.0);
pub const TOUCH_BUTTON_PADDING: Vec2 = vec2(14.0, 10.0);

/// Apply touch ergonomics to an `egui::Style`.
pub fn apply_touch_style(style: &mut Style) {
    let spacing = &mut style.spacing;
    spacing.interact_size.x = spacing.interact_size.x.max(TOUCH_INTERACT_SIZE.x);
    spacing.interact_size.y = spacing.interact_size.y.max(TOUCH_INTERACT_SIZE.y);
    spacing.item_spacing = TOUCH_ITEM_SPACING;
    spacing.button_padding = TOUCH_BUTTON_PADDING;
}

/// Execute a closure inside a UI scope that enforces touch-friendly widget sizes.
pub fn with_touch_style<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        apply_touch_style(ui.style_mut());
        add_contents(ui)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_style_enforces_minimum_48pt_targets() {
        let mut style = Style::default();
        style.spacing.interact_size = vec2(20.0, 20.0);
        apply_touch_style(&mut style);
        assert!(style.spacing.interact_size.x >= 48.0);
        assert!(style.spacing.interact_size.y >= 48.0);
    }
}
