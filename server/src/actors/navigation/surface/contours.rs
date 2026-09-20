use anyhow::{Result, ensure};
use bevy::math::U16Vec3;
use rerecast::ContourSet;

type Vertex = (U16Vec3, u32);

// rerecast 0.4 emits interior rings separately, but its polygon triangulator
// expects each region's holes to have been joined to the outer contour.
pub(super) fn join_holes(contours: &mut ContourSet) -> Result<()> {
    let holes: Vec<_> = contours
        .contours
        .iter()
        .enumerate()
        .filter(|(_, contour)| signed_area(&contour.vertices) > 0)
        .map(|(index, _)| index)
        .collect();
    for hole_index in holes {
        let hole = &contours.contours[hole_index];
        let region = hole.region;
        let mut best = None;
        for (outer_index, outer) in contours.contours.iter().enumerate() {
            if outer.region != region || signed_area(&outer.vertices) >= 0 {
                continue;
            }
            for (a, &(from, _)) in outer.vertices.iter().enumerate() {
                for (b, &(to, _)) in hole.vertices.iter().enumerate() {
                    let midpoint = (
                        (f64::from(from.x) + f64::from(to.x)) / 2.0,
                        (f64::from(from.z) + f64::from(to.z)) / 2.0,
                    );
                    if !inside(&outer.vertices, midpoint) || inside(&hole.vertices, midpoint) {
                        continue;
                    }
                    if contours
                        .contours
                        .iter()
                        .filter(|c| c.region == region)
                        .any(|c| edges(&c.vertices).any(|(v, w)| intersects_bridge(from, to, v, w)))
                    {
                        continue;
                    }
                    let dx = i64::from(from.x) - i64::from(to.x);
                    let dz = i64::from(from.z) - i64::from(to.z);
                    let distance = dx * dx + dz * dz;
                    if best.is_none_or(|(cost, _, _, _)| distance < cost) {
                        best = Some((distance, outer_index, a, b));
                    }
                }
            }
        }
        ensure!(
            best.is_some(),
            "navigation contour hole has no visible boundary connection"
        );
        let (_, outer_index, a, b) = best.expect("connection missing from validated contour hole");
        let hole = std::mem::take(&mut contours.contours[hole_index].vertices);
        let outer = &mut contours.contours[outer_index].vertices;
        let mut merged = Vec::with_capacity(outer.len() + hole.len() + 2);
        merged.extend_from_slice(&outer[..=a]);
        merged.extend(hole[b..].iter().chain(&hole[..=b]).copied());
        merged.extend_from_slice(&outer[a..]);
        *outer = merged;
    }
    contours.contours.retain(|contour| !contour.vertices.is_empty());
    Ok(())
}

fn edges(vertices: &[Vertex]) -> impl Iterator<Item = (U16Vec3, U16Vec3)> + '_ {
    vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
        .map(|(a, b)| (a.0, b.0))
}

fn signed_area(vertices: &[Vertex]) -> i64 {
    edges(vertices)
        .map(|(a, b)| i64::from(a.x) * i64::from(b.z) - i64::from(b.x) * i64::from(a.z))
        .sum()
}

fn inside(vertices: &[Vertex], (x, z): (f64, f64)) -> bool {
    let mut inside = false;
    for (a, b) in edges(vertices) {
        let (ax, az, bx, bz) = (f64::from(a.x), f64::from(a.z), f64::from(b.x), f64::from(b.z));
        if (az > z) != (bz > z) && x < (bx - ax) * (z - az) / (bz - az) + ax {
            inside = !inside;
        }
    }
    inside
}

fn cross(a: U16Vec3, b: U16Vec3, c: U16Vec3) -> i64 {
    (i64::from(b.x) - i64::from(a.x)) * (i64::from(c.z) - i64::from(a.z))
        - (i64::from(b.z) - i64::from(a.z)) * (i64::from(c.x) - i64::from(a.x))
}

fn intersects_bridge(a: U16Vec3, b: U16Vec3, c: U16Vec3, d: U16Vec3) -> bool {
    let same = |a: U16Vec3, b: U16Vec3| a.x == b.x && a.z == b.z;
    if same(a, c) || same(a, d) || same(b, c) || same(b, d) {
        return false;
    }
    if a.x.max(b.x) < c.x.min(d.x)
        || c.x.max(d.x) < a.x.min(b.x)
        || a.z.max(b.z) < c.z.min(d.z)
        || c.z.max(d.z) < a.z.min(b.z)
    {
        return false;
    }
    let (p, q, r, s) = (cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b));
    p.signum() * q.signum() <= 0 && r.signum() * s.signum() <= 0
}

#[cfg(test)]
#[path = "tests/contours.rs"]
mod tests;
