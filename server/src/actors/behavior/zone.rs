use common::protocol::Position;

// Euclidean xz-distance from `pos` to the nearest edge of the
// `(min_x, min_z, max_x, max_z)` rectangle. Inside the rectangle returns 0.
pub(super) fn xz_distance_from_rect(pos: &Position, bounds: (f32, f32, f32, f32)) -> f32 {
    let (min_x, min_z, max_x, max_z) = bounds;
    let dx = (min_x - pos.x).max(0.0).max(pos.x - max_x);
    let dz = (min_z - pos.z).max(0.0).max(pos.z - max_z);
    dx.hypot(dz)
}

// Closest point on the rectangle (in xz) to `pos`. y is preserved so the
// actor doesn't try to climb to a different floor.
pub(super) fn closest_point_in_rect(pos: &Position, bounds: (f32, f32, f32, f32)) -> Position {
    let (min_x, min_z, max_x, max_z) = bounds;
    Position {
        x: pos.x.clamp(min_x, max_x),
        y: pos.y,
        z: pos.z.clamp(min_z, max_z),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: f32, z: f32) -> Position {
        Position { x, y: 0.0, z }
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {actual} to be near {expected}"
        );
    }

    #[test]
    fn xz_distance_from_rect_zero_inside() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        assert_eq!(xz_distance_from_rect(&pos(0.0, 0.0), bounds), 0.0);
        assert_eq!(xz_distance_from_rect(&pos(-2.0, -2.0), bounds), 0.0);
        assert_eq!(xz_distance_from_rect(&pos(2.0, 2.0), bounds), 0.0);
        assert_eq!(xz_distance_from_rect(&pos(1.5, -1.5), bounds), 0.0);
    }

    #[test]
    fn xz_distance_from_rect_axis_aligned_outside() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        assert_near(xz_distance_from_rect(&pos(5.0, 0.0), bounds), 3.0); // east
        assert_near(xz_distance_from_rect(&pos(-5.0, 0.0), bounds), 3.0); // west
        assert_near(xz_distance_from_rect(&pos(0.0, 5.0), bounds), 3.0); // south
        assert_near(xz_distance_from_rect(&pos(0.0, -5.0), bounds), 3.0); // north
    }

    #[test]
    fn xz_distance_from_rect_corner() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        // Point at (5, 6): nearest corner is (2, 2). dx=3, dz=4 -> 5.
        assert_near(xz_distance_from_rect(&pos(5.0, 6.0), bounds), 5.0);
    }

    #[test]
    fn closest_point_in_rect_inside_returns_unchanged() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        let p = Position {
            x: 1.0,
            y: 7.5,
            z: -0.5,
        };
        let clamped = closest_point_in_rect(&p, bounds);
        assert_eq!(
            clamped,
            Position {
                x: 1.0,
                y: 7.5,
                z: -0.5
            }
        );
    }

    #[test]
    fn closest_point_in_rect_clamps_to_edge() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        let p = Position { x: 5.0, y: 7.5, z: 0.0 };
        let clamped = closest_point_in_rect(&p, bounds);
        assert_eq!(clamped, Position { x: 2.0, y: 7.5, z: 0.0 });
    }

    #[test]
    fn closest_point_in_rect_clamps_to_corner() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        let p = Position {
            x: 5.0,
            y: 7.5,
            z: -10.0,
        };
        let clamped = closest_point_in_rect(&p, bounds);
        assert_eq!(
            clamped,
            Position {
                x: 2.0,
                y: 7.5,
                z: -2.0
            }
        );
    }

    #[test]
    fn closest_point_in_rect_preserves_y() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        let p = Position {
            x: 100.0,
            y: 42.0,
            z: 100.0,
        };
        let clamped = closest_point_in_rect(&p, bounds);
        assert_eq!(clamped.y, 42.0);
    }
}
