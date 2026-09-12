use super::beam::BeamContext;
use crate::{characters::character_surface_distance, config::ActorAttackConfig};
use bevy::prelude::Vec3;
use common::{physics::character_hitbox_center, protocol::Position};

pub(super) fn threat_distance_sq(point: Vec3, threats: &[Position]) -> f32 {
    threats
        .iter()
        .map(|p| point.distance_squared(Vec3::from(*p)))
        .fold(f32::INFINITY, f32::min)
}

pub(super) fn covered(point: Vec3, threats: &[Position], context: &BeamContext<'_>) -> bool {
    let radius = context.kind_config.character.physics().movement_collider.radius();
    threats.iter().all(|target| {
        let target = character_hitbox_center(*target, context.player_physics);
        [
            Vec3::ZERO,
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ]
        .into_iter()
        .all(|offset| {
            let eye = point + Vec3::Y * context.kind_config.character.eye_height() + offset * radius;
            eye.distance_squared(target) > radius * radius * 4.0
                && !context.collision_world.line_of_sight_clear(eye, target)
        })
    })
}

pub(super) fn attack_position(pos: Position, target: Position, context: &BeamContext<'_>) -> bool {
    let character = &context.kind_config.character;
    let physics = character.physics();
    let range = match context.kind_config.attack {
        ActorAttackConfig::Beam(beam) => pos.distance_sq(&target) <= beam.range * beam.range,
        _ => {
            character_surface_distance(pos, physics, target, context.player_physics)
                <= context
                    .kind_config
                    .attack
                    .contact_trigger_gap()
                    .expect("contact attack gap missing")
        }
    };
    range
        && context.collision_world.attack_path_clear(
            Vec3::from(pos) + Vec3::Y * character.beam_origin_y_offset(),
            character_hitbox_center(target, context.player_physics),
            context.open_barriers,
        )
}
