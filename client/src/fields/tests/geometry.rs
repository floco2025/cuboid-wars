use common::protocol::CheckpointKind;
#[test]
fn checkpoint_perimeter_has_four_vertical_sides_and_an_open_top() {
    let checkpoint = Checkpoint {
        kind: CheckpointKind::Individual,
        carrier: CarrierId::WORLD,
        level: 2,
        min_x: 1.0,
        max_x: 7.0,
        min_z: 3.0,
        max_z: 11.0,
        y: 8.0,
    };
    let fields = VisualField::checkpoint_perimeter(&checkpoint, 0.9, 0.05);
    for field in fields {
        assert_eq!(field.carrier, checkpoint.carrier);
        assert_eq!(field.level, 2);
        assert_eq!(field.rect.min.y, 8.0);
        assert!((field.rect.max.y - 8.9).abs() < 1e-5);
        assert!(field.axis == 0 || field.axis == 2);
    }
    assert_eq!([fields[0].plane, fields[1].plane], [3.0, 11.0]);
    assert_eq!([fields[2].plane, fields[3].plane], [1.0, 7.0]);
    assert_eq!(fields[0].rect.width(), 6.0);
    assert_eq!(fields[2].rect.width(), 8.0);
}

use super::*;
use common::protocol::Wall;

fn barrier() -> Barrier {
    Barrier {
        x1: 0.0,
        z1: 0.0,
        x2: 4.0,
        z2: 0.0,
        y: 0.0,
        height: 3.5,
        width: 0.1,
        level: 0,
        levels: 1,
        kind: BarrierKindId(0),
        carrier: CarrierId::WORLD,
    }
}

fn field(kind: Option<BarrierKindId>) -> VisualField {
    VisualField {
        kind,
        ..VisualField::from_barrier(&barrier())
    }
}

fn floor(y: f32) -> Floor {
    Floor {
        x1: -0.5,
        x2: 4.5,
        z1: -0.5,
        z2: 0.5,
        y,
        thickness: 0.5,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn overlap_area(a: Rect, b: Rect) -> f32 {
    let size = (a.max.min(b.max) - a.min.max(b.min)).max(Vec2::ZERO);
    size.x * size.y
}

#[test]
fn barriers_and_erasers_have_identical_visual_bounds_in_both_axes_and_directions() {
    for (dx, dz) in [(4.0, 0.0), (-4.0, 0.0), (0.0, 4.0), (0.0, -4.0)] {
        let barrier = Barrier {
            x1: 2.0,
            z1: 3.0,
            x2: 2.0 + dx,
            z2: 3.0 + dz,
            ..barrier()
        };
        let eraser = Eraser {
            x1: barrier.x1,
            z1: barrier.z1,
            x2: barrier.x2,
            z2: barrier.z2,
            y: barrier.y,
            height: 4.0,
            width: barrier.width,
            level: barrier.level,
            carrier: barrier.carrier,
        };
        let barrier_field = VisualField::from_barrier(&barrier);
        let eraser_field = VisualField::from_eraser(&eraser, 0.5);
        assert_eq!(barrier_field.rect, eraser_field.rect);
        assert_eq!(barrier_field.transform(), eraser_field.transform());
        for field in [barrier_field, eraser_field] {
            let transform = field.transform();
            for y in [0.0, barrier.height] {
                let points = [-field.rect.width() / 2.0, field.rect.width() / 2.0]
                    .map(|x| transform.transform_point(Vec3::new(x, y - barrier.height / 2.0, 0.0)));
                for expected in [
                    Vec3::new(barrier.x1, y, barrier.z1),
                    Vec3::new(barrier.x2, y, barrier.z2),
                ] {
                    assert!(points.iter().any(|point| point.abs_diff_eq(expected, 1e-5)));
                }
            }
        }
    }
}

#[test]
fn adjacent_fields_merge_without_internal_frames_but_not_across_kinds_or_carriers() {
    for kind in [None, Some(BarrierKindId(0))] {
        let a = field(kind);
        let b = VisualField {
            rect: Rect::new(4.0, 0.0, 8.0, 3.5),
            ..a
        };
        let c = VisualField {
            rect: Rect::new(8.0, 0.0, 12.0, 3.5),
            ..a
        };
        let merged = merge_fields([c, a, b], &[], 0.5);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].rect, Rect::new(0.0, 0.0, 12.0, 3.5));
        assert_eq!(merged[0].frame_rects(&MapLayout::default()).len(), 4);
        for other in [
            VisualField {
                kind: Some(BarrierKindId(1)),
                ..b
            },
            VisualField {
                carrier: CarrierId(1),
                ..b
            },
            VisualField {
                rect: Rect::new(4.1, 0.0, 8.1, 3.5),
                ..b
            },
        ] {
            assert_eq!(merge_fields([a, other], &[], 0.5).len(), 2);
        }
    }
}

#[test]
fn scrambled_rectangular_grids_merge_to_one_outer_frame() {
    let order = [(0, 0), (1, 0), (1, 1), (2, 1), (2, 2), (1, 2), (0, 2), (0, 1), (2, 0)];
    for kind in [None, Some(BarrierKindId(0))] {
        let fields = order.map(|(column, level)| VisualField {
            rect: Rect::new(
                column as f32 * 4.0,
                level as f32 * 4.0,
                (column + 1) as f32 * 4.0,
                level as f32 * 4.0 + 3.5,
            ),
            level,
            ..field(kind)
        });
        let merged = merge_fields(fields, &[], 0.5);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].rect, Rect::new(0.0, 0.0, 12.0, 11.5));
        assert_eq!(merged[0].levels, 3);
    }
}

