use bevy::prelude::*;

use super::variants::{ScorchVariant, ScorchVertex};

// A half-plane of the mark's plane: keeps the points with `normal · p <= offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HalfPlane {
    pub(super) normal: Vec2,
    pub(super) offset: f32,
}

impl HalfPlane {
    fn excess(self, point: Vec2) -> f32 {
        self.normal.dot(point) - self.offset
    }
}

// The intersection of its half-planes.
pub(super) type Convex = Vec<HalfPlane>;

// Where a mark shows: on any `keep` region (anywhere when none is listed),
// and off every `cut` region.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ClipRegion {
    pub(super) keep: Vec<Convex>,
    pub(super) cut: Vec<Convex>,
}

type Polygon = Vec<ScorchVertex>;

impl ClipRegion {
    pub(crate) fn contains(&self, point: Vec2) -> bool {
        (self.keep.is_empty() || self.keep.iter().any(|region| inside(region, point)))
            && !self.cut.iter().any(|region| inside(region, point))
    }

    pub(super) fn apply(&self, variant: &ScorchVariant) -> ScorchVariant {
        let mut clipped = ScorchVariant::default();
        for triangle in &variant.triangles {
            let polygon: Polygon = triangle.iter().map(|&index| variant.vertices[index as usize]).collect();
            let mut pieces = if self.keep.is_empty() {
                vec![polygon]
            } else {
                let mut pieces = Vec::new();
                for (index, region) in self.keep.iter().enumerate() {
                    let Some(piece) = intersect(&polygon, region) else {
                        continue;
                    };
                    // Where keep regions overlap, the earlier one already drew the overlap.
                    let mut distinct = vec![piece];
                    for earlier in &self.keep[..index] {
                        distinct = distinct.iter().flat_map(|piece| subtract(piece, earlier)).collect();
                    }
                    pieces.extend(distinct);
                }
                pieces
            };
            for region in &self.cut {
                pieces = pieces.iter().flat_map(|piece| subtract(piece, region)).collect();
            }
            for piece in &pieces {
                push_polygon(&mut clipped, piece);
            }
        }
        clipped
    }
}

fn inside(region: &Convex, point: Vec2) -> bool {
    region.iter().all(|half| half.excess(point) <= 0.0)
}

// Pieces thinner than rounding noise along a cut are dropped, not drawn.
const MIN_PIECE_AREA: f32 = 1e-7;

fn push_polygon(variant: &mut ScorchVariant, polygon: &[ScorchVertex]) {
    if polygon.len() < 3 || polygon_area(polygon) < MIN_PIECE_AREA {
        return;
    }
    let first = variant.vertices.len() as u32;
    variant.vertices.extend_from_slice(polygon);
    for corner in 1..polygon.len() as u32 - 1 {
        variant.triangles.push([first, first + corner, first + corner + 1]);
    }
}

fn polygon_area(polygon: &[ScorchVertex]) -> f32 {
    let origin = polygon[0].position;
    polygon
        .windows(2)
        .map(|edge| (edge[0].position - origin).perp_dot(edge[1].position - origin))
        .sum::<f32>()
        .abs()
        * 0.5
}

// Both sides of a polygon cut by a half-plane; a crossing edge gains its
// intersection on both sides.
fn split(polygon: &[ScorchVertex], half: HalfPlane) -> (Polygon, Polygon) {
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    for (index, &current) in polygon.iter().enumerate() {
        let next = polygon[(index + 1) % polygon.len()];
        let current_excess = half.excess(current.position);
        let next_excess = half.excess(next.position);
        if current_excess <= 0.0 {
            inside.push(current);
        } else {
            outside.push(current);
        }
        if (current_excess > 0.0) != (next_excess > 0.0) {
            let crossing = current.lerp(next, current_excess / (current_excess - next_excess));
            inside.push(crossing);
            outside.push(crossing);
        }
    }
    (inside, outside)
}

fn intersect(polygon: &[ScorchVertex], region: &Convex) -> Option<Polygon> {
    let mut remainder = polygon.to_vec();
    for &half in region {
        remainder = split(&remainder, half).0;
        if remainder.len() < 3 {
            return None;
        }
    }
    Some(remainder)
}

// The polygon minus the region, as pieces that do not overlap: each
// half-plane peels off what lies outside it, and what is left inside every
// one is the part removed.
fn subtract(polygon: &[ScorchVertex], region: &Convex) -> Vec<Polygon> {
    let mut remainder = polygon.to_vec();
    let mut pieces = Vec::new();
    for &half in region {
        let (inside, outside) = split(&remainder, half);
        if outside.len() >= 3 {
            pieces.push(outside);
        }
        remainder = inside;
        if remainder.len() < 3 {
            break;
        }
    }
    pieces
}

#[cfg(test)]
#[path = "tests/clip.rs"]
mod tests;
