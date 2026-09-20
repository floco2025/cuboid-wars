use std::collections::VecDeque;

use bevy::math::{DVec2, Vec3, Vec3Swizzles};

use super::{RouteFailure, SurfaceLocation, SurfaceMesh, TraversalAction};

const EPSILON: f64 = 1e-7;
pub(super) type Portal = [SurfaceLocation; 2];

impl SurfaceMesh {
    // Walk through polygon adjacency, rather than sampling nearest surfaces:
    // sampling can jump a hole or silently change to a different storey.
    pub(super) fn direct_walk(
        &self,
        start: SurfaceLocation,
        goal: SurfaceLocation,
        remaining: &mut usize,
    ) -> Result<bool, RouteFailure> {
        let origin = flat(start);
        let delta = flat(goal) - origin;
        let mut entered = vec![f64::INFINITY; self.polygon_count()];
        entered[start.polygon] = 0.0;
        let mut queue = VecDeque::from([(start.polygon, 0.0)]);
        while let Some((polygon, at)) = queue.pop_front() {
            if at > entered[polygon] {
                continue;
            }
            spend(remaining)?;
            let vertices = &self.polygons[polygon];
            let Some((low, high)) = clip(vertices, origin, delta) else {
                continue;
            };
            if at < low - EPSILON || at > high + EPSILON {
                continue;
            }
            if polygon == goal.polygon && high >= 1.0 - EPSILON {
                return Ok(true);
            }
            for (edge, next) in self.neighbors[polygon].iter().enumerate() {
                let Some(next) = *next else {
                    continue;
                };
                let a = vertices[edge].xz().as_dvec2();
                let b = vertices[(edge + 1) % vertices.len()].xz().as_dvec2();
                let Some((begin, end)) = crossing(origin, delta, a, b) else {
                    continue;
                };
                let crossing = at.max(begin);
                if crossing <= end + EPSILON && crossing <= high + EPSILON && crossing < entered[next] - EPSILON {
                    entered[next] = crossing;
                    queue.push_back((next, crossing));
                }
            }
        }
        Ok(false)
    }

    pub(super) fn portal(&self, polygon: usize, edge: usize) -> Portal {
        let vertices = &self.polygons[polygon];
        let point = |index| SurfaceLocation {
            carrier: self.carrier,
            polygon,
            position: self.project(polygon, vertices[index]).expect("portal height").into(),
        };
        let a = point(edge);
        let b = point((edge + 1) % vertices.len());
        if winding(vertices) > 0.0 { [b, a] } else { [a, b] }
    }

    pub(super) fn corridor_actions(
        &self,
        start: SurfaceLocation,
        goal: SurfaceLocation,
        portals: &[Portal],
        remaining: &mut usize,
    ) -> Vec<TraversalAction> {
        let corners = funnel(start, goal, portals, remaining).ok().filter(|corners| {
            let mut previous = start;
            corners.iter().all(|&corner| {
                let valid = self.direct_walk(previous, corner, remaining) == Ok(true);
                previous = corner;
                valid
            })
        });
        // Folded ramps can overlap in XZ. Keep the proven corridor if a
        // shortcut leaves its surface or smoothing exhausts the budget;
        // optional smoothing must not discard a route we already found.
        let points = corners.unwrap_or_else(|| {
            portals
                .iter()
                .map(|&[left, right]| SurfaceLocation {
                    position: self
                        .project(left.polygon, Vec3::from(left.position).midpoint(right.position.into()))
                        .expect("portal midpoint")
                        .into(),
                    ..left
                })
                .chain(std::iter::once(goal))
                .collect()
        });
        points
            .into_iter()
            .map(|point| TraversalAction::Walk {
                carrier: self.carrier,
                target: point.position,
            })
            .collect()
    }
}

fn spend(remaining: &mut usize) -> Result<(), RouteFailure> {
    *remaining = remaining.checked_sub(1).ok_or(RouteFailure::SearchLimit)?;
    Ok(())
}

fn flat(point: SurfaceLocation) -> DVec2 {
    Vec3::from(point.position).xz().as_dvec2()
}

