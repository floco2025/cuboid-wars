use bevy::prelude::*;

use crate::{
    map::OpenBarrierKinds,
    network::{PlayerStateQuery, broadcast_to_others},
    players::PlayerMap,
};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, ProjectileMotion, calculate_projectile_spawns},
    protocol::*,
};

// Handle shot message.
pub fn handle_shot_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CShot,
    players: &mut PlayerMap,
    time: &Res<Time>,
    player_data: &PlayerStateQuery,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    open_barrier_kinds: &OpenBarrierKinds,
) {
    // Reject non-finite aim before it reaches projectile trig / authoritative
    // hit detection. Checked ahead of `try_start_shot` so a bad shot doesn't
    // burn the fire cooldown.
    if !(msg.face_dir.is_finite() && msg.face_pitch.is_finite()) {
        return;
    }

    let now = time.elapsed_secs();

    let Some(has_multi_shot) = players
        .get_mut(&id)
        .and_then(|info| info.try_start_shot(now, gameplay_config.projectiles.cooldown_secs))
    else {
        return;
    };

    commands.entity(entity).insert(FaceDirection(msg.face_dir));

    // Spawn projectile(s) on server for hit detection
    if let Ok((pos, _, _, _)) = player_data.get(entity) {
        let spawns = calculate_projectile_spawns(
            pos,
            msg.face_dir,
            msg.face_pitch,
            has_multi_shot,
            gameplay_config.player.eye_height(),
            &gameplay_config.projectiles,
            collision_world,
            &open_barrier_kinds.0,
        );

        // Spawn each projectile
        for spawn_info in spawns {
            let proj_motion = ProjectileMotion::new(
                spawn_info.direction_yaw,
                spawn_info.direction_pitch,
                &gameplay_config.projectiles,
            );

            commands.spawn((
                ProjectileMarker,
                id, // Tag projectile with shooter's ID
                spawn_info.position,
                proj_motion,
            ));
        }
    }

    // Broadcast shot with face direction to all other logged-in players
    broadcast_to_others(
        players,
        id,
        ServerMessage::Shot(SShot {
            id,
            face_dir: msg.face_dir,
            face_pitch: msg.face_pitch,
        }),
    );
}
