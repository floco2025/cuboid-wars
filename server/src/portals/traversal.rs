use bevy::prelude::*;

use crate::{config::ServerGameplayConfig, network::broadcast_to_all, players::PlayerMap};
use common::{
    config::GameplayConfig,
    constants::PORTAL_KNOCKBACK_CARRY_FACTOR,
    physics::{CharacterVerticalVelocity, KnockbackVelocity, PortalSet, player_control_velocity},
    protocol::{FaceYaw, PlayerId, PlayerMarker, PlayerMoveIntent, Position, SPlayerTeleport, ServerMessage},
};

// Runs right after the movement step: the trigger reads final post-collision
// positions, and the direct `Position` write lands in this tick's snapshot.
pub fn players_portal_traversal_system(
    mut commands: Commands,
    portal_set: Res<PortalSet>,
    mut players: ResMut<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
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
        if info.life.portal_cooldown > 0.0 {
            continue;
        }
        // The same velocity the movement step just integrated: intent-derived
        // control plus knockback plus the persistent vertical velocity.
        let control = player_control_velocity(*move_intent, &gameplay_config, info.has_speed(), info.is_stunned());
        let knockback_velocity = knockback.as_ref().map_or(Vec3::ZERO, |k| k.0);
        let Some(hop) = portal_set.character_hop(
            Vec3::from(*pos),
            gameplay_config.player.physics(),
            control,
            knockback_velocity,
            vertical_velocity.0,
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
