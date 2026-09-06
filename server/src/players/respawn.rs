use bevy::prelude::*;

use super::{PlayerMap, UnlimitedMissiles};
use crate::characters::{generate_player_spawn_position, spawn_face_yaw};
use crate::config::ServerGameplayConfig;
use crate::map::MapConfig;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{AirborneMomentum, CharacterVerticalVelocity, CollisionWorld},
    protocol::{FaceYaw, Health, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

// Tick each dead player's respawn timer. When it elapses, spawn a fresh entity
// at a new spawn-zone cell with full health. Per-life state (power-ups, keys,
// stun) was already cleared at death; score is preserved.
//
// The new entity moves the player's lifecycle back to alive; the next
// `SSnapshot` carries the new position and resurrects the client visual.
pub fn players_respawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut players: ResMut<PlayerMap>,
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    unlimited_missiles: Res<UnlimitedMissiles>,
    player_query: Query<&Position, With<PlayerMarker>>,
) {
    let delta = time.delta_secs();
    let mut to_respawn: Vec<PlayerId> = Vec::new();

    for (id, info) in players.iter_mut() {
        let Some(timer) = info.respawn_remaining_secs_mut() else {
            continue;
        };
        *timer -= delta;
        if *timer <= 0.0 {
            to_respawn.push(*id);
        }
    }

    if to_respawn.is_empty() {
        return;
    }

    let mut occupied_positions: Vec<Position> = player_query.iter().copied().collect();
    for id in to_respawn {
        let pos = generate_player_spawn_position(
            &map_config,
            &carriers,
            &collision_world,
            &occupied_positions,
            gameplay_config.player.physics(),
        );
        let face_yaw = spawn_face_yaw(&pos);
        let move_intent = PlayerMoveIntent::Idle;
        let entity = commands
            .spawn((
                PlayerMarker,
                id,
                pos,
                move_intent,
                FaceYaw(face_yaw),
                CharacterVerticalVelocity::default(),
                AirborneMomentum::default(),
                Health(server_gameplay_config.combat.health.player.max),
            ))
            .id();

        if let Some(info) = players.get_mut(&id) {
            info.finish_respawn(entity);
            if unlimited_missiles.0 {
                info.life.missiles = gameplay_config.missiles.max_missiles;
            }
        }

        occupied_positions.push(pos);
        info!("{} respawned at {:?}", players.describe(&id), pos);
    }
}
