use bevy_math::Vec2;

// Points closer than this are one point, and turns flatter than it are straight.
const MERGE_DISTANCE: f32 = 1e-4;

// The points `p` with `normal · p <= offset`.
#[derive(Debug, Clone, Copy)]
pub(super) struct HalfPlane {
    pub normal: Vec2,
    pub offset: f32,
}

impl HalfPlane {
    fn excess(self, point: Vec2) -> f32 {
        self.normal.dot(point) - self.offset
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Rectangle {
    pub center: Vec2,
    pub across: Vec2,
    pub half_across: f32,
    pub along: Vec2,
    pub half_along: f32,
}

impl Rectangle {
    pub fn at(self, center: Vec2) -> Self {
        Self { center, ..self }
    }

    pub fn grown(self, by: f32) -> Self {
        Self {
            half_across: self.half_across + by,
            half_along: self.half_along + by,
            ..self
        }
    }
}

// Andrew's monotone chain, counter-clockwise without collinear points. One
// distinct point or a straight run comes back as one or two points.
pub(super) fn convex_hull(points: &[Vec2]) -> Vec<Vec2> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    sorted.dedup_by(|a, b| a.distance(*b) < MERGE_DISTANCE);
    if sorted.len() < 3 {
        return sorted;
    }
    let chain = |points: &mut dyn Iterator<Item = Vec2>| {
        let mut chain: Vec<Vec2> = Vec::new();
        for point in points {
            while let [.., before, last] = chain[..]
                && (last - before).perp_dot(point - last) <= MERGE_DISTANCE * MERGE_DISTANCE
            {
                chain.pop();
            }
            chain.push(point);
        }
        // Its last point starts the other chain.
        chain.pop();
        chain
    };
    let mut hull = chain(&mut sorted.iter().copied());
    hull.extend(chain(&mut sorted.iter().rev().copied()));
    hull
}

// Sutherland-Hodgman against each half-plane in turn; the polygon must be convex.
pub(super) fn clip(polygon: &[Vec2], planes: &[HalfPlane]) -> Vec<Vec2> {
    let mut polygon = polygon.to_vec();
    for plane in planes {
        let mut clipped = Vec::with_capacity(polygon.len() + 1);
        for (index, &from) in polygon.iter().enumerate() {
            let to = polygon[(index + 1) % polygon.len()];
            let (before, after) = (plane.excess(from), plane.excess(to));
            if before <= 0.0 {
                clipped.push(from);
            }
            if (before < 0.0 && after > 0.0) || (before > 0.0 && after < 0.0) {
                clipped.push(from.lerp(to, before / (before - after)));
            }
        }
        polygon = clipped;
    }
    polygon
}

// An axis-aligned box in some frame's coordinates, `low` to `high`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Box2 {
    pub low: Vec2,
    pub high: Vec2,
}

impl Rectangle {
    // The frame coordinates of a point: along `across` and `along`.
    pub fn local(&self, point: Vec2) -> Vec2 {
        Vec2::new(point.dot(self.across), point.dot(self.along))
    }

    pub fn world(&self, local: Vec2) -> Vec2 {
        self.across * local.x + self.along * local.y
    }

    pub fn bounds(&self) -> Box2 {
        let (center, half) = (self.local(self.center), Vec2::new(self.half_across, self.half_along));
        Box2 {
            low: center - half,
            high: center + half,
        }
    }
}

