use super::{EraserContacts, PlayerMap, PlayerMovementReport, reconciliation::reconcile_player_movement};
use crate::portals::{broadcast_portal_crossing, resolve_portal_crossing};
use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterEnvironment, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity,
        LadderMode, PortalSet, inspect_character_support, passable_barrier_kinds, player_control_velocity,
    },
    protocol::{
        FaceYaw, MapSettings, PlateState, PlayerId, PlayerMarker, PlayerMoveIntent, PlayerMovementState, Position,
        ServerTick,
    },
};

pub(crate) fn apply_pending_player_inputs_system(
    mut players: ResMut<PlayerMap>,
    mut query: Query<(&PlayerId, &mut PlayerMoveIntent, &mut FaceYaw), With<PlayerMarker>>,
) {
    for (id, mut intent, mut yaw) in &mut query {
        let Some(info) = players.get_mut(id) else {
            continue;
        };
        info.life.processed_move_seq = None;
        let Some(report) = info.life.pending_moves.front() else {
            continue;
        };
        *intent = report.comparison_state().move_intent;
        yaw.0 = report.comparison_state().face_yaw;
    }
}

pub(crate) fn finish_player_movement_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    gameplay: Res<GameplayConfig>,
    settings: Res<MapSettings>,
    collision: Res<CollisionWorld>,
    carriers: Res<Carriers>,
    plates: Res<PlateState>,
    portal_set: Res<PortalSet>,
    time: Res<Time>,
    tick: Res<ServerTick>,
    mut erasers: ResMut<EraserContacts>,
    mut query: Query<
        (
            Entity,
            &PlayerId,
            &mut Position,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &mut PlayerMoveIntent,
            Option<&KnockbackVelocity>,
            Option<&AirborneMomentum>,
        ),
        With<PlayerMarker>,
    >,
) {
    let mut crossings = Vec::new();
    for (entity, id, mut pos, mut yaw, mut vertical, mut intent, knockback, airborne) in &mut query {
        let Some(info) = players.get_mut(id) else {
            continue;
        };
        let start = info.life.movement_start.take().unwrap_or(*pos);
        let mut movement = PlayerMovementState::new(*pos, *intent, vertical.0, yaw.0).with_momentum(
            airborne.map_or(Vec3::ZERO, |m| m.0),
            knockback.map_or(Vec3::ZERO, |m| m.0),
        );
        let mut portal_entrance = None;
        let accepted = match info.life.pending_moves.pop() {
            Some(PlayerMovementReport::Move(report)) => {
                reconcile_player_movement(*id, info, report.seq, report.movement, &mut movement)
            }
            Some(PlayerMovementReport::PortalCross(report)) => {
                let result = resolve_portal_crossing(*id, info, &report, &mut movement, tick.0);
                if result.accepted {
                    portal_entrance = Some(report.entrance.pos);
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
        commands.entity(entity).insert((
            AirborneMomentum(Vec3::from_array(movement.airborne_momentum)),
            KnockbackVelocity(Vec3::from_array(movement.knockback)),
        ));
        if accepted {
            let passable = passable_barrier_kinds(&info.life.held_keys, &plates.open_barrier_kinds);
            let control = player_control_velocity(*intent, &settings.movement, info.has_speed(), info.is_stunned());
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
            let (support, grounding, crushed) = inspect_character_support(
                *pos,
                vertical.0,
                control,
                &env,
                time.delta_secs(),
                info.life.fall_state.is_crushed(),
            );
            info.life.fall_state.record_movement(support, crushed);
            commands.entity(entity).insert((support, grounding));
        }
        let sweep_end = portal_entrance.unwrap_or(*pos);
        erasers.swept.extend(
            collision
                .character_eraser_contacts(&start, &sweep_end, gameplay.player.physics(), Some(&carriers))
                .map(|field| (*id, field)),
        );
    }
    for result in crossings {
        broadcast_portal_crossing(&players, result);
    }
}
