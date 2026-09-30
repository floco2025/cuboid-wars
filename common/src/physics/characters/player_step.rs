use super::{
    geometry::character_movement_shape,
    movement::step_character_movement,
    player_control::{accelerate_player, player_control_velocity, player_move_speed},
    support::rider_carry,
};
use crate::{
    config::GameplayConfig,
    constants::{CHARACTER_TERMINAL_VELOCITY, PLAYER_CROUCH_BLEND_SECS, PLAYER_CROUCH_SPEED_RATIO},
    map::Carriers,
    physics::{
        CharacterEnvironment, CharacterMovementResult, CharacterStep, CharacterSupport, CollisionWorld,
        HorizontalVelocity, LadderMode, PortalSet, grounding_diagnostics, passable_fields, portals::FunnelStep,
    },
    protocol::{FieldId, MapSettings, PlayerMoveIntent, PlayerStance, Position},
};
use bevy_math::Vec3;

#[derive(Clone, Copy)]
pub struct PlayerMovementStep<'a> {
    pub start: Position,
    pub vertical_velocity: f32,
    pub horizontal_velocity: Vec3,
    pub stance: PlayerStance,
    pub intent: PlayerMoveIntent,
    pub has_speed: bool,
    pub disabled: bool,
    pub delta: f32,
    pub has_low_gravity: bool,
    pub held_keys: &'a [FieldId],
    pub open_fields: &'a [FieldId],
    // Blast displacement only. Ordinary and portal velocity share horizontal_velocity.
    pub external_displacement: Vec3,
    pub collision_world: &'a CollisionWorld,
    pub map_settings: &'a MapSettings,
    pub gameplay_config: &'a GameplayConfig,
    pub portal_set: &'a PortalSet,
    pub carriers: &'a Carriers,
}

pub struct PlayerStepResult {
    pub start: Position,
    pub movement: CharacterMovementResult,
    pub horizontal_velocity: Vec3,
    pub stance: PlayerStance,
    pub control_velocity: Vec3,
    // The blast displacement the step applied; a blocked retry keeps only its vertical part.
    pub external_displacement: Vec3,
}

// The same player policy runs for the rendered owner and the headless owner.
// Character movement remains reusable by actors, which do not use this controller.
pub fn step_player_movement(step: PlayerMovementStep<'_>) -> PlayerStepResult {
    step_player(step, true)
}

// A body-blocked owner retries with vertical travel only: gravity, vertical
// blast displacement, carrier riding, and stance, with neither locomotion nor
// funnel capture and no horizontal velocity kept, so support and landing
// outcomes describe the accepted position.
pub fn step_player_movement_blocked(step: PlayerMovementStep<'_>) -> PlayerStepResult {
    let mut result = step_player(
        PlayerMovementStep {
            horizontal_velocity: Vec3::ZERO,
            external_displacement: step.external_displacement * Vec3::Y,
            ..step
        },
        false,
    );
    result.horizontal_velocity = Vec3::ZERO;
    result
}

