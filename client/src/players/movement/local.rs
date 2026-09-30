use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    prelude::*,
};
use common::{
    config::{GameplayConfig, NetworkConfig},
    map::Carriers,
    physics::{
        CharacterMovePlan, CharacterSupport, CharacterVerticalVelocity, CollisionWorld, HorizontalVelocity,
        KnockbackVelocity, PortalSet,
    },
    protocol::{
        ActorId, ActorMarker, CMoveOutcome, ClientMessage, FaceYaw, MapSettings, PlayerId, PlayerMarker,
        PlayerMoveIntent, PlayerStance, Position, PowerUpKind, SwitchState,
    },
};

use super::{JumpRequested, LocalMovementStep, OwnerBody, OwnerWorld, feedback::bump, owner::owner_tick};
use crate::{
    actors::ActorMap,
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    network::ClientToServerChannel,
    players::{
        BumpFeedbackState, CrouchBlend, LocalPlayerInfo, LocalPlayerMarker, PlayerAnimationMotion, PlayerMap,
        eye_position,
    },
    portals::apply_portal_view,
};

// Below this horizontal speed a tick counts as standing still.
const STANDSTILL_SPEED: f32 = 0.5;

// The local body's components, which the owner's tick reads and writes.
#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct LocalBody {
    pub entity: Entity,
    pub id: &'static PlayerId,
    pub position: &'static mut Position,
    pub previous_position: &'static mut PreviousTickPosition,
    pub intent: &'static mut PlayerMoveIntent,
    pub face_yaw: &'static mut FaceYaw,
    pub vertical_velocity: &'static mut CharacterVerticalVelocity,
    pub horizontal_velocity: &'static mut HorizontalVelocity,
    pub knockback: &'static mut KnockbackVelocity,
    pub stance: &'static mut PlayerStance,
    pub support: &'static mut CharacterSupport,
    pub jump_requested: &'static mut JumpRequested,
    pub step: &'static mut LocalMovementStep,
    pub crouch_blend: &'static CrouchBlend,
}

type RemoteBody = (
    Entity,
    &'static Position,
    &'static CharacterVerticalVelocity,
    &'static PlayerStance,
);

// The world the owner's tick runs in.
#[derive(SystemParam)]
pub(crate) struct OwnerResources<'w> {
    time: Res<'w, Time>,
    collision_world: Res<'w, CollisionWorld>,
    carriers: Res<'w, Carriers>,
    map_settings: Res<'w, MapSettings>,
    gameplay_config: Res<'w, GameplayConfig>,
    portal_set: Res<'w, PortalSet>,
    switch_state: Res<'w, SwitchState>,
    network: Res<'w, NetworkConfig>,
    players: Res<'w, PlayerMap>,
    actors: Res<'w, ActorMap>,
}

