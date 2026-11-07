//! Persisted mobile-shell state.
//!
//! This lives inside [`crate::state::UiState`] and so is serialised with it, reported by
//! `ui.inspect` and settable with `ui.set {mobile: …}`, exactly like every other piece of UI state.
//! That is the whole point: the mobile arrangement must be drivable and observable through the same
//! control channel as the desktop one, not a private flag the automation cannot see.
//!
//! Nothing here duplicates panel or document state. `open_group` only *names* which group's host is
//! open (the group's tab selection and content live in [`crate::state::DockTabs`] and the panels
//! themselves); `sheet_fraction` is only the host's height.

use serde::{Deserialize, Serialize};

use crate::dock::Group;
use crate::mobile::shell::SHEET_DEFAULT_FRACTION;

/// Whether the shell is in its mobile arrangement, and how it is currently shown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MobileUiState {
    /// Draw the mobile arrangement instead of the desktop chrome. Off by default so the desktop app
    /// is byte-for-byte unchanged unless a host turns it on (the mobile entry point does).
    pub enabled: bool,
    /// Force a device class regardless of the viewport, for testing a phone layout on a desktop
    /// window (`ui.set {mobile: {forceClass: "phone"}}`). `None` classifies from the viewport.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_class: Option<String>,
    /// Which panel group's host (bottom sheet on a phone, dock on a tablet) is open, by
    /// [`Group::key`]; `None` when none is. A group key rather than an enum keeps this stable when
    /// groups are reordered, and readable in `ui.inspect`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_group: Option<String>,
    /// Sheet height as a fraction of the viewport, on phones.
    pub sheet_fraction: f32,
}

impl Default for MobileUiState {
    fn default() -> Self {
        Self { enabled: false, force_class: None, open_group: None, sheet_fraction: SHEET_DEFAULT_FRACTION }
    }
}

impl MobileUiState {
    /// The open group, if any, as a [`Group`].
    pub fn open(&self) -> Option<Group> {
        self.open_group.as_deref().and_then(Group::from_key)
    }

    /// Open a group's host (and close whichever was open).
    pub fn show(&mut self, group: Group) {
        self.open_group = Some(group.key().to_owned());
    }

    /// Close the panel host.
    pub fn hide(&mut self) {
        self.open_group = None;
    }

    /// The class to lay out for, from the forced value (for tests) or the viewport.
    pub fn class(&self, viewport: egui::Vec2) -> crate::mobile::device::DeviceClass {
        match self.force_class.as_deref() {
            Some("phone") => crate::mobile::device::DeviceClass::Phone,
            Some("tablet") => crate::mobile::device::DeviceClass::Tablet,
            _ => crate::mobile::device::DeviceClass::classify(viewport),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    #[test]
    fn open_group_round_trips_through_its_key() {
        let mut s = MobileUiState::default();
        assert_eq!(s.open(), None);
        s.show(Group::Layers);
        assert_eq!(s.open_group.as_deref(), Some("layers"));
        assert_eq!(s.open(), Some(Group::Layers));
        s.hide();
        assert_eq!(s.open(), None);
    }

    #[test]
    fn every_group_has_a_distinct_key_that_round_trips() {
        for g in Group::ALL {
            let mut s = MobileUiState::default();
            s.show(g);
            assert_eq!(s.open(), Some(g), "{}", g.key());
        }
    }

    #[test]
    fn forced_class_overrides_the_viewport() {
        let mut s = MobileUiState::default();
        assert_eq!(s.class(vec2(1024.0, 768.0)), crate::mobile::DeviceClass::Tablet);
        s.force_class = Some("phone".into());
        assert_eq!(s.class(vec2(1024.0, 768.0)), crate::mobile::DeviceClass::Phone);
        s.force_class = Some("tablet".into());
        assert_eq!(s.class(vec2(390.0, 844.0)), crate::mobile::DeviceClass::Tablet);
        s.force_class = Some("nonsense".into());
        assert_eq!(s.class(vec2(390.0, 844.0)), crate::mobile::DeviceClass::Phone);
    }

    #[test]
    fn disabled_by_default() {
        assert!(!MobileUiState::default().enabled);
    }

    #[test]
    fn serde_is_camel_case_and_defaults() {
        let s = MobileUiState::default();
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["enabled"], serde_json::json!(false));
        assert!(j.get("openGroup").is_none(), "None is skipped: {j}");
        let back: MobileUiState = serde_json::from_str(r#"{"enabled":true,"openGroup":"layers","sheetFraction":0.6}"#).unwrap();
        assert!(back.enabled);
        assert_eq!(back.open(), Some(Group::Layers));
        assert!((back.sheet_fraction - 0.6).abs() < 1e-6);
    }
}
