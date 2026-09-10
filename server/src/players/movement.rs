use super::{
    EraserContacts, PlayerMap, PlayerMovementReport,
    reconciliation::{adopt_trusted_movement, resolve_portal_crossing},
};
use crate::{characters::MovementStart, network::broadcast_portal_crossing};
use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterEnvironment, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity,
        LadderMode, PortalSet, character_crushed_at, passable_barrier_kinds, player_movement_state,
    },
    protocol::{FaceYaw, MapSettings, PlateState, PlayerId, PlayerMarker, PlayerMoveIntent, Position, ServerTick},
};

pub(crate) fn apply_pending_player_inputs_system(
    players: Res<PlayerMap>,
    mut query: Query<(&PlayerId, &mut PlayerMoveIntent, &mut FaceYaw), With<PlayerMarker>>,
) {
    for (id, mut intent, mut yaw) in &mut query {
        let Some(report) = players.get(id).and_then(|info| info.life.pending_moves.front()) else {
            continue;
        };
        *intent = report.comparison_state().move_intent;
        yaw.0 = report.comparison_state().face_yaw;
    }
}

// Chooses each player's position for this tick: the client's report where
// it is trusted, the server's step otherwise. Support comes with an accepted
// report, since the client's step already judged it; only the crush test is
// re-run there. Eraser sweeps run from the step's start to the chosen
// position, stopping at a crossing's entrance.
pub(crate) fn finish_player_movement_system(
    mut players: ResMut<PlayerMap>,
    gameplay: Res<GameplayConfig>,
    settings: Res<MapSettings>,
    collision: Res<CollisionWorld>,
    carriers: Res<Carriers>,
    plates: Res<PlateState>,
    portal_set: Res<PortalSet>,
    tick: Res<ServerTick>,
    mut erasers: ResMut<EraserContacts>,
    mut query: Query<
        (
            &PlayerId,
            &MovementStart,
            &mut Position,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &mut PlayerMoveIntent,
            &mut AirborneMomentum,
            &mut KnockbackVelocity,
        ),
        With<PlayerMarker>,
    >,
) {
    let mut crossings = Vec::new();
    for (id, start, mut pos, mut yaw, mut vertical, mut intent, mut momentum, mut knockback) in &mut query {
        let Some(info) = players.get_mut(id) else {
            continue;
        };
        let mut movement = player_movement_state(
            *pos,
            *intent,
            &yaw,
            &vertical,
            &momentum,
            &knockback,
            info.life.fall_state.support(),
        );
        let report = info.life.pending_moves.pop_front();
        info.life.processed_move_seq = report.as_ref().map(PlayerMovementReport::seq);
        let mut sweep_end = None;
        let accepted = match report {
            Some(PlayerMovementReport::Move(report)) => {
                adopt_trusted_movement(*id, info, report.seq, report.movement, &mut movement)
            }
            Some(PlayerMovementReport::PortalCross(report)) => {
                let result = resolve_portal_crossing(*id, info, &report, &mut movement, tick.0);
                if result.accepted {
                    sweep_end = Some(report.entrance.pos);
                }
                crossings.push(result);
                result.accepted
            }
            None => false,
        };
        *pos = movement.pos;
        *intent = movement.move_intent;
        yaw.0 = movement.face_yaw;
        vertical.0 = movement.vertical_velocity;
        momentum.0 = Vec3::from_array(movement.airborne_momentum);
        knockback.0 = Vec3::from_array(movement.knockback);
        if accepted {
            let passable = passable_barrier_kinds(&info.life.held_keys, &plates.open_barrier_kinds);
            let env = CharacterEnvironment {
                collision_world: &collision,
                gravity: settings.gravity_for(info.has_low_gravity()),
                passable_kinds: &passable,
                physics: gameplay.player.physics(),
                ladder_climb_ratio: settings.movement.ladder_climb_ratio,
                ladder_mode: LadderMode::Automatic,
                portals: Some(&portal_set),
                carriers: &carriers,
            };
            let lifted = info.life.fall_state.was_lifted();
            info.life
                .fall_state
                .record_movement(movement.support, character_crushed_at(*pos, &env, lifted), lifted);
        }
        erasers.swept.extend(
            collision
                .character_eraser_contacts(
                    &start.0,
                    &sweep_end.unwrap_or(*pos),
                    gameplay.player.physics(),
                    Some(&carriers),
                )
                .map(|field| (*id, field)),
        );
    }
    // Sent after the loop: the loop holds the roster mutably.
    for result in crossings {
        broadcast_portal_crossing(&players, result);
    }
}
