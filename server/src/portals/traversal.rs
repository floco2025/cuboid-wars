use bevy::prelude::*;

use crate::{config::ServerGameplayConfig, network::broadcast_to_all, players::PlayerMap};
use common::constants::{CHARACTER_TERMINAL_VELOCITY, PORTAL_FAST_ENTRY_SPEED, PORTAL_MIN_APPROACH_SPEED};
use common::{
    config::GameplayConfig,
    constants::PORTAL_KNOCKBACK_CARRY_FACTOR,
    physics::{CharacterVerticalVelocity, KnockbackVelocity, PortalSet, player_control_velocity},
    protocol::{
        FaceYaw, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, Position, SPlayerTeleport, ServerMessage,
    },
};

// Runs right after the movement step: the trigger reads final post-collision
// positions, and the direct `Position` write lands in this tick's snapshot.
pub fn players_portal_traversal_system(
    mut commands: Commands,
    portal_set: Res<PortalSet>,
    mut players: ResMut<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_settings: Res<MapSettings>,
    mut player_query: Query<
        (
            Entity,
            &PlayerId,
            &mut Position,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &PlayerMoveIntent,
            Option<&mut KnockbackVelocity>,
        ),
        With<PlayerMarker>,
    >,
) {
    if portal_set.is_empty() {
        return;
    }
    let knockback_cap = PORTAL_KNOCKBACK_CARRY_FACTOR * gameplay_config.movement.knockback.max_speed;
    for (entity, id, mut pos, mut face_yaw, mut vertical_velocity, move_intent, knockback) in &mut player_query {
        let Some(info) = players.get(id) else { continue };
        // The same velocity the movement step just integrated: intent-derived
        // control plus knockback plus the persistent vertical velocity.
        let control = player_control_velocity(*move_intent, &gameplay_config, info.has_speed(), info.is_stunned());
        let knockback_velocity = knockback.as_ref().map_or(Vec3::ZERO, |k| k.0);
        let gravity = map_settings.gravity_for(info.has_low_gravity());
        let entry_vertical =
            entry_vertical_velocity(vertical_velocity.0, info.life.fall_state.fall_energy(), pos.y, gravity);
        // The cooldown paces only slow (resting) re-triggers. A body with
        // real persistent speed passes: a fast fall chain teleports again
        // within a tick or two, far inside any fixed cooldown.
        if info.life.portal_cooldown > 0.0
            && (knockback_velocity + Vec3::Y * entry_vertical).length() < PORTAL_FAST_ENTRY_SPEED
        {
            continue;
        }
        let Some(hop) = portal_set.character_hop(
            Vec3::from(*pos),
            gameplay_config.player.physics(),
            control,
            knockback_velocity,
            entry_vertical,
            face_yaw.0,
            knockback_cap,
        ) else {
            continue;
        };

        let from_pos = *pos;
        *pos = hop.origin.into();
        face_yaw.0 = hop.yaw;
        vertical_velocity.0 = hop.vertical_velocity;
        match knockback {
            Some(mut existing) => existing.0 = hop.knockback,
            None => {
                commands.entity(entity).insert(KnockbackVelocity(hop.knockback));
            }
        }
        if let Some(info) = players.get_mut(id) {
            info.life.fall_state.reset();
            let exit_down = hop.vertical_velocity.min(0.0);
            info.life
                .fall_state
                .seed_energy(exit_down.mul_add(exit_down, 2.0 * gravity * hop.origin.y));
            info.life.portal_cooldown = server_gameplay_config.portals.teleport_cooldown_secs;
        }
        debug!("{} teleported through a portal", players.describe(id));
        broadcast_to_all(
            &players,
            ServerMessage::PlayerTeleport(SPlayerTeleport {
                id: *id,
                from_pos,
                pos: *pos,
                face_yaw: hop.yaw,
                vertical_velocity: hop.vertical_velocity,
                velocity_x: hop.knockback.x,
                velocity_z: hop.knockback.z,
            }),
        );
    }
}

// The vertical speed a body brings INTO a portal. Landing on a standable
// portal is part of the movement step, which grounds the character and
// zeroes its vertical velocity before this system runs; the fall tracker
// still holds the airborne window's arrival energy (max of v² + 2gy) until
// the damage system consumes it. That recovers the exact speed at the
// portal's height with no integration lag, and it accumulates across hops —
// a ceiling exit that was already falling keeps everything it carried, so a
// portal fall chain builds toward terminal velocity instead of decaying.
fn entry_vertical_velocity(vertical_velocity: f32, fall_energy: f32, current_y: f32, gravity: f32) -> f32 {
    let arrival_sq = fall_energy - 2.0 * gravity * current_y;
    let fall_speed = arrival_sq.max(0.0).sqrt().min(CHARACTER_TERMINAL_VELOCITY);
    if fall_speed > PORTAL_MIN_APPROACH_SPEED {
        vertical_velocity.min(-fall_speed)
    } else {
        vertical_velocity
    }
}

#[cfg(test)]
mod tests {
    use super::entry_vertical_velocity;
    use common::constants::CHARACTER_TERMINAL_VELOCITY;

    #[test]
    fn landing_recovers_the_fall_speed_from_energy() {
        // 8 m drop from rest at g = 25: energy 2·25·8 = 400 → 20 m/s at y = 0.
        assert_eq!(entry_vertical_velocity(0.0, 400.0, 0.0, 25.0), -20.0);
    }

    #[test]
    fn carried_exit_speed_accumulates_into_the_next_hop() {
        // Exited a ceiling at 12 m/s from 2 m up: 144 + 2·25·2 = 244.
        let entry = entry_vertical_velocity(0.0, 244.0, 0.0, 25.0);
        assert!((entry + 244.0_f32.sqrt()).abs() < 1e-4);
    }

    #[test]
    fn mid_fall_keeps_a_faster_live_velocity() {
        assert_eq!(entry_vertical_velocity(-21.0, 100.0, 0.0, 25.0), -21.0);
    }

    #[test]
    fn rising_after_a_grounded_jump_is_untouched() {
        assert_eq!(entry_vertical_velocity(12.0, 0.0, 0.0, 25.0), 12.0);
    }

    #[test]
    fn rising_at_its_own_energy_frontier_is_untouched() {
        // While rising, tracked energy never exceeds 2·g·y, so no fall speed.
        assert_eq!(entry_vertical_velocity(8.0, 150.0, 3.0, 25.0), 8.0);
    }

    #[test]
    fn recovered_speed_caps_at_terminal_velocity() {
        assert_eq!(
            entry_vertical_velocity(0.0, 1_000_000.0, 0.0, 25.0),
            -CHARACTER_TERMINAL_VELOCITY
        );
    }
}
