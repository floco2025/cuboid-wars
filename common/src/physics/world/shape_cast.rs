use bevy_math::Vec3;
use rapier3d::parry::query;

use crate::{
    math::from_rapier,
    protocol::{BarrierKindId, BridgeKindId, CarrierId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Barrier(BarrierKindId),
    Bridge(BridgeKindId),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeCastHit {
    pub normal: Vec3,
    // World-space contact point on the world collider. Rapier's composite
    // cast returns `witness1` already transformed by the collider's world
    // pose (`witness2` stays in the cast shape's local frame).
    pub contact: Vec3,
    pub t: f32,
    pub field_kind: Option<FieldKind>,
    // Whose collider was hit: what a body standing on it rides.
    pub carrier: CarrierId,
}

pub(super) fn upward_surface_hit(hit: query::ShapeCastHit, carrier: CarrierId) -> Option<ShapeCastHit> {
    Some(from_rapier(hit.normal1))
        .filter(|normal| normal.y > 0.1)
        .and_then(Vec3::try_normalize)
        .map(|normal| ShapeCastHit {
            normal,
            contact: from_rapier(hit.witness1),
            t: hit.time_of_impact,
            field_kind: None,
            carrier,
        })
}