// The outlines of what boxes cover together, counter-clockwise, one per
// connected piece, holes included. Each box edge contributes the stretches
// with nothing covering the far side of it; those stretches chain into the
// outlines.
pub(super) fn boxes_union(boxes: &[Box2]) -> Vec<Vec<Vec2>> {
    // Alike boxes would each keep the other's edges.
    let mut boxes = boxes.to_vec();
    boxes.sort_by(|a, b| {
        [a.low.x, a.low.y, a.high.x, a.high.y]
            .iter()
            .zip([b.low.x, b.low.y, b.high.x, b.high.y])
            .map(|(a, b)| a.total_cmp(&b))
            .find(|order| order.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    boxes.dedup_by(|a, b| a.low.distance(b.low) <= MERGE_DISTANCE && a.high.distance(b.high) <= MERGE_DISTANCE);
    let boxes = &boxes;
    let mut segments: Vec<(Vec2, Vec2)> = Vec::new();
    for (index, a) in boxes.iter().enumerate() {
        let others = boxes.iter().enumerate().filter(|(other, _)| *other != index);
        // Each edge: the line it lies on, its span, the boxes covering the far side, and its direction.
        let edges = [
            (a.high.x, (a.low.y, a.high.y), true, 1.0),
            (a.low.x, (a.low.y, a.high.y), true, -1.0),
            (a.high.y, (a.low.x, a.high.x), false, -1.0),
            (a.low.y, (a.low.x, a.high.x), false, 1.0),
        ];
        for (line, (start, stop), vertical, direction) in edges {
            let outward = if vertical { direction } else { -direction };
            let mut covered: Vec<(f32, f32)> = others
                .clone()
                .filter_map(|(_, b)| {
                    let (b_low, b_high, span_low, span_high) = if vertical {
                        (b.low.x, b.high.x, b.low.y, b.high.y)
                    } else {
                        (b.low.y, b.high.y, b.low.x, b.high.x)
                    };
                    // Covers the far side when the line, nudged outward, is inside it.
                    let inside = if outward > 0.0 {
                        b_low <= line + MERGE_DISTANCE && line < b_high - MERGE_DISTANCE
                    } else {
                        b_low < line - MERGE_DISTANCE && line <= b_high + MERGE_DISTANCE
                    };
                    inside.then_some((span_low, span_high))
                })
                .collect();
            covered.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut at = start;
            let mut free = Vec::new();
            for (low, high) in covered {
                if low > at + MERGE_DISTANCE {
                    free.push((at, low.min(stop)));
                }
                at = at.max(high);
                if at >= stop {
                    break;
                }
            }
            if at < stop - MERGE_DISTANCE {
                free.push((at, stop));
            }
            for (from, to) in free {
                if to - from <= MERGE_DISTANCE {
                    continue;
                }
                let (from, to) = if direction > 0.0 { (from, to) } else { (to, from) };
                let point = |at: f32| {
                    if vertical {
                        Vec2::new(line, at)
                    } else {
                        Vec2::new(at, line)
                    }
                };
                segments.push((point(from), point(to)));
            }
        }
    }
    // Chain the segments end to start into closed outlines.
    let mut outlines = Vec::new();
    let mut unused: Vec<bool> = vec![true; segments.len()];
    for first in 0..segments.len() {
        if !unused[first] {
            continue;
        }
        unused[first] = false;
        let mut outline = vec![segments[first].0];
        let mut end = segments[first].1;
        while end.distance(outline[0]) > MERGE_DISTANCE {
            let Some(next) =
                (0..segments.len()).find(|&index| unused[index] && segments[index].0.distance(end) <= MERGE_DISTANCE)
            else {
                break;
            };
            unused[next] = false;
            outline.push(end);
            end = segments[next].1;
        }
        // Straight runs need no points in between.
        let mut trimmed: Vec<Vec2> = Vec::with_capacity(outline.len());
        for (index, &point) in outline.iter().enumerate() {
            let before = outline[(index + outline.len() - 1) % outline.len()];
            let after = outline[(index + 1) % outline.len()];
            if (point - before).perp_dot(after - point).abs() > MERGE_DISTANCE * MERGE_DISTANCE {
                trimmed.push(point);
            }
        }
        if trimmed.len() >= 3 {
            outlines.push(trimmed);
        }
    }
    outlines
}

// Drops the points of an outline whose whole stretch of the original lies
// within `tolerance` of the line between the kept neighbours, least
// deviating first, so a fine staircase draws as a chamfer that never
// strays further than that from the true outline.
pub(super) fn simplify(polygon: Vec<Vec2>, tolerance: f32) -> Vec<Vec2> {
    if tolerance <= 0.0 || polygon.len() <= 4 {
        return polygon;
    }
    let mut kept: Vec<usize> = (0..polygon.len()).collect();
    loop {
        if kept.len() <= 4 {
            break;
        }
        // Dropping kept point `at` leaves the original points from the
        // kept neighbour before it to the one after it on one line.
        let deviation = |at: usize| {
            let (before, after) = (kept[(at + kept.len() - 1) % kept.len()], kept[(at + 1) % kept.len()]);
            let (start, stop) = (polygon[before], polygon[after]);
            let along = stop - start;
            let length = along.length();
            let mut index = before;
            let mut worst: f32 = 0.0;
            while index != after {
                index = (index + 1) % polygon.len();
                let off = polygon[index] - start;
                let distance = if length <= MERGE_DISTANCE {
                    off.length()
                } else {
                    off.perp_dot(along).abs() / length
                };
                worst = worst.max(distance);
            }
            worst
        };
        let (at, least) = (0..kept.len())
            .map(|at| (at, deviation(at)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("kept points remain");
        if least > tolerance {
            break;
        }
        kept.remove(at);
    }
    kept.into_iter().map(|index| polygon[index]).collect()
}

// The quarter of the plane, seen from `apex`, whose directions snap to `yaw`
// under the game's quarter-turn placement rule. Points are `(x, z)`.
pub(super) fn yaw_wedge(apex: Vec2, yaw: f32) -> [HalfPlane; 2] {
    let (sin, cos) = yaw.sin_cos();
    let forward = Vec2::new(sin, cos);
    let sideways = Vec2::new(cos, -sin);
    [sideways - forward, -sideways - forward].map(|normal| HalfPlane {
        normal,
        offset: normal.dot(apex),
    })
}

pub(super) fn area(polygon: &[Vec2]) -> f32 {
    let twice: f32 = polygon
        .iter()
        .enumerate()
        .map(|(index, point)| point.perp_dot(polygon[(index + 1) % polygon.len()]))
        .sum();
    twice.abs() * 0.5
}

#[cfg(test)]
#[path = "tests/polygons.rs"]
mod tests;
