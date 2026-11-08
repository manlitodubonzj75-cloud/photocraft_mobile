//! Unified navigation delta for 2-finger gestures: Pan, Zoom, and Rotation around a centroid.

use egui::{Pos2, Vec2, pos2};

use super::pointer::ContactKey;

/// Minimum distance in points between two contacts before computing zoom/scale deltas.
pub const MIN_SPAN_PT: f32 = 10.0;

/// A combined 2-finger navigation transformation delta for one step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureDelta {
    /// The focal center / centroid of the multi-touch gesture in screen coordinates.
    pub centroid: Pos2,
    /// Screen-space translation delta (panning).
    pub translation: Vec2,
    /// Scale factor multiplier (e.g. 1.05 for 5% zoom-in, 0.95 for zoom-out).
    pub scale: f32,
    /// Rotation angle delta in radians. (0.0 when rotation is disabled).
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
        let prev_center = pos2((prev_a.x + prev_b.x) * 0.5, (prev_a.y + prev_b.y) * 0.5);
        let curr_center = pos2((curr_a.x + curr_b.x) * 0.5, (curr_a.y + curr_b.y) * 0.5);

        let mut translation = curr_center - prev_center;
        if !translation.is_finite() {
            translation = Vec2::ZERO;
        }

        let prev_dist = (prev_a - prev_b).length();
        let curr_dist = (curr_a - curr_b).length();

        // Guard against near-zero span or NaN: if fingers are too close, suppress scale changes.
        let scale = if prev_dist >= MIN_SPAN_PT && curr_dist >= MIN_SPAN_PT {
            let s = curr_dist / prev_dist;
            if s.is_finite() && s > 0.0 { s } else { 1.0 }
        } else {
            1.0
        };

        // Rotation is explicitly 0.0 for this stage.
        let rotation_rad = 0.0;

        let centroid = if curr_center.to_vec2().is_finite() { curr_center } else { Pos2::ZERO };

        Self { centroid, translation, scale, rotation_rad }
    }
}

/// State tracker maintaining stable two-contact pair order and baseline geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct TwoFingerNavTracker {
    /// The canonical pair of contacts, sorted by ContactKey for stability.
    pub pair: [ContactKey; 2],
    /// Previous positions for [contact_0, contact_1].
    pub prev_positions: [Pos2; 2],
    /// Current positions for [contact_0, contact_1].
    pub curr_positions: [Pos2; 2],
    /// Initial positions when the two contacts were first established.
    pub initial_positions: [Pos2; 2],
}

impl TwoFingerNavTracker {
    /// Create a new tracker for two contacts.
    /// Canonical ordering ensures (ID 7, ID 42) and (ID 42, ID 7) produce identical mappings.
    pub fn new(k0: ContactKey, pos0: Pos2, k1: ContactKey, pos1: Pos2) -> Self {
        let (pair, positions) = if k0 <= k1 { ([k0, k1], [pos0, pos1]) } else { ([k1, k0], [pos1, pos0]) };
        Self { pair, prev_positions: positions, curr_positions: positions, initial_positions: positions }
    }

    /// Reset baseline (e.g. when transitioning into navigation to avoid jump).
    pub fn reset_baseline(&mut self) {
        self.prev_positions = self.curr_positions;
    }

    /// Update one contact's position and compute the step delta.
    /// Returns None if the key is not one of the tracked pair.
    pub fn update_contact(&mut self, key: ContactKey, new_pos: Pos2) -> Option<GestureDelta> {
        let idx = if self.pair[0] == key {
            0
        } else if self.pair[1] == key {
            1
        } else {
            return None;
        };

        self.prev_positions = self.curr_positions;
        self.curr_positions[idx] = new_pos;

        Some(GestureDelta::from_two_touches(self.prev_positions[0], self.prev_positions[1], self.curr_positions[0], self.curr_positions[1]))
    }

    /// Maximum distance moved by either contact from its initial touch-down position.
    pub fn max_travel_from_initial(&self) -> f32 {
        let d0 = (self.curr_positions[0] - self.initial_positions[0]).length();
        let d1 = (self.curr_positions[1] - self.initial_positions[1]).length();
        d0.max(d1)
    }

    /// Current span (distance between the two contacts).
    pub fn current_span(&self) -> f32 {
        (self.curr_positions[0] - self.curr_positions[1]).length()
    }

    /// Centroid of the two contacts.
    pub fn centroid(&self) -> Pos2 {
        pos2((self.curr_positions[0].x + self.curr_positions[1].x) * 0.5, (self.curr_positions[0].y + self.curr_positions[1].y) * 0.5)
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
        assert_eq!(delta.rotation_rad, 0.0);
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

    #[test]
    fn near_zero_span_does_not_produce_wild_scale() {
        let prev_a = pos2(100.0, 100.0);
        let prev_b = pos2(102.0, 100.0); // distance 2 < MIN_SPAN_PT
        let curr_a = pos2(100.0, 100.0);
        let curr_b = pos2(105.0, 100.0);

        let delta = GestureDelta::from_two_touches(prev_a, prev_b, curr_a, curr_b);
        assert_eq!(delta.scale, 1.0);
    }

    #[test]
    fn order_invariance_for_pair_keys() {
        let k1 = ContactKey::new(1, 7);
        let k2 = ContactKey::new(2, 42);

        let pos1_start = pos2(100.0, 100.0);
        let pos2_start = pos2(200.0, 200.0);

        let mut tracker1 = TwoFingerNavTracker::new(k1, pos1_start, k2, pos2_start);
        let mut tracker2 = TwoFingerNavTracker::new(k2, pos2_start, k1, pos1_start);

        let d1 = tracker1.update_contact(k1, pos2(110.0, 105.0)).unwrap();
        let d2 = tracker2.update_contact(k1, pos2(110.0, 105.0)).unwrap();

        assert_eq!(d1, d2);
    }
}
