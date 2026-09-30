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

fn boxed(x0: f32, y0: f32, x1: f32, y1: f32) -> Box2 {
    Box2 {
        low: Vec2::new(x0, y0),
        high: Vec2::new(x1, y1),
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
    let apart = clip(
        &SQUARE,
        &[HalfPlane {
            normal: Vec2::Y,
            offset: -1.0,
        }],
    );
    assert!(apart.is_empty(), "{apart:?}");
}

#[test]
fn a_rectangle_maps_between_its_frame_and_the_world() {
    let tilted = Rectangle {
        center: Vec2::new(3.0, 4.0),
        across: Vec2::new(0.0, 1.0),
        half_across: 0.5,
        along: Vec2::new(-1.0, 0.0),
        half_along: 2.0,
    };
    let bounds = tilted.bounds();
    assert_eq!((bounds.low, bounds.high), (Vec2::new(3.5, -5.0), Vec2::new(4.5, -1.0)));
    assert_eq!(tilted.world(tilted.local(Vec2::new(7.0, -2.0))), Vec2::new(7.0, -2.0));
    assert_eq!(
        upright(Vec2::ONE, 1.0, 2.0).grown(0.5).bounds().high,
        Vec2::new(2.5, 3.5)
    );
}

#[test]
fn boxes_unite_into_one_outline_with_the_shared_edges_gone() {
    let staircase = boxes_union(&[boxed(0.0, 0.0, 4.0, 2.0), boxed(1.0, 1.0, 3.0, 3.0)]);
    assert_eq!(staircase.len(), 1, "{staircase:?}");
    assert!((area(&staircase[0]) - (8.0 + 2.0)).abs() < 1e-5, "{staircase:?}");
    assert_eq!(staircase[0].len(), 8, "{staircase:?}");
    let turns_left: f32 = (0..8)
        .map(|index| {
            (staircase[0][(index + 1) % 8] - staircase[0][index])
                .perp_dot(staircase[0][(index + 2) % 8] - staircase[0][(index + 1) % 8])
        })
        .sum();
    assert!(turns_left > 0.0, "{staircase:?}");
    // One inside another, or two exactly alike, is the larger.
    let nested = boxes_union(&[
        boxed(0.0, 0.0, 4.0, 4.0),
        boxed(1.0, 1.0, 2.0, 2.0),
        boxed(0.0, 0.0, 4.0, 4.0),
    ]);
    assert_eq!(nested.len(), 1);
    assert!((area(&nested[0]) - 16.0).abs() < 1e-5, "{nested:?}");
    assert_eq!(nested[0].len(), 4, "{nested:?}");
    // Apart, they stay two outlines; touching along an edge, they merge.
    assert_eq!(
        boxes_union(&[boxed(0.0, 0.0, 1.0, 1.0), boxed(5.0, 5.0, 6.0, 6.0)]).len(),
        2
    );
    let touching = boxes_union(&[boxed(0.0, 0.0, 1.0, 1.0), boxed(1.0, 0.0, 2.0, 1.0)]);
    assert_eq!(touching.len(), 1, "{touching:?}");
    assert_eq!(touching[0].len(), 4, "{touching:?}");
    assert!(boxes_union(&[]).is_empty());
}

#[test]
fn simplifying_chamfers_fine_steps_and_keeps_coarse_ones() {
    let steps: Vec<Vec2> = (0..10)
        .flat_map(|step| {
            let (x, y) = (step as f32 * 0.1, step as f32 * 0.01);
            [Vec2::new(x, y), Vec2::new(x + 0.1, y)]
        })
        .chain([Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)])
        .collect();
    let smooth = simplify(steps.clone(), 0.02);
    assert!(smooth.len() <= 5, "{smooth:?}");
    assert!((area(&smooth) - area(&steps)).abs() < 0.05);
    assert_eq!(simplify(steps.clone(), 0.0), steps);
    assert_eq!(simplify(SQUARE.to_vec(), 0.5), SQUARE.to_vec());
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