fn step_player(step: PlayerMovementStep<'_>, locomotion: bool) -> PlayerStepResult {
    let passable = passable_fields(step.held_keys, step.open_fields);
    let body = &step.gameplay_config.player;
    let mut stance = step.stance;
    let old_physics = stance.physics(body);
    let cfg = &step.map_settings.movement;
    let gravity = step.map_settings.gravity_for(step.has_low_gravity);
    let mut control = if locomotion {
        player_control_velocity(step.intent, cfg, step.has_speed, step.disabled)
    } else {
        Vec3::ZERO
    };
    let environment = CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: step.collision_world,
        gravity: gravity * 0.5,
        passable_fields: &passable,
        physics: old_physics,
        portals: Some(step.portal_set),
        carriers: step.carriers,
    };
    let carry = rider_carry(
        &CharacterStep {
            start: step.start,
            vertical_velocity: step.vertical_velocity,
            control_velocity: control,
            external_displacement: step.external_displacement,
            delta: step.delta,
        },
        &environment,
        &character_movement_shape(old_physics),
    );
    // Carrier colliders have already moved. Probe where their rider is carried,
    // not inside the lift at its previous pose.
    let support_position = Position::from(Vec3::from(step.start) + carry.displacement);

    let grounded = step.vertical_velocity <= 0.0
        && grounding_diagnostics(
            step.collision_world,
            &support_position,
            old_physics,
            &passable,
            &step.portal_set.collision_exclusions(step.start.into(), old_physics),
        )
        .supported;
    // A body in a ladder's front volume moves at the ladder speed under direct
    // control, like the climb itself: the ladder rules see the whole intent,
    // there is no momentum to build or brake, and a dismount clears the
    // volume before the ladder can catch the body again.
    let on_ladder = !grounded
        && step
            .collision_world
            .ladder_volume_at(&Position {
                y: step.start.y,
                ..support_position
            })
            .is_some();
    if on_ladder {
        control *= cfg.player.move_speed_ladder;
    }
    let crouch = step.intent.crouch && !step.disabled;
    let mut start = step.start;
    if crouch != stance.crouched {
        let next_stance = PlayerStance {
            crouched: crouch,
            ..stance
        };
        let next_physics = next_stance.physics(body);
        let mut candidate = start;
        // Air ducking changes the hull about its centre; grounded ducking keeps feet planted.
        if !grounded {
            candidate.y += (old_physics.movement_collider.height - next_physics.movement_collider.height) * 0.5;
        }
        if crouch
            || !step.collision_world.character_penetrates_solid(
                &Position::from(Vec3::from(candidate) + carry.displacement),
                next_physics,
                &passable,
            )
        {
            stance = next_stance;
            start = candidate;
        }
    }
    let target = if stance.crouched { 1.0 } else { 0.0 };
    stance.fraction = if grounded {
        let blend_step = step.delta / PLAYER_CROUCH_BLEND_SECS;
        stance.fraction + (target - stance.fraction).clamp(-blend_step, blend_step)
    } else {
        target
    };
    let physics = stance.physics(body);
    // A held climber is not flying: its wish matches its control exactly, so
    // nothing but a shove reaches the motor as external displacement.
    let mut wish = if step.disabled || !locomotion {
        Vec3::ZERO
    } else {
        step.intent
            .wish_velocity(player_move_speed(&cfg.player, step.has_speed), !grounded && !on_ladder)
    };
    if grounded && stance.crouched {
        wish *= PLAYER_CROUCH_SPEED_RATIO;
    }
    if on_ladder {
        wish *= cfg.player.move_speed_ladder;
    }
    let blast = step.external_displacement / step.delta;
    let mut velocity = if on_ladder {
        wish
    } else {
        accelerate_player(
            step.horizontal_velocity,
            blast,
            wish,
            step.vertical_velocity,
            grounded,
            &cfg.player,
            step.delta,
        )
    };
    let steering = step.intent.forward != 0.0 || step.intent.sideways != 0.0;
    let funnel = if locomotion && !grounded && !on_ladder && !step.disabled && !steering {
        step.portal_set.funnel_correction(FunnelStep {
            origin: start.into(),
            physics,
            velocity: (velocity + blast).with_y(step.vertical_velocity),
            gravity,
            delta: step.delta,
            config: step.gameplay_config.portals.funnel,
            world: step.collision_world,
            passable_fields: &passable,
        })
    } else {
        None
    }
    .unwrap_or_default();
    // Intent, not momentum, decides whether a nearby ladder is being mounted.
    if grounded && stance.crouched {
        control *= PLAYER_CROUCH_SPEED_RATIO;
    }
    let mut movement = step_character_movement(
        CharacterStep {
            start,
            vertical_velocity: step.vertical_velocity,
            control_velocity: control,
            external_displacement: (velocity - control) * step.delta + step.external_displacement + funnel,
            delta: step.delta,
        },
        &CharacterEnvironment {
            // The generic motor applies half gravity before movement. Finish below.
            physics,
            ..environment
        },
    );
    if movement.support == CharacterSupport::Airborne {
        movement.vertical_velocity =
            (movement.vertical_velocity - gravity * step.delta * 0.5).max(-CHARACTER_TERMINAL_VELOCITY);
    }
    let mut horizontal = HorizontalVelocity(velocity);
    horizontal.finish_step(&movement);
    velocity = horizontal.0;
    PlayerStepResult {
        start,
        movement,
        horizontal_velocity: velocity,
        stance,
        control_velocity: control,
        external_displacement: step.external_displacement,
    }
}
