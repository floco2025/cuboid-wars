use bevy::math::Vec3;
use common::{
    config::ActorGameplayConfig, constants::CHARACTER_CONTACT_OFFSET, map::Carriers, physics::CollisionWorld,
    protocol::FieldId,
};

use crate::actors::{
    SurfaceGoal,
    navigation::{ActorTerritory, radical_inverse, surface::SurfaceNavigation},
};

pub(super) fn return_goal(
    home: &ActorTerritory,
    from: SurfaceGoal,
    character: &ActorGameplayConfig,
    navigation: &mut SurfaceNavigation,
    world: &CollisionWorld,
    carriers: &Carriers,
    open: &[FieldId],
    sample_index: &mut usize,
) -> Option<SurfaceGoal> {
    let pose = carriers.pose(home.carrier);
    let physics = character.physics();
    navigation.prepare_area(from, physics);
    for attempt in 0..9 {
        let fraction = if attempt == 0 {
            Vec3::new(0.5, 1.0, 0.5)
        } else {
            *sample_index = sample_index.wrapping_add(1);
            Vec3::new(
                radical_inverse(*sample_index, 2),
                radical_inverse(*sample_index, 5),
                radical_inverse(*sample_index, 3),
            )
        };
        // A zone describes a volume, whose center may be between storeys.
        // Probe real support inside the spawn volume, not the roaming extension.
        let sample = home.volume.min + (home.volume.max - home.volume.min) * fraction
            - Vec3::Y * (home.center_height + CHARACTER_CONTACT_OFFSET);
        let distance = sample.y - (home.volume.min.y - home.center_height);
        let Some(surface) =
            world.support_surface_on_carrier(pose.transform_point(sample), distance, home.carrier, open)
        else {
            continue;
        };
        let point = pose.inverse_transform_point(surface.point);
        let Some(candidate) = navigation
            .mesh(home.carrier, physics)
            .and_then(|(mesh, _)| mesh.locate(point.into(), 1.0))
        else {
            continue;
        };
        if !home.contains_spawn_position(candidate.position.into()) {
            continue;
        }
        let destination = SurfaceGoal {
            carrier: home.carrier,
            position: candidate.position,
        };
        let goal = navigation.approach_goal(
            from,
            destination,
            physics,
            character.can_use_ladders,
            world,
            carriers,
            open,
        );
        if navigation.prepare_route(from, goal, physics, character.can_use_ladders) != Some(false) {
            return Some(goal);
        }
    }
    None
}