fn winding(vertices: &[Vec3]) -> f64 {
    let origin = vertices[0].xz().as_dvec2();
    vertices[1..]
        .windows(2)
        .map(|pair| (pair[0].xz().as_dvec2() - origin).perp_dot(pair[1].xz().as_dvec2() - origin))
        .sum::<f64>()
        .signum()
}

fn clip(vertices: &[Vec3], origin: DVec2, delta: DVec2) -> Option<(f64, f64)> {
    let orientation = winding(vertices);
    let mut low: f64 = 0.0;
    let mut high: f64 = 1.0;
    for (&a, &b) in vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
    {
        let edge = b.xz().as_dvec2() - a.xz().as_dvec2();
        let distance = orientation * edge.perp_dot(origin - a.xz().as_dvec2());
        let slope = orientation * edge.perp_dot(delta);
        if slope.abs() < EPSILON {
            if distance < -EPSILON {
                return None;
            }
        } else if slope > 0.0 {
            low = low.max(-distance / slope);
        } else {
            high = high.min(-distance / slope);
        }
    }
    (low <= high + EPSILON).then_some((low, high))
}

fn crossing(origin: DVec2, delta: DVec2, a: DVec2, b: DVec2) -> Option<(f64, f64)> {
    let edge = b - a;
    let denominator = delta.perp_dot(edge);
    let offset = a - origin;
    if denominator.abs() > EPSILON {
        let t = offset.perp_dot(edge) / denominator;
        let u = offset.perp_dot(delta) / denominator;
        return ((-EPSILON..=1.0 + EPSILON).contains(&t) && (-EPSILON..=1.0 + EPSILON).contains(&u))
            .then_some((t.clamp(0.0, 1.0), t.clamp(0.0, 1.0)));
    }
    if delta.length_squared() < EPSILON {
        let t = ((origin - a).dot(edge) / edge.length_squared().max(EPSILON)).clamp(0.0, 1.0);
        return (origin.distance_squared(a + edge * t) < EPSILON * EPSILON).then_some((0.0, 1.0));
    }
    if offset.perp_dot(delta).abs() > EPSILON {
        return None;
    }
    let a = offset.dot(delta) / delta.length_squared();
    let b = (b - origin).dot(delta) / delta.length_squared();
    let low = a.min(b).max(0.0);
    let high = a.max(b).min(1.0);
    (low <= high + EPSILON).then_some((low, high))
}

fn funnel(
    start: SurfaceLocation,
    goal: SurfaceLocation,
    portals: &[Portal],
    remaining: &mut usize,
) -> Result<Vec<SurfaceLocation>, RouteFailure> {
    let mut result = Vec::new();
    let mut apex = start;
    let mut left = start;
    let mut right = start;
    let mut left_index = 0;
    let mut right_index = 0;
    let mut index = 0;
    let area = |a, b, c| (flat(b) - flat(a)).perp_dot(flat(c) - flat(a));
    let same = |a, b| flat(a).distance_squared(flat(b)) < EPSILON * EPSILON;
    while index <= portals.len() {
        spend(remaining)?;
        let [next_left, next_right] = portals.get(index).copied().unwrap_or([goal, goal]);
        if area(apex, right, next_right) >= -EPSILON {
            if same(apex, right) || area(apex, left, next_right) <= EPSILON {
                right = next_right;
                right_index = index;
            } else {
                result.push(left);
                apex = left;
                right = apex;
                left = apex;
                right_index = left_index;
                index = left_index + 1;
                continue;
            }
        }
        if area(apex, left, next_left) <= EPSILON {
            if same(apex, left) || area(apex, right, next_left) >= -EPSILON {
                left = next_left;
                left_index = index;
            } else {
                result.push(right);
                apex = right;
                left = apex;
                right = apex;
                left_index = right_index;
                index = right_index + 1;
                continue;
            }
        }
        index += 1;
    }
    if result.last() != Some(&goal) {
        result.push(goal);
    }
    Ok(result)
}
