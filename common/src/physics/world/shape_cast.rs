use bevy_math::Vec3;
use rapier3d::parry::query::ShapeCastHit as RapierShapeCastHit;

use crate::protocol::BarrierKindId;

#[derive(Debug, Clone, Copy)]
pub struct ShapeCastHit {
    pub normal: Vec3,
    // World-space contact point on the world collider. Rapier's composite
    // cast returns `witness1` already transformed by the collider's world
    // pose (`witness2` stays in the cast shape's local frame).
    pub contact: Vec3,
    pub t: f32,
    pub barrier_kind: Option<BarrierKindId>,
}

pub(super) fn upward_surface_hit(hit: RapierShapeCastHit) -> Option<ShapeCastHit> {
    [hit.normal1, hit.normal2, -hit.normal1, -hit.normal2]
        .into_iter()
        .map(|normal| Vec3::new(normal.x, normal.y, normal.z))
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .filter(|normal| normal.y > 0.1)
        .and_then(Vec3::try_normalize)
        .map(|normal| ShapeCastHit {
            normal,
            contact: Vec3::new(hit.witness1.x, hit.witness1.y, hit.witness1.z),
            t: hit.time_of_impact,
            barrier_kind: None,
        })
}
