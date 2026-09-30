use bevy::prelude::*;
use common::{
    config::{GameplayConfig, NetworkConfig},
    map::Carriers,
    physics::{
        CharacterMovePlan, CharacterPortalHop, CharacterSupport, CharacterVerticalVelocity, CollisionWorld,
        HorizontalVelocity, KnockbackVelocity, PlayerHopBody, PlayerJump, PlayerMovementStep, PortalSet,
        passable_fields, player_jump,
    },
    protocol::{
        CMove, CarrierId, FaceYaw, FieldId, MapSettings, MoveOutcome, PlayerMoveIntent, PlayerMovementState,
        PlayerStance, Position,
    },
};

use super::{LocalMovementReports, LocalMovementStep, collect_move_outcomes, plan_player_move};

// A jump the input asked for that no tick has consumed yet: it survives a
// frame without fixed steps and fires in exactly one.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct JumpRequested(pub bool);

// The local body as the owner's tick reads and writes it. The rendered
// client builds the view from the body's components, the headless
// experiment from its own fields, so one tick serves both.
pub struct OwnerBody<'a> {
    pub position: &'a mut Position,
    pub previous_position: &'a mut Position,
    pub intent: &'a mut PlayerMoveIntent,
    pub face_yaw: &'a mut FaceYaw,
    pub vertical_velocity: &'a mut CharacterVerticalVelocity,
    pub horizontal_velocity: &'a mut HorizontalVelocity,
    pub knockback: &'a mut KnockbackVelocity,
    pub stance: &'a mut PlayerStance,
    pub support: &'a mut CharacterSupport,
    pub jump_requested: &'a mut bool,
    pub step: &'a mut LocalMovementStep,
    pub reports: &'a mut LocalMovementReports,
}

impl OwnerBody<'_> {
    // The state a report carries, in world space.
    #[must_use]
    pub fn movement_state(&self) -> PlayerMovementState {
        PlayerMovementState {
            carrier: CarrierId::WORLD,
            pos: *self.position,
            move_intent: *self.intent,
            vertical_velocity: self.vertical_velocity.0,
            face_yaw: self.face_yaw.0,
            horizontal_velocity: self.horizontal_velocity.0.to_array(),
            knockback: self.knockback.0.to_array(),
            support: *self.support,
            stance: *self.stance,
        }
    }
}

// What the world holds for the owner this tick.
pub struct OwnerWorld<'a> {
    pub collision_world: &'a CollisionWorld,
    pub carriers: &'a Carriers,
    pub map_settings: &'a MapSettings,
    pub gameplay_config: &'a GameplayConfig,
    pub portal_set: &'a PortalSet,
    pub network: &'a NetworkConfig,
    pub open_fields: &'a [FieldId],
    pub held_keys: &'a [FieldId],
    pub has_speed: bool,
    pub has_low_gravity: bool,
    pub stunned: bool,
    pub delta: f32,
}

pub struct OwnerTickOutcome {
    // What a requested jump did, `None` when none was requested or it was refused.
    pub jump: Option<PlayerJump>,
    pub hop: Option<CharacterPortalHop>,
    pub outcomes: Vec<MoveOutcome>,
    pub report: Option<CMove>,
}

// The facing a held climber keeps: square to the ladder, whatever the view does.
#[must_use]
pub fn ladder_facing(collision: &CollisionWorld, position: &Position, support: CharacterSupport) -> Option<f32> {
    (support == CharacterSupport::Ladder)
        .then(|| collision.ladder_volume_at(position))
        .flatten()
        .map(|ladder| (-ladder.normal_x).atan2(-ladder.normal_z))
}

