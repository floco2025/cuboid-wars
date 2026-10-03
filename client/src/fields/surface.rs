use bevy::prelude::*;
use common::{physics::Solid, protocol::CarrierId};

pub(crate) fn surface_frame_rects(surfaces: &[Rect], thickness: f32) -> Vec<Rect> {
    let mut frame = Vec::new();
    for edge in surface_edges(surfaces) {
        let rail = Rect {
            min: edge.min - thickness / 2.0,
            max: edge.max + thickness / 2.0,
        };
        let mut parts = vec![rail];
        for existing in &frame {
            parts = parts
                .into_iter()
                .flat_map(|part| subtract_rect(part, *existing))
                .collect();
        }
        frame.extend(parts);
    }
    frame
}

pub(super) fn surface_edges(surfaces: &[Rect]) -> Vec<Rect> {
    let mut edges = Vec::new();
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
                    let mut line = Rect::default();
                    line.min[axis] = min;
                    line.max[axis] = max;
                    line.min[normal] = edge;
                    line.max[normal] = edge;
                    edges.push(line);
                }
            }
        }
    }
    edges
}

// How far a solid may stop short of the surface's slab and still cut it, and
// the thinnest piece a cut leaves: collider bounds carry float rounding the
// surfaces' own grid coordinates do not.
const CLIP_EPSILON: f32 = 1e-4;

// The parts of the surfaces no structural box on their carrier covers: the
// surfaces lie in the plane `plane` across `axes`, and a box cuts them where
// it comes within `depth` of that plane.
pub(crate) fn clip_surface_rects(
    mut surfaces: Vec<Rect>,
    solids: &[Solid],
    carrier: CarrierId,
    axes: [usize; 2],
    plane: f32,
    depth: f32,
) -> Vec<Rect> {
    let normal = 3 - axes[0] - axes[1];
    let reach = depth + CLIP_EPSILON;
    for solid in solids.iter().filter(|solid| solid.carrier == carrier && solid.is_box()) {
        let (min, max) = (solid.min, solid.max);
        if min[normal] > plane + reach || max[normal] < plane - reach {
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
    .filter(|rect| rect.width() > CLIP_EPSILON && rect.height() > CLIP_EPSILON)
    .collect()
}
