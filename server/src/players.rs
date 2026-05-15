use bevy::prelude::*;

use super::characters::generate_player_spawn_position;
use super::combat::kill_player;
use super::network::broadcast_to_all;
use crate::resources::{MapConfig, PlayerMap};
use common::{
    config::GameplayConfig,
    constants::CHARACTER_FALL_DEATH_Y,
    map_geometry::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{FaceDirection, Health, PlayerId, PlayerMarker, PlayerMoveIntent, Position, ServerMessage},
};

// ============================================================================
// Players Status Timers System
// ============================================================================

// System to count down player power-up and stun timers
pub fn players_status_timers_system(time: Res<Time>, mut players: ResMut<PlayerMap>) {
    let delta = time.delta_secs();

    let mut status_messages = Vec::new();

    for (player_id, player_info) in players.iter_mut() {
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
// Players Fall Death System
// ============================================================================

// Detect players that have fallen below the death threshold and kill them
// using the same flow as any other death (clear per-life state, arm respawn
// timer, despawn entity). The respawn system brings them back at a fresh
// spawn-zone cell after `respawn_delay_secs`.
pub fn players_fall_death_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<crate::config::ServerGameplayConfig>,
    player_query: Query<(Entity, &PlayerId, &Position), With<PlayerMarker>>,
) {
    // Debug invincibility shorts the whole system — a player can keep
    // falling indefinitely. That's the intended trade-off; the only
    // alternative would be a teleport, which is beyond "no damage".
    if server_gameplay_config.player.invincible {
        return;
    }
    for (entity, id, pos) in player_query.iter() {
        if pos.y >= CHARACTER_FALL_DEATH_Y {
            continue;
        }
        // Skip players already dead this tick (e.g. killed by a projectile
        // before falling out of the world).
        if players.get(id).is_some_and(|info| info.is_dead()) {
            continue;
        }
        info!("{:?} fell and died at {:?}", id, pos);
        kill_player(
            &mut commands,
            &mut players,
            *id,
            entity,
            *pos,
            gameplay_config.player.respawn_delay_secs,
            None,
        );
    }
}

// ============================================================================
// Players Respawn System
// ============================================================================

// Tick each dead player's respawn timer. When it elapses, spawn a fresh entity
// at a new spawn-zone cell with full health. Per-life state (power-ups, keys,
// stun) was already cleared at death; score is preserved.
//
// The new entity replaces the (already despawned) old one; the next `SSnapshot`
// will carry the player at their new position and the client's snapshot diff
// resurrects their visual.
pub fn players_respawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut players: ResMut<PlayerMap>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    player_query: Query<&Position, With<PlayerMarker>>,
) {
    let delta = time.delta_secs();

    let mut occupied_positions: Vec<Position> = player_query.iter().copied().collect();
    let mut to_respawn: Vec<PlayerId> = Vec::new();

    for (id, info) in players.iter_mut() {
        let Some(timer) = info.death_timer.as_mut() else {
            continue;
        };
        *timer -= delta;
        if *timer <= 0.0 {
            to_respawn.push(*id);
        }
    }

    for id in to_respawn {
        let pos = generate_player_spawn_position(
            &map_config,
            &map_geometry,
            &collision_world,
            &occupied_positions,
            gameplay_config.player.physics(),
        );
        let face_dir = (-pos.x).atan2(-pos.z);
        let move_intent = PlayerMoveIntent::Idle;
        let entity = commands
            .spawn((
                PlayerMarker,
                id,
                pos,
                move_intent,
                FaceDirection(face_dir),
                CharacterVerticalVelocity::default(),
                Health(gameplay_config.player.health().max),
            ))
            .id();

        if let Some(info) = players.get_mut(&id) {
            info.entity = entity;
            info.death_timer = None;
        }

        occupied_positions.push(pos);
        info!("{:?} respawned at {:?}", id, pos);
    }
}