// One fixed tick of the owner's simulation: the jump it was asked for, the
// step against the world and the other bodies, the portal crossing the step
// made, the outcomes the server must hear about, and the periodic report.
// Nothing corrects it afterwards; only a new body generation moves the body
// otherwise.
pub fn owner_tick(
    entity: Entity,
    body: OwnerBody<'_>,
    world: &OwnerWorld<'_>,
    blockers: &[CharacterMovePlan],
) -> OwnerTickOutcome {
    let gameplay = world.gameplay_config;
    let settings = world.map_settings;
    let passable = passable_fields(world.held_keys, world.open_fields);
    let jump = (std::mem::take(body.jump_requested) && !body.stance.crouched && !world.stunned)
        .then(|| {
            player_jump(
                body.step.result.support,
                *body.intent,
                body.vertical_velocity.0,
                world.collision_world,
                body.stance.physics(&gameplay.player),
                &settings.movement,
                world.has_speed,
                body.position,
                &passable,
            )
        })
        .flatten();
    match jump {
        Some(PlayerJump::Rise(velocity)) => body.vertical_velocity.0 = velocity,
        Some(PlayerJump::Release(shove)) => body.knockback.0 += shove,
        None => {}
    }
    if let Some(yaw) = ladder_facing(world.collision_world, body.position, body.step.result.support) {
        body.face_yaw.0 = yaw;
    }
    let planned = plan_player_move(
        entity,
        PlayerMovementStep {
            start: *body.position,
            vertical_velocity: body.vertical_velocity.0,
            horizontal_velocity: body.horizontal_velocity.0,
            stance: *body.stance,
            intent: *body.intent,
            has_speed: world.has_speed,
            disabled: world.stunned,
            delta: world.delta,
            has_low_gravity: world.has_low_gravity,
            held_keys: world.held_keys,
            open_fields: world.open_fields,
            knockback_displacement: body.knockback.step(world.delta),
            collision_world: world.collision_world,
            map_settings: settings,
            gameplay_config: gameplay,
            portal_set: world.portal_set,
            carriers: world.carriers,
        },
        blockers,
    );
    let step = planned.step;
    let result = step.movement;
    *body.previous_position = step.start;
    *body.position = result.position;
    body.vertical_velocity.0 = result.vertical_velocity;
    body.horizontal_velocity.0 = step.horizontal_velocity;
    *body.stance = step.stance;
    *body.support = result.support;
    *body.step = LocalMovementStep {
        start: step.start,
        result,
        horizontal_velocity: step.horizontal_velocity,
        intent_velocity: step.intent_velocity,
        knockback_displacement: step.knockback_displacement,
        hits_character: planned.hits_character,
    };

    let hop = world.portal_set.player_hop(
        Vec3::from(step.start),
        Vec3::from(*body.position),
        gameplay,
        &settings.movement,
        PlayerHopBody {
            stance: *body.stance,
            knockback: body.knockback,
            horizontal_velocity: body.horizontal_velocity,
            vertical_velocity: body.vertical_velocity.0,
            carried: result.carried(),
            yaw: body.face_yaw.0,
        },
        world.delta,
    );
    if let Some(hop) = &hop {
        let entrance = *body.position;
        hop.apply_player_state(
            body.position,
            body.face_yaw,
            body.vertical_velocity,
            body.intent,
            body.stance,
        );
        hop.apply_motion_components(body.knockback, body.horizontal_velocity);
        // Render interpolation anchors at the exit: the transit is a cut
        // there, not a smear between the portals.
        *body.previous_position = *body.position;
        body.reports.begin_crossing(entrance);
    }

    let outcomes = collect_move_outcomes(
        body.position,
        body.step,
        body.stance.physics(&gameplay.player),
        world.collision_world,
        world.carriers,
        body.reports,
        world.network,
    );
    let report = body
        .reports
        .movement_report(world.network, body.movement_state(), result.carrier, world.carriers);
    body.knockback
        .decay(world.delta, settings.movement.knockback.deceleration);
    OwnerTickOutcome {
        jump,
        hop,
        outcomes,
        report,
    }
}

#[cfg(test)]
#[path = "tests/owner.rs"]
mod tests;
