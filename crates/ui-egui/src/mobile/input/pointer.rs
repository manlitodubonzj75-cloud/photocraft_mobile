//! Normalized pointer primitives for multi-device touch and stylus input.
//!
//! Abstracts differences between desktop mouse, direct touch screen fingers,
//! Apple Pencil, S-Pen, and generic styluses into unified [`PointerSample`]s.

use egui::{Pos2, Vec2};

/// Persistent key uniquely identifying a physical touch/pointer contact across devices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContactKey {
    pub device_id: u64,
    pub contact_id: u64,
}

impl ContactKey {
    pub const fn new(device_id: u64, contact_id: u64) -> Self {
        Self { device_id, contact_id }
    }
}

impl From<u64> for ContactKey {
    fn from(contact_id: u64) -> Self {
        Self::new(0, contact_id)
    }
}

/// The physical tool type producing pointer input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerKind {
    /// A human finger on a capacitive touchscreen.
    Finger,
    /// An active or passive stylus/pen (Apple Pencil, S-Pen, Wacom digitizer).
    Stylus,
    /// The eraser tip of a dual-ended stylus.
    Eraser,
    /// A mouse cursor or touchpad pointer.
    Mouse,
}

impl Default for PointerKind {
    fn default() -> Self {
        Self::Finger
    }
}

/// The lifecycle phase of a pointer contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerPhase {
    /// Initial contact with the digitizer/screen.
    Down,
    /// Continuous movement while contacting the screen.
    Move,
    /// Clean release/lift off the screen.
    Up,
    /// Abrupt termination by the OS (incoming phone call, palm rejection, gesture navigation).
    Cancel,
}

/// A normalized, time-stamped pointer sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerSample {
    /// Hardware device identifier producing this contact (0 for default single device).
    pub device_id: u64,
    /// Persistent identifier for this contact (e.g. Android pointer ID, iOS UITouch pointer).
    pub id: u64,
    /// Device type (Finger, Stylus, etc.).
    pub kind: PointerKind,
    /// Event phase.
    pub phase: PointerPhase,
    /// Screen-space position in logical points.
    pub position: Pos2,
    /// Monotonic timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Normalized pressure in 0.0 ..= 1.0 (if supported by hardware).
    pub pressure: Option<f32>,
    /// Altitude / tilt angle vector (if supported by stylus).
    pub tilt: Option<Vec2>,
}

impl PointerSample {
    pub fn new(id: u64, kind: PointerKind, phase: PointerPhase, position: Pos2, timestamp_ms: u64) -> Self {
        Self { device_id: 0, id, kind, phase, position, timestamp_ms, pressure: None, tilt: None }
    }

    pub fn with_device_id(mut self, device_id: u64) -> Self {
        self.device_id = device_id;
        self
    }

    pub fn with_pressure(mut self, pressure: f32) -> Self {
        self.pressure = Some(pressure.clamp(0.0, 1.0));
        self
    }

    pub fn with_tilt(mut self, tilt: Vec2) -> Self {
        self.tilt = Some(tilt);
        self
    }

    pub fn key(&self) -> ContactKey {
        ContactKey::new(self.device_id, self.id)
    }
}
