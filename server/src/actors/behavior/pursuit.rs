use bevy::math::Vec3;
use common::{
    constants::CHARACTER_CONTACT_OFFSET,
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld},
    protocol::FieldId,
};

use crate::actors::{SurfaceGoal, resources::AwarePlayer};

pub(super) fn pursuit_surface(
    target: &AwarePlayer,
    world: &CollisionWorld,
    carriers: &Carriers,
    open: &[FieldId],
) -> Option<SurfaceGoal> {
    if target.support != CharacterSupport::Airborne {
        return Some(SurfaceGoal {
            carrier: target.carrier,
            position: target.carrier_pos,
        });
    }
    // Pursue the surface beneath the observed position, at any jump height
    // or airtime. This is an AI destination, not a simulation of the owner.
    // An airborne report no longer names its support, so any carrier's
    // surface counts.
    let origin = Vec3::from(target.pos) + Vec3::Y * CHARACTER_CONTACT_OFFSET;
    let (min, _) = world.geometry_bounds()?;
    world
        .support_surface(origin, origin.y - min.y + CHARACTER_CONTACT_OFFSET, open)
        .map(|surface| SurfaceGoal {
            carrier: surface.carrier,
            position: carriers
                .pose(surface.carrier)
                .inverse_transform_point(surface.point)
                .into(),
        })
}
