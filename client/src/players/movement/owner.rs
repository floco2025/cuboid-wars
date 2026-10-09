use bevy::prelude::*;
use common::{
    config::{GameplayConfig, NetworkConfig},
    constants::{PLAYER_COYOTE_SECS, PLAYER_JUMP_BUFFER_SECS},
    map::Carriers,
    physics::{
        CharacterMovePlan, CharacterPortalHop, CharacterSupport, CharacterVerticalVelocity, CollisionWorld,
        KnockbackVelocity, PortalSet, passable_fields,
    },
    protocol::{
        CMove, CarrierId, FaceYaw, FieldId, MapSettings, MoveOutcome, PlayerMoveIntent, PlayerMovementState,
        PlayerStance, Position,
    },
};

use super::{
    HorizontalVelocity, LocalMovementReports, LocalMovementStep, PlayerJump, PlayerMovementStep, collect_move_outcomes,
    plan_player_move, player_jump,
};
use crate::portals::{PlayerHopBody, player_hop};

// The jump the input asked for and what the tick makes of it. A press
// survives frames without fixed steps and is kept for
// `PLAYER_JUMP_BUFFER_SECS`, so one just before a landing fires on the
// landing tick; a body that stood within `PLAYER_COYOTE_SECS` still jumps
// as if it did. Either way one press is at most one jump.
#[derive(Component, Debug, Clone, Copy)]
pub struct JumpRequest {
    pub pressed: bool,
    // Seconds the pending press has left before it lapses.
    buffered_secs: Option<f32>,
    // Seconds since the last step ended standing.
    since_ground_secs: f32,
}

impl Default for JumpRequest {
    fn default() -> Self {
        Self {
            pressed: false,
            buffered_secs: None,
            since_ground_secs: f32::INFINITY,
        }
    }
}

impl JumpRequest {
    fn take_press(&mut self) -> bool {
        if std::mem::take(&mut self.pressed) {
            self.buffered_secs = Some(PLAYER_JUMP_BUFFER_SECS);
        }
        self.buffered_secs.is_some()
    }

    // Spends the press, or lets it age; the buffer lapses before the next tick.
    fn settle(&mut self, spent: bool, delta: f32) {
        self.buffered_secs = match self.buffered_secs {
            Some(left) if !spent && left > delta => Some(left - delta),
            _ => None,
        };
    }

    fn stood(&mut self, support: CharacterSupport, delta: f32) {
        self.since_ground_secs = if support == CharacterSupport::Ground {
            0.0
        } else {
            self.since_ground_secs + delta
        };
    }
}

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
    pub jump: &'a mut JumpRequest,
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
    let last = body.step.result.support;
    // A body just off an edge jumps as if it were still on it.
    let coyote = last == CharacterSupport::Airborne
        && body.jump.since_ground_secs <= PLAYER_COYOTE_SECS
        && body.vertical_velocity.0 <= 0.0;
    // A crouched or stunned body drops the press; an airborne one keeps it
    // for the landing.
    let refused = body.stance.crouched || world.stunned;
    let jump = (body.jump.take_press() && !refused)
        .then(|| {
            if coyote {
                return Some(PlayerJump::Rise(settings.movement.player.jump_speed));
            }
            player_jump(
                last,
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
    body.jump.settle(jump.is_some() || refused, world.delta);
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
    body.jump.stood(result.support, world.delta);
    *body.step = LocalMovementStep {
        start: step.start,
        result,
        horizontal_velocity: step.horizontal_velocity,
        intent_velocity: step.intent_velocity,
        knockback_displacement: step.knockback_displacement,
        hits_character: planned.hits_character,
    };

    let hop = player_hop(
        world.portal_set,
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
        hop.apply(
            body.position,
            body.face_yaw,
            body.vertical_velocity,
            body.intent,
            body.stance,
            body.knockback,
            body.horizontal_velocity,
        );
        // Render interpolation anchors at the exit: the transit is a cut
        // there, not a smear between the portals.
        *body.previous_position = *body.position;
        body.reports.begin_crossing(entrance);
        // A press meant for the entrance side is not carried through.
        body.jump.settle(true, 0.0);
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
        hop: hop.map(|hop| hop.crossing),
        outcomes,
        report,
    }
}

#[cfg(test)]
#[path = "tests/owner.rs"]
mod tests;
