//! Mobile shell support for PhotoCraft.
//!
//! This module adapts the *existing* desktop shell (`crate::app`) to smartphones and tablets
//! without touching the engine, the document model, the tools, the renderer or the panels. It is
//! deliberately a **layout and input policy** layer: the mobile host app owns a
//! [`crate::PhotocraftApp`] exactly as `apps/photocraft-web` owns one, and this module only decides
//! *where* the existing pieces are drawn and *how* touch input is interpreted.
//!
//! See `docs/mobile-ui-mapping.md` for the component-to-architecture mapping and
//! `docs/mobile-ui-plan.md` for the slice plan.

pub mod chrome;
pub mod device;
pub mod panels;
pub mod shell;
pub mod state;
pub mod viewport;

pub use device::{DeviceClass, Orientation};
pub use panels::{HostKind, group_label, group_rail_icon, group_tabs, host_close_button, host_header, panel_host};
pub use shell::{MobileShell, MobileShellLayout, OpenPanels, Region, RegionId, SheetHost, metrics, regions};
pub use state::MobileUiState;
pub use viewport::MobileViewport;

#[cfg(test)]
mod shell_tests;
