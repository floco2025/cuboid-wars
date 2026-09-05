use std::collections::HashMap;

use bevy::prelude::*;

use crate::players::PlayerMap;
use common::{
    config::GameplayConfig,
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity, PortalSet},
    protocol::{FaceYaw, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
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
    map_settings: Res<MapSettings>,
    mut previous: Local<HashMap<Entity, Position>>,
    mut player_query: Query<
        (
            Entity,
            &PlayerId,
            &mut Position,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &mut PlayerMoveIntent,
            Option<&mut KnockbackVelocity>,
            Option<&mut AirborneMomentum>,
        ),
        With<PlayerMarker>,
    >,
) {
    let mut seen: HashMap<Entity, Position> = HashMap::new();
    for (entity, id, mut pos, mut face_yaw, mut vertical_velocity, mut move_intent, knockback, momentum) in
        &mut player_query
    {
        let from = previous.get(&entity).copied();
        let hop = from.and_then(|from| {
            if portal_set.is_empty() {
                return None;
            }
            let info = players.get(id)?;
            portal_set.player_hop(
                Vec3::from(from),
                Vec3::from(*pos),
                &gameplay_config,
                &map_settings.movement,
                *move_intent,
                info.has_speed(),
                info.is_stunned(),
                knockback.as_deref(),
                momentum.as_deref(),
                vertical_velocity.0,
                face_yaw.0,
            )
        });
        if let Some(hop) = hop {
            hop.apply_player_state(&mut pos, &mut face_yaw, &mut vertical_velocity, &mut move_intent);
            hop.apply_motion_components(&mut commands, entity, knockback, momentum);
            if let Some(info) = players.get_mut(id) {
                // No fall damage across a portal: the drop tracker restarts
                // at the exit.
                info.life.fall_state.reset();
                info.session.hops = info.session.hops.wrapping_add(1);
            }
            // Not broadcast: every client simulates every player's crossings
            // from the shared geometry; the snapshot corrects a wrong guess.
            debug!("{} passed through a portal", players.describe(id));
        }
        seen.insert(entity, *pos);
    }
    *previous = seen;
}
