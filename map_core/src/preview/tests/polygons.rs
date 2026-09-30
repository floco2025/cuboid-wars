use std::f32::consts::FRAC_PI_2;

use super::*;

const SQUARE: [Vec2; 4] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(2.0, 0.0),
    Vec2::new(2.0, 2.0),
    Vec2::new(0.0, 2.0),
];

fn upright(center: Vec2, half_across: f32, half_along: f32) -> Rectangle {
    Rectangle {
        center,
        across: Vec2::X,
        half_across,
        along: Vec2::Y,
        half_along,
    }
}

#[test]
fn hull_keeps_only_the_corners_counter_clockwise() {
    let mut points = SQUARE.to_vec();
    points.extend([Vec2::new(1.0, 1.0), Vec2::new(1.0, 0.0), Vec2::new(2.0, 1.5)]);
    let hull = convex_hull(&points);
    assert_eq!(hull.len(), 4, "{hull:?}");
    assert!((area(&hull) - 4.0).abs() < 1e-5);
    let turns_left = (0..4).all(|index| {
        let (a, b, c) = (hull[index], hull[(index + 1) % 4], hull[(index + 2) % 4]);
        (b - a).perp_dot(c - b) > 0.0
    });
    assert!(turns_left, "{hull:?}");
}

#[test]
fn hull_of_one_place_or_one_line_is_a_point_or_its_two_ends() {
    assert_eq!(convex_hull(&[Vec2::ONE, Vec2::ONE, Vec2::ONE]), vec![Vec2::ONE]);
    let line = convex_hull(&[Vec2::new(1.0, 1.0), Vec2::new(3.0, 3.0), Vec2::new(2.0, 2.0)]);
    assert_eq!(line, vec![Vec2::new(1.0, 1.0), Vec2::new(3.0, 3.0)]);
    assert!(convex_hull(&[]).is_empty());
}

#[test]
fn clipping_keeps_the_part_inside_every_half_plane() {
    let half = clip(
        &SQUARE,
        &[HalfPlane {
            normal: Vec2::X,
            offset: 0.5,
        }],
    );
    assert!((area(&half) - 1.0).abs() < 1e-5, "{half:?}");
    let overlap = clip(
        &upright(Vec2::ZERO, 1.0, 2.0).corners(),
        &upright(Vec2::new(1.5, 1.0), 1.0, 2.0).sides(),
    );
    assert!((area(&overlap) - 0.5 * 3.0).abs() < 1e-5, "{overlap:?}");
    let apart = clip(&SQUARE, &upright(Vec2::new(9.0, 9.0), 1.0, 1.0).sides());
    assert!(apart.is_empty(), "{apart:?}");
}

#[test]
fn minkowski_sum_grows_a_shape_by_the_rectangle() {
    let rectangle = upright(Vec2::new(50.0, 50.0), 0.5, 1.0);
    let around_point = minkowski_sum(&[Vec2::new(3.0, 4.0)], &rectangle);
    assert!((area(&around_point) - 2.0).abs() < 1e-5, "{around_point:?}");
    let along_run = minkowski_sum(&[Vec2::ZERO, Vec2::new(3.0, 0.0)], &rectangle);
    assert!((area(&along_run) - (2.0 + 3.0 * 2.0)).abs() < 1e-5, "{along_run:?}");
}

#[test]
fn a_yaw_wedge_is_the_quarter_of_directions_that_snap_to_it() {
    let apex = Vec2::new(1.0, -2.0);
    let inside = |yaw: f32, point: Vec2| yaw_wedge(apex, yaw).iter().all(|plane| plane.excess(point) <= 0.0);
    // Yaw 0 faces +Z, the second coordinate; a quarter turn faces +X.
    assert!(inside(0.0, apex + Vec2::new(0.9, 1.0)));
    assert!(!inside(0.0, apex + Vec2::new(1.1, 1.0)));
    assert!(!inside(0.0, apex + Vec2::new(0.0, -1.0)));
    assert!(inside(FRAC_PI_2, apex + Vec2::new(1.0, -0.9)));
    assert!(!inside(FRAC_PI_2, apex + Vec2::new(-1.0, 0.0)));
    assert!(inside(3.0 * FRAC_PI_2, apex + Vec2::new(-1.0, 0.2)));
}
