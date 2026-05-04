use bevy_math::Vec3;
use rapier3d::parry::query::ShapeCastHit as RapierShapeCastHit;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShapeCastHit {
    pub normal: Vec3,
    pub t: f32,
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
            t: hit.time_of_impact,
        })
}
