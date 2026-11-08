//! Touch and multi-touch input routing module for mobile devices.

pub mod canvas_nav;
pub mod gesture_arena;
pub mod pointer;
pub mod router;
pub mod stroke_adapter;

#[cfg(test)]
mod integration_tests;

pub use canvas_nav::{GestureDelta, TwoFingerNavTracker};
pub use gesture_arena::{GestureAction, GestureArena, GestureState};
pub use pointer::{ContactKey, PointerKind, PointerPhase, PointerSample};
pub use router::{CanvasGestureState, ContactState, GestureConfig, InputOwner, LayoutSnapshot, TouchAction, TouchRouter};
pub use stroke_adapter::{StrokeAdapter, StrokePhase, StrokeTransaction};
