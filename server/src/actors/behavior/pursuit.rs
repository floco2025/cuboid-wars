use bevy::math::Vec3;
use common::{
    constants::CHARACTER_CONTACT_OFFSET,
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld},
    protocol::{CarrierId, FieldId},
};

use crate::actors::{SurfaceGoal, resources::AwarePlayer};

pub(super) fn pursuit_surface(
    target: &AwarePlayer,
    reported_carrier: CarrierId,
    world: &CollisionWorld,
    carriers: &Carriers,
    open: &[FieldId],
) -> Option<SurfaceGoal> {
    if target.support != CharacterSupport::Airborne {
        return Some(SurfaceGoal {
            carrier: reported_carrier,
            position: carriers.pose(reported_carrier).inverse_transform_position(&target.pos),
        });
    }
    // Pursue the surface beneath the observed position, at any jump height
    // or airtime. This is an AI destination, not a simulation of the owner.
    // Check every carrier: an airborne report no longer names its support.
    let origin = Vec3::from(target.pos) + Vec3::Y * CHARACTER_CONTACT_OFFSET;
    let (min, _) = world.geometry_bounds()?;
    std::iter::once(CarrierId::WORLD)
        .chain(carriers.carried_ids())
        .filter_map(|carrier| {
            world.support_surface_on_carrier(origin, origin.y - min.y + CHARACTER_CONTACT_OFFSET, carrier, open)
        })
        .max_by(|a, b| a.point.y.total_cmp(&b.point.y))
        .map(|surface| SurfaceGoal {
            carrier: surface.carrier,
            position: carriers
                .pose(surface.carrier)
                .inverse_transform_point(surface.point)
                .into(),
        })
}
