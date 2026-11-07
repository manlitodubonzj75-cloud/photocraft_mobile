//! Unified navigation delta for 2-finger gestures: Pan, Zoom, and Rotation around a centroid.

use egui::{Pos2, Vec2};

/// A combined 2-finger navigation transformation delta for one step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureDelta {
    /// The focal center / centroid of the multi-touch gesture in screen coordinates.
    pub centroid: Pos2,
    /// Screen-space translation delta (panning).
    pub translation: Vec2,
    /// Scale factor multiplier (e.g. 1.05 for 5% zoom-in, 0.95 for zoom-out).
    pub scale: f32,
    /// Rotation angle delta in radians.
    pub rotation_rad: f32,
}

impl Default for GestureDelta {
    fn default() -> Self {
        Self { centroid: Pos2::ZERO, translation: Vec2::ZERO, scale: 1.0, rotation_rad: 0.0 }
    }
}

impl GestureDelta {
    /// Compute a delta from previous and current 2-touch sample pairs.
    pub fn from_two_touches(prev_a: Pos2, prev_b: Pos2, curr_a: Pos2, curr_b: Pos2) -> Self {
        let prev_center = Pos2::new((prev_a.x + prev_b.x) * 0.5, (prev_a.y + prev_b.y) * 0.5);
        let curr_center = Pos2::new((curr_a.x + curr_b.x) * 0.5, (curr_a.y + curr_b.y) * 0.5);

        let translation = curr_center - prev_center;

        let prev_dist = (prev_a - prev_b).length().max(1.0);
        let curr_dist = (curr_a - curr_b).length().max(1.0);
        let scale = curr_dist / prev_dist;

        let prev_angle = (prev_b.y - prev_a.y).atan2(prev_b.x - prev_a.x);
        let curr_angle = (curr_b.y - curr_a.y).atan2(curr_b.x - curr_a.x);
        let mut rotation_rad = curr_angle - prev_angle;

        // Wrap angle difference into -PI .. PI
        while rotation_rad > std::f32::consts::PI {
            rotation_rad -= 2.0 * std::f32::consts::PI;
        }
        while rotation_rad < -std::f32::consts::PI {
            rotation_rad += 2.0 * std::f32::consts::PI;
        }

        Self { centroid: curr_center, translation, scale, rotation_rad }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn pure_pan_produces_translation_only() {
        let prev_a = pos2(100.0, 100.0);
        let prev_b = pos2(200.0, 100.0);
        let curr_a = pos2(120.0, 110.0);
        let curr_b = pos2(220.0, 110.0);

        let delta = GestureDelta::from_two_touches(prev_a, prev_b, curr_a, curr_b);
        assert!((delta.translation.x - 20.0).abs() < 1e-4);
        assert!((delta.translation.y - 10.0).abs() < 1e-4);
        assert!((delta.scale - 1.0).abs() < 1e-4);
        assert!(delta.rotation_rad.abs() < 1e-4);
    }

    #[test]
    fn pure_pinch_produces_scale_around_center() {
        let prev_a = pos2(100.0, 100.0);
        let prev_b = pos2(200.0, 100.0); // distance 100
        let curr_a = pos2(50.0, 100.0);
        let curr_b = pos2(250.0, 100.0); // distance 200

        let delta = GestureDelta::from_two_touches(prev_a, prev_b, curr_a, curr_b);
        assert_eq!(delta.centroid, pos2(150.0, 100.0));
        assert!((delta.scale - 2.0).abs() < 1e-4);
        assert!(delta.translation.length() < 1e-4);
    }
}
