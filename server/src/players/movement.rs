use super::{EraserContacts, PlayerMap, reconciliation::reconcile_player_movement};
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
    },
};

pub(crate) struct PlayerMovementPath {
    pub start: Position,
    pub start_hops: u32,
    // Eraser sweeps stop at the entry, never across the gap between portals.
    pub portal_entry: Option<Position>,
}

pub(crate) fn apply_pending_player_inputs_system(
    mut players: ResMut<PlayerMap>,
    mut query: Query<(&PlayerId, &mut PlayerMoveIntent, &mut FaceYaw), With<PlayerMarker>>,
) {
    for (id, mut intent, mut yaw) in &mut query {
        let Some(info) = players.get_mut(id) else {
            continue;
        };
        info.life.processed_move_seq = None;
        let Some(report) = &info.life.pending_move else {
            continue;
        };
        if report.hops == info.session.hops {
            *intent = report.input.move_intent;
            yaw.0 = report.input.face_yaw;
        }
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
    for (entity, id, mut pos, mut yaw, mut vertical, mut intent, knockback, airborne) in &mut query {
        let Some(info) = players.get_mut(id) else {
            continue;
        };
        let path = info.life.movement_path.take().unwrap_or(PlayerMovementPath {
            start: *pos,
            start_hops: info.session.hops,
            portal_entry: None,
        });
        let predicted_hops = info.session.hops;
        let mut movement = PlayerMovementState::new(*pos, *intent, vertical.0, yaw.0).with_momentum(
            airborne.map_or(Vec3::ZERO, |m| m.0),
            knockback.map_or(Vec3::ZERO, |m| m.0),
        );
        let accepted = reconcile_player_movement(*id, info, &mut movement);
        if info.session.hops != path.start_hops {
            info.life.fall_state.reset();
        }
        *pos = movement.pos;
        *intent = movement.move_intent;
        yaw.0 = movement.face_yaw;
        vertical.0 = movement.vertical_velocity;
        commands.entity(entity).insert((
            AirborneMomentum(Vec3::from_array(movement.airborne_momentum)),
            KnockbackVelocity(Vec3::from_array(movement.knockback)),
        ));
        if accepted || info.session.hops != path.start_hops {
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
        let sweep_end = if info.session.hops == path.start_hops {
            Some(*pos)
        } else if info.session.hops == predicted_hops {
            path.portal_entry
        } else {
            None
        };
        if let Some(end) = sweep_end {
            erasers.swept.extend(
                collision
                    .character_eraser_contacts(&path.start, &end, gameplay.player.physics(), Some(&carriers))
                    .map(|field| (*id, field)),
            );
        }
    }
}
