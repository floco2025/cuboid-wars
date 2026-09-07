use bevy::prelude::*;
use common::protocol::{CarrierId, Floor, MapLayout};

pub(crate) fn surface_frame_rects(surfaces: &[Rect], thickness: f32) -> Vec<Rect> {
    let half = thickness / 2.0;
    let mut frame = Vec::new();
    for (index, surface) in surfaces.iter().enumerate() {
        for axis in [0, 1] {
            let normal = 1 - axis;
            for (edge, positive) in [(surface.min[normal], false), (surface.max[normal], true)] {
                let mut spans = vec![(surface.min[axis], surface.max[axis])];
                for (other_index, other) in surfaces.iter().enumerate() {
                    let across = if positive {
                        other.min[normal] <= edge && edge < other.max[normal]
                    } else {
                        other.min[normal] < edge && edge <= other.max[normal]
                    };
                    if index == other_index || !across {
                        continue;
                    }
                    spans = spans
                        .into_iter()
                        .flat_map(|(min, max)| {
                            [(min, max.min(other.min[axis])), (min.max(other.max[axis]), max)]
                                .into_iter()
                                .filter(|(min, max)| min < max)
                        })
                        .collect();
                }
                for (min, max) in spans {
                    let mut rail = Rect::default();
                    rail.min[axis] = min - half;
                    rail.max[axis] = max + half;
                    rail.min[normal] = edge - half;
                    rail.max[normal] = edge + half;
                    // Assign each corner once so adjoining rails never have overlapping faces.
                    let mut parts = vec![rail];
                    for existing in &frame {
                        parts = parts
                            .into_iter()
                            .flat_map(|part| subtract_rect(part, *existing))
                            .collect();
                    }
                    frame.extend(parts);
                }
            }
        }
    }
    frame
}

pub(crate) fn clip_surface_rects(
    mut surfaces: Vec<Rect>,
    layout: &MapLayout,
    carrier: CarrierId,
    axes: [usize; 2],
    plane: f32,
    depth: f32,
) -> Vec<Rect> {
    let walls = layout.walls.iter().filter(|wall| wall.carrier == carrier).map(|wall| {
        let start = Vec3::new(wall.x1, wall.y, wall.z1);
        let end = Vec3::new(wall.x2, wall.y + wall.height, wall.z2);
        let pad = if wall.z1 == wall.z2 { Vec3::Z } else { Vec3::X } * (wall.width / 2.0);
        (start.min(end) - pad, start.max(end) + pad)
    });
    let floors = layout
        .floors
        .iter()
        .filter(|floor| floor.carrier == carrier)
        .map(floor_bounds);
    let normal = 3 - axes[0] - axes[1];
    for (min, max) in walls.chain(floors) {
        if min[normal] > plane + depth || max[normal] < plane - depth {
            continue;
        }
        let cut = Rect::new(min[axes[0]], min[axes[1]], max[axes[0]], max[axes[1]]);
        surfaces = surfaces
            .into_iter()
            .flat_map(|surface| subtract_rect(surface, cut))
            .collect();
    }
    surfaces
}

pub(crate) fn floor_bounds(floor: &Floor) -> (Vec3, Vec3) {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
    (
        Vec3::new(min_x, floor.y - floor.thickness, min_z),
        Vec3::new(max_x, floor.y, max_z),
    )
}

fn subtract_rect(surface: Rect, cut: Rect) -> Vec<Rect> {
    let min = surface.min.max(cut.min);
    let max = surface.max.min(cut.max);
    if min.x >= max.x || min.y >= max.y {
        return vec![surface];
    }
    [
        Rect {
            min: surface.min,
            max: Vec2::new(surface.max.x, min.y),
        },
        Rect {
            min: Vec2::new(surface.min.x, max.y),
            max: surface.max,
        },
        Rect {
            min: Vec2::new(surface.min.x, min.y),
            max: Vec2::new(min.x, max.y),
        },
        Rect {
            min: Vec2::new(max.x, min.y),
            max: Vec2::new(surface.max.x, max.y),
        },
    ]
    .into_iter()
    .filter(|rect| rect.width() > 0.0 && rect.height() > 0.0)
    .collect()
}
