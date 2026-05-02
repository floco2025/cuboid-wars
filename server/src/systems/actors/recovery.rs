use bevy::prelude::*;

use super::network::broadcast_actor_teleport;
use crate::{
    resources::{MapConfig, PlayerMap},
    systems::characters::generate_character_spawn_position,
};
use common::{
    config::GameplayConfig,
    constants::CHARACTER_FALL_TELEPORT_Y,
    markers::ActorMarker,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorId, ActorMoveIntent, Position},
};

pub fn actor_fall_recovery_system(
    players: Res<PlayerMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    mut query: Query<
        (
            Entity,
            &ActorId,
            &mut Position,
            &mut CharacterVerticalVelocity,
            &ActorMoveIntent,
        ),
        With<ActorMarker>,
    >,
) {
    let fallen: Vec<(Entity, ActorId)> = query
        .iter()
        .filter_map(|(entity, id, pos, _, _)| (pos.y < CHARACTER_FALL_TELEPORT_Y).then_some((entity, *id)))
        .collect();

    if fallen.is_empty() {
        return;
    }

    let mut occupied_positions: Vec<Position> = query.iter().map(|(_, _, pos, _, _)| *pos).collect();

    for (entity, id) in fallen {
        let teleport_pos = generate_character_spawn_position(
            &map_config,
            &collision_world,
            &occupied_positions,
            gameplay_config.characters.actor.physics(),
        );

        if let Ok((_, _, mut pos, mut motion, move_intent)) = query.get_mut(entity) {
            *pos = teleport_pos;
            motion.0 = 0.0;
            broadcast_actor_teleport(&players, id, teleport_pos, *move_intent);
        }

        occupied_positions.push(teleport_pos);
        info!("{:?} fell and teleported to {:?}", id, teleport_pos);
    }
}
