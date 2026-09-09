use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use common::protocol::{Barrier, BarrierKindId, CarrierId, Eraser, Floor, MapLayout};

use super::surface::{clip_surface_rects, floor_bounds, surface_frame_rects};

const MERGE_EPSILON: f32 = 1e-4;

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualField {
    pub kind: Option<BarrierKindId>,
    pub carrier: CarrierId,
    pub level: u8,
    pub levels: u8,
    pub rect: Rect,
    pub thickness: f32,
    axis: usize,
    plane: f32,
}

impl VisualField {
    pub fn from_barrier(barrier: &Barrier) -> Self {
        let (axis, plane, rect) = segment_rect(
            Vec3::new(barrier.x1, barrier.y, barrier.z1),
            Vec3::new(barrier.x2, barrier.y + barrier.height, barrier.z2),
        );
        Self {
            kind: Some(barrier.kind),
            carrier: barrier.carrier,
            level: barrier.level,
            levels: barrier.levels,
            rect,
            thickness: barrier.width,
            axis,
            plane,
        }
    }

    pub fn from_eraser(eraser: &Eraser, floor_thickness: f32) -> Self {
        // The trigger fills a storey; its visible panel leaves the same ceiling space as a barrier.
        let (axis, plane, rect) = segment_rect(
            Vec3::new(eraser.x1, eraser.y, eraser.z1),
            Vec3::new(eraser.x2, eraser.y + eraser.height - floor_thickness, eraser.z2),
        );
        Self {
            kind: None,
            carrier: eraser.carrier,
            level: eraser.level,
            levels: 1,
            rect,
            thickness: eraser.width,
            axis,
            plane,
        }
    }

    pub fn transform(&self) -> Transform {
        let center = self.rect.center();
        let mut translation = Vec3::new(0.0, center.y, 0.0);
        translation[self.axis] = center.x;
        translation[2 - self.axis] = self.plane;
        Transform::from_translation(translation).with_rotation(if self.axis == 0 {
            Quat::IDENTITY
        } else {
            Quat::from_rotation_y(-FRAC_PI_2)
        })
    }

    pub fn panel_rects(&self, layout: &MapLayout) -> Vec<Rect> {
        self.exposed_rects(vec![self.rect], layout, 0.0)
    }

    pub fn frame_rects(&self, layout: &MapLayout) -> Vec<Rect> {
        self.exposed_rects(
            surface_frame_rects(&[self.rect], self.thickness),
            layout,
            self.thickness / 2.0,
        )
    }

    fn exposed_rects(&self, surfaces: Vec<Rect>, layout: &MapLayout, depth: f32) -> Vec<Rect> {
        clip_surface_rects(surfaces, layout, self.carrier, [self.axis, 1], self.plane, depth)
    }

    fn can_merge(&self, other: &Self, floors: &[Floor], floor_thickness: f32, stack: bool) -> bool {
        if self.kind != other.kind
            || self.carrier != other.carrier
            || self.axis != other.axis
            || !near(self.plane, other.plane)
            || !near(self.thickness, other.thickness)
        {
            return false;
        }
        if !stack {
            return near(self.rect.min.y, other.rect.min.y)
                && near(self.rect.max.y, other.rect.max.y)
                && self.rect.min.x.max(other.rect.min.x) <= self.rect.max.x.min(other.rect.max.x) + MERGE_EPSILON;
        }
        if !near(self.rect.min.x, other.rect.min.x) || !near(self.rect.max.x, other.rect.max.x) {
            return false;
        }
        let joint = self.rect.min.y.max(other.rect.min.y);
        if joint > self.rect.max.y.min(other.rect.max.y) + floor_thickness + MERGE_EPSILON {
            return false;
        }
        let mut covered: Vec<_> = floors
            .iter()
            .filter(|floor| floor.carrier == self.carrier)
            .filter_map(|floor| {
                let (min, max) = floor_bounds(floor);
                (near(floor.y, joint)
                    && min[2 - self.axis] <= self.plane
                    && self.plane <= max[2 - self.axis]
                    && min[self.axis] < self.rect.max.x
                    && self.rect.min.x < max[self.axis])
                    .then_some((min[self.axis], max[self.axis]))
            })
            .collect();
        covered.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Wall trim covers only the ends; a separating floor must span the entire opening.
        let mut end = self.rect.min.x;
        for (min, max) in covered {
            if min > end + MERGE_EPSILON {
                return true;
            }
            end = end.max(max);
        }
        end < self.rect.max.x - MERGE_EPSILON
    }
}

pub(crate) fn merge_fields(
    fields: impl IntoIterator<Item = VisualField>,
    floors: &[Floor],
    floor_thickness: f32,
) -> Vec<VisualField> {
    // Stack before joining neighbors so authored order cannot leave internal frames in a rectangular grid.
    let stacked = merge_runs(fields, floors, floor_thickness, true);
    merge_runs(stacked, floors, floor_thickness, false)
}

fn merge_runs(
    fields: impl IntoIterator<Item = VisualField>,
    floors: &[Floor],
    floor_thickness: f32,
    stack: bool,
) -> Vec<VisualField> {
    let mut merged: Vec<VisualField> = Vec::new();
    for mut field in fields {
        let mut index = 0;
        while index < merged.len() {
            if field.can_merge(&merged[index], floors, floor_thickness, stack) {
                let other = merged.swap_remove(index);
                let last_level = field
                    .level
                    .saturating_add(field.levels.saturating_sub(1))
                    .max(other.level.saturating_add(other.levels.saturating_sub(1)));
                field.level = field.level.min(other.level);
                field.levels = (last_level - field.level).saturating_add(1);
                field.rect.min = field.rect.min.min(other.rect.min);
                field.rect.max = field.rect.max.max(other.rect.max);
                index = 0;
            } else {
                index += 1;
            }
        }
        merged.push(field);
    }
    merged
}

fn segment_rect(start: Vec3, end: Vec3) -> (usize, f32, Rect) {
    let axis = if start.z == end.z { 0 } else { 2 };
    let min = start.min(end);
    let max = start.max(end);
    (axis, start[2 - axis], Rect::new(min[axis], min.y, max[axis], max.y))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= MERGE_EPSILON
}

#[cfg(test)]
mod tests {
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
}