#[test]
fn stacked_fields_bridge_floorless_gaps_and_keep_floor_separated_storeys_apart() {
    for kind in [None, Some(BarrierKindId(0))] {
        let lower = field(kind);
        let upper = VisualField {
            rect: Rect::new(0.0, 4.0, 4.0, 7.5),
            level: 1,
            ..lower
        };
        let merged = merge_fields([lower, upper], &[], 0.5);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].rect, Rect::new(0.0, 0.0, 4.0, 7.5));
        assert_eq!(merged[0].levels, 2);
        assert_eq!(merge_fields([upper, lower], &[floor(4.0)], 0.5).len(), 2);
    }
}

#[test]
fn stacked_fields_merge_between_wall_trim_without_leaving_an_open_seam() {
    for kind in [None, Some(BarrierKindId(0))] {
        for axis in [0, 2] {
            let lower = VisualField { axis, ..field(kind) };
            let upper = VisualField {
                rect: Rect::new(0.0, 4.0, 4.0, 7.5),
                level: 1,
                ..lower
            };
            let mut floors = vec![
                Floor {
                    x1: -4.0,
                    x2: 0.25,
                    ..floor(4.0)
                },
                Floor {
                    x1: 3.75,
                    x2: 8.0,
                    ..floor(4.0)
                },
            ];
            if axis == 2 {
                for floor in &mut floors {
                    (floor.x1, floor.z1) = (floor.z1, floor.x1);
                    (floor.x2, floor.z2) = (floor.z2, floor.x2);
                }
            }
            let walls = floors
                .iter()
                .flat_map(|floor| {
                    [0.0, 4.0].map(|y| Wall {
                        x1: if axis == 0 { floor.x1 } else { 0.0 },
                        x2: if axis == 0 { floor.x2 } else { 0.0 },
                        z1: if axis == 2 { floor.z1 } else { 0.0 },
                        z2: if axis == 2 { floor.z2 } else { 0.0 },
                        y,
                        height: 3.5,
                        width: 1.0,
                        level: u8::from(y > 0.0),
                        carrier: floor.carrier,
                    })
                })
                .collect();
            let layout = MapLayout {
                walls,
                floors,
                ..Default::default()
            };
            let merged = merge_fields([upper, lower], &layout.floors, 0.5);
            assert_eq!(merged.len(), 1);
            let seam = Rect::new(0.25, 3.5, 3.75, 4.0);
            let covered: f32 = merged[0]
                .panel_rects(&layout)
                .iter()
                .map(|rect| overlap_area(*rect, seam))
                .sum();
            assert_eq!(covered, seam.width() * seam.height());
            assert!(
                merged[0]
                    .frame_rects(&layout)
                    .iter()
                    .all(|rect| overlap_area(*rect, seam) == 0.0)
            );
        }
    }
}

#[test]
fn adjoining_floor_pieces_together_separate_stacked_fields() {
    let lower = field(None);
    let upper = VisualField {
        rect: Rect::new(0.0, 4.0, 4.0, 7.5),
        level: 1,
        ..lower
    };
    let floors = [Floor { x1: 2.0, ..floor(4.0) }, Floor { x2: 2.0, ..floor(4.0) }];
    assert_eq!(merge_fields([lower, upper], &floors, 0.5).len(), 2);
    let other_carrier = floors.map(|floor| Floor {
        carrier: CarrierId(1),
        ..floor
    });
    assert_eq!(merge_fields([lower, upper], &other_carrier, 0.5).len(), 1);
}

#[test]
fn free_standing_frame_has_complete_corners_without_overlapping_faces() {
    let field = field(None);
    let rects = field.frame_rects(&MapLayout::default());
    let area: f32 = rects.iter().map(|rect| rect.width() * rect.height()).sum();
    let expected = 2.0 * field.thickness * (field.rect.width() + field.rect.height());
    assert!((area - expected).abs() < 1e-5);
    for (index, rect) in rects.iter().enumerate() {
        for other in &rects[index + 1..] {
            assert_eq!(overlap_area(*rect, *other), 0.0);
        }
    }
}

#[test]
fn walls_hide_only_the_covered_frame_and_pane_sections() {
    let field = field(None);
    let wall = Wall {
        x1: 0.0,
        x2: 0.0,
        z1: -1.0,
        z2: 1.0,
        width: 0.5,
        y: 0.0,
        height: 2.0,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let layout = MapLayout {
        walls: vec![wall],
        floors: vec![floor(0.0)],
        ..Default::default()
    };
    let covered = Rect::new(-0.25, 0.0, 0.25, 2.0);
    for rect in field.frame_rects(&layout).into_iter().chain(field.panel_rects(&layout)) {
        assert_eq!(overlap_area(rect, covered), 0.0);
        assert!(rect.min.y >= 0.0);
    }
    assert!(
        field
            .frame_rects(&layout)
            .iter()
            .any(|rect| rect.contains(Vec2::new(0.0, 2.5)))
    );
    let other_carrier = MapLayout {
        walls: vec![Wall {
            carrier: CarrierId(1),
            ..wall
        }],
        ..Default::default()
    };
    assert_eq!(
        field.frame_rects(&other_carrier),
        field.frame_rects(&MapLayout::default())
    );
    let coplanar = MapLayout {
        walls: vec![Wall {
            x1: -1.0,
            x2: 5.0,
            z1: 0.25,
            z2: 0.25,
            height: 4.0,
            ..wall
        }],
        ..Default::default()
    };
    assert!(field.panel_rects(&coplanar).is_empty());
}
