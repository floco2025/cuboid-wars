use bevy::prelude::*;

use super::PlayerMap;
use crate::characters::{generate_player_spawn_position, spawn_face_yaw};
use crate::map::MapConfig;
use common::{
    config::GameplayConfig,
    map::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{FaceYaw, Health, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

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
                Health(gameplay_config.player.health().max),
            ))
            .id();

        if let Some(info) = players.get_mut(&id) {
            info.entity = entity;
            info.death_timer = None;
        }

        occupied_positions.push(pos);
        info!("{} respawned at {:?}", players.describe(&id), pos);
    }
}
