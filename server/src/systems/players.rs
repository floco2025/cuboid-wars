use bevy::prelude::*;

use super::characters::generate_player_spawn_position;
use super::network::broadcast_to_all;
use crate::resources::{MapConfig, PlayerMap};
use common::{
    config::GameplayConfig,
    constants::CHARACTER_FALL_TELEPORT_Y,
    markers::PlayerMarker,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{PlayerId, PlayerMoveIntent, PlayerMovementState, Position, SPlayerTeleport, ServerMessage},
};

// ============================================================================
// Players Timer System
// ============================================================================

// System to count down player power-up and stun timers
pub fn players_timer_system(time: Res<Time>, mut players: ResMut<PlayerMap>) {
    let delta = time.delta_secs();

    let mut status_messages = Vec::new();

    for (player_id, player_info) in &mut players.0 {
        let old_status = player_info.status(*player_id);

        player_info.tick_timers(delta);

        let new_status = player_info.status(*player_id);

        if old_status != new_status {
            status_messages.push(new_status);
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}

// ============================================================================
// Players Fall Recovery System
// ============================================================================

// Detect players that have fallen below the death threshold and move them back
// to a spawn position. Broadcasts a teleport so clients apply it immediately
// rather than waiting for the next `SUpdate`.
pub fn players_fall_recovery_system(
    players: Res<PlayerMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    mut player_query: Query<
        (
            Entity,
            &PlayerId,
            &mut Position,
            &mut CharacterVerticalVelocity,
            &PlayerMoveIntent,
        ),
        With<PlayerMarker>,
    >,
) {
    let dead: Vec<(Entity, PlayerId)> = player_query
        .iter()
        .filter_map(|(entity, id, pos, _, _)| (pos.y < CHARACTER_FALL_TELEPORT_Y).then_some((entity, *id)))
        .collect();

    if dead.is_empty() {
        return;
    }

    // Snapshot all current positions so spawn-distance checks see a consistent view.
    let mut occupied_positions: Vec<Position> = player_query.iter().map(|(_, _, pos, _, _)| *pos).collect();

    for (entity, id) in dead {
        let teleport_pos = generate_player_spawn_position(
            &map_config,
            &collision_world,
            &occupied_positions,
            gameplay_config.player.physics(),
        );

        if let Ok((_, _, mut pos, mut motion, move_intent)) = player_query.get_mut(entity) {
            *pos = teleport_pos;
            motion.0 = 0.0;
            broadcast_to_all(
                &players,
                ServerMessage::PlayerTeleport(SPlayerTeleport {
                    id,
                    movement: PlayerMovementState::new(teleport_pos, *move_intent, 0.0),
                }),
            );
        }

        occupied_positions.push(teleport_pos);
        info!("{:?} fell and teleported to {:?}", id, teleport_pos);
    }
}
