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

    pub fn corners(&self) -> [Vec2; 4] {
        let (across, along) = (self.across * self.half_across, self.along * self.half_along);
        [
            self.center - across - along,
            self.center + across - along,
            self.center + across + along,
            self.center - across + along,
        ]
    }

    pub fn sides(&self) -> [HalfPlane; 4] {
        [
            (self.across, self.half_across),
            (-self.across, self.half_across),
            (self.along, self.half_along),
            (-self.along, self.half_along),
        ]
        .map(|(normal, half)| HalfPlane {
            normal,
            offset: normal.dot(self.center) + half,
        })
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
