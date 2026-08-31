use std::collections::HashMap;

use bevy::prelude::*;

use crate::players::PlayerMap;
use common::{
    config::GameplayConfig,
    constants::PORTAL_KNOCKBACK_CARRY_FACTOR,
    physics::{CharacterVerticalVelocity, KnockbackVelocity, PortalSet, player_control_velocity},
    protocol::{FaceYaw, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

// Runs right after the movement step. The step already let the body sink
// into any linked aperture (its backing colliders are excluded while the
// body overlaps it); the tick the body's center crosses the plane, it
// continues from the paired end. The direct `Position` write lands in this
// tick's snapshot. `previous` holds each entity's post-step position from
// the last tick — the "from" side of the crossing test.
pub fn players_portal_traversal_system(
    mut commands: Commands,
    portal_set: Res<PortalSet>,
    mut players: ResMut<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    mut previous: Local<HashMap<Entity, Position>>,
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
    let knockback_cap = PORTAL_KNOCKBACK_CARRY_FACTOR * gameplay_config.movement.knockback.max_speed;
    let mut seen: HashMap<Entity, Position> = HashMap::new();
    for (entity, id, mut pos, mut face_yaw, mut vertical_velocity, move_intent, knockback) in &mut player_query {
        let from = previous.get(&entity).copied();
        let hop = from.and_then(|from| {
            if portal_set.is_empty() {
                return None;
            }
            let info = players.get(id)?;
            let control = player_control_velocity(*move_intent, &gameplay_config, info.has_speed(), info.is_stunned());
            let knockback_velocity = knockback.as_ref().map_or(Vec3::ZERO, |k| k.0);
            portal_set.character_hop(
                Vec3::from(from),
                Vec3::from(*pos),
                gameplay_config.player.physics(),
                control,
                knockback_velocity,
                vertical_velocity.0,
                face_yaw.0,
                knockback_cap,
            )
        });
        if let Some(hop) = hop {
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
                // No fall damage across a portal: the drop tracker restarts
                // at the exit.
                info.life.fall_state.reset();
            }
            // Not broadcast: every client simulates every player's crossings
            // from the shared geometry; the snapshot corrects a wrong guess.
            debug!("{} passed through a portal", players.describe(id));
        }
        seen.insert(entity, *pos);
    }
    *previous = seen;
}
