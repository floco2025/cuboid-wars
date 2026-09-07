use bevy::prelude::*;

use crate::{
    network::broadcast_to_others,
    players::{PlayerMap, PlayerStateQuery},
};
use common::{
    config::{GameplayConfig, MultiShotConfig},
    physics::{CollisionWorld, ProjectileMotion, calculate_projectile_spawns},
    protocol::*,
};

pub fn handle_projectile_shot_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: &CProjectileShot,
    players: &mut PlayerMap,
    time: &Res<Time>,
    player_data: &PlayerStateQuery,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    map_settings: &MapSettings,
    plates: &PlateState,
) {
    // Reject non-finite aim before it reaches projectile trig / authoritative
    // hit detection. Checked ahead of `try_start_shot` so a bad shot doesn't
    // burn the fire cooldown.
    if !(msg.face_yaw.is_finite() && msg.face_pitch.is_finite()) {
        return;
    }

    let now = time.elapsed_secs();

    if !players
        .get_mut(&id)
        .is_some_and(|info| info.try_start_shot(now, gameplay_config.projectiles.cooldown_secs, msg.pattern.is_some()))
    {
        return;
    }
    let actual_pattern = resolved_pattern(msg.pattern.as_deref(), &gameplay_config.projectiles.multi_shot);

    commands.entity(entity).insert(FaceYaw(msg.face_yaw));

    // Spawn projectile(s) on server for hit detection
    if let Ok((pos, _, _, _)) = player_data.get(entity) {
        let spawns = calculate_projectile_spawns(
            pos,
            msg.face_yaw,
            msg.face_pitch,
            actual_pattern,
            gameplay_config.player.eye_height(),
            gameplay_config,
            collision_world,
            &plates.open_barrier_kinds,
        );

        // Spawn each projectile
        for spawn_info in spawns {
            let proj_motion = ProjectileMotion::new(
                spawn_info.direction_yaw,
                spawn_info.direction_pitch,
                map_settings.movement.projectile_speed,
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

    // Broadcast shot with face direction to all other active players.
    broadcast_to_others(
        players,
        id,
        ServerMessage::ProjectileShot(SProjectileShot {
            id,
            face_yaw: msg.face_yaw,
            face_pitch: msg.face_pitch,
            pattern: actual_pattern.map(str::to_owned),
        }),
    );
}

fn resolved_pattern<'a>(requested: Option<&'a str>, multi_shot: &'a MultiShotConfig) -> Option<&'a str> {
    requested.map(|name| {
        multi_shot
            .pattern(name)
            .map(|_| name)
            .unwrap_or_else(|| multi_shot.first_allowed_pattern().0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authoritative_pattern_preserves_single_shots_and_falls_back_for_unknown_patterns() {
        let multi_shot: MultiShotConfig = serde_json::from_str(
            r#"{"spread_degrees":2.0,"allowed_patterns":["first_2","second_2"],"patterns":{"first_2":{"stencil":["xo"],"column_scale":1.0,"row_scale":1.0},"second_2":{"stencil":["xo"],"column_scale":1.0,"row_scale":1.0},"dormant_2":{"stencil":["xo"],"column_scale":1.0,"row_scale":1.0}}}"#,
        )
        .expect("test multi-shot config failed to parse");
        assert_eq!(resolved_pattern(Some("second_2"), &multi_shot), Some("second_2"));
        assert_eq!(resolved_pattern(Some("dormant_2"), &multi_shot), Some("first_2"));
        assert_eq!(resolved_pattern(Some("unknown"), &multi_shot), Some("first_2"));
        assert_eq!(resolved_pattern(None, &multi_shot), None);
    }
}