// The owner's simulation, one fixed tick: the tick over the local body, the
// portal view for a crossing, and the messages the server gets.
pub(crate) fn local_player_movement_system(
    mut commands: Commands,
    resources: OwnerResources,
    to_server: Res<ClientToServerChannel>,
    mut local: ResMut<LocalPlayerInfo>,
    cameras: Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    mut body_query: Query<LocalBody, With<LocalPlayerMarker>>,
    remote_bodies: Query<RemoteBody, (With<PlayerMarker>, Without<LocalPlayerMarker>)>,
    actor_bodies: Query<
        (Entity, &ActorId, &Position),
        (With<ActorMarker>, Without<PlayerMarker>, Without<LocalPlayerMarker>),
    >,
) {
    let OwnerResources {
        time,
        collision_world,
        carriers,
        map_settings,
        gameplay_config,
        portal_set,
        switch_state,
        network,
        players,
        actors,
    } = resources;
    let Ok(body) = body_query.single_mut() else {
        return;
    };
    let LocalBodyItem {
        entity,
        id,
        position,
        previous_position,
        intent,
        face_yaw,
        vertical_velocity,
        horizontal_velocity,
        knockback,
        stance,
        support,
        jump_requested,
        step,
        crouch_blend,
    } = body;
    let position = position.into_inner();
    let previous_position = previous_position.into_inner();
    if local.is_dead {
        // A dead body stays where it is, so the render lerp collapses onto
        // its position instead of replaying the last step.
        previous_position.0 = *position;
        return;
    }
    let mut blockers: Vec<CharacterMovePlan> = actor_bodies
        .iter()
        .filter_map(|(entity, id, pos)| {
            let info = actors.get(id)?;
            Some(CharacterMovePlan::stationary(
                entity,
                *pos,
                0.0,
                gameplay_config.expect_actor(&info.kind).physics(),
            ))
        })
        .collect();
    blockers.extend(remote_bodies.iter().map(|(entity, position, vertical, stance)| {
        CharacterMovePlan::stationary(entity, *position, vertical.0, stance.physics(&gameplay_config.player))
    }));
    let info = players.get(id);
    let world = OwnerWorld {
        collision_world: &collision_world,
        carriers: &carriers,
        map_settings: &map_settings,
        gameplay_config: &gameplay_config,
        portal_set: &portal_set,
        network: &network,
        open_fields: &switch_state.open_fields,
        held_keys: info.map_or(&[], |info| info.held_keys.as_slice()),
        has_speed: info.is_some_and(|info| info.power_up(PowerUpKind::Speed)),
        has_low_gravity: info.is_some_and(|info| info.power_up(PowerUpKind::LowGravity)),
        stunned: info.is_some_and(|info| info.stunned),
        delta: time.delta_secs(),
    };
    let face_yaw = face_yaw.into_inner();
    let outcome = owner_tick(
        entity,
        OwnerBody {
            position: &mut *position,
            previous_position: &mut previous_position.0,
            intent: intent.into_inner(),
            face_yaw: &mut *face_yaw,
            vertical_velocity: vertical_velocity.into_inner(),
            horizontal_velocity: horizontal_velocity.into_inner(),
            knockback: knockback.into_inner(),
            stance: stance.into_inner(),
            support: support.into_inner(),
            jump_requested: &mut jump_requested.into_inner().0,
            step: step.into_inner(),
            reports: &mut local.reports,
        },
        &world,
        &blockers,
    );
    if let Some(hop) = outcome.hop {
        apply_portal_view(
            &mut commands,
            cameras.single().ok(),
            &mut local,
            eye_position(*position, crouch_blend.eye_height(&gameplay_config.player)),
            &hop.entry,
            &hop.exit,
            face_yaw.0,
        );
    }
    let generation = local.reports.generation;
    for event in outcome.outcomes {
        to_server.send(ClientMessage::MoveOutcome(CMoveOutcome { generation, event }));
    }
    if let Some(report) = outcome.report {
        to_server.send(ClientMessage::Move(report));
    }
}

// What the tick shows: the animation's motion and the bump sound, from the
// step the owner just recorded.
pub(crate) fn local_player_feedback_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    client_settings: Res<ClientSettings>,
    mut query: Query<
        (
            &Position,
            &LocalMovementStep,
            &mut PlayerAnimationMotion,
            &mut BumpFeedbackState,
        ),
        (With<LocalPlayerMarker>, Changed<LocalMovementStep>),
    >,
) {
    let delta = time.delta_secs();
    for (position, step, mut animation, mut feedback) in &mut query {
        let result = &step.result;
        animation.record_step(
            step.start,
            result,
            if result.support == CharacterSupport::Ladder {
                step.intent_velocity
            } else {
                step.horizontal_velocity
            },
            step.knockback_displacement,
            delta,
        );
        if step.hits_character || result.blocked {
            bump(
                &mut commands,
                &asset_server,
                &asset_set,
                &client_settings.audio.bump,
                &mut feedback,
                !step.hits_character,
            );
        } else {
            let moved = (position.x - step.start.x).hypot(position.z - step.start.z);
            // Standing still ends the run-up, so a hop at a wall from
            // beside it starts from nothing.
            feedback.run_up = if moved > delta * STANDSTILL_SPEED {
                feedback.run_up + moved
            } else {
                0.0
            };
        }
    }
}

#[cfg(test)]
#[path = "tests/local.rs"]
mod tests;
