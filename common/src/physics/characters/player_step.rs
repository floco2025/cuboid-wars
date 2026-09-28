use super::{
    geometry::character_movement_shape,
    movement::step_character_movement,
    player_control::{accelerate_player, player_control_velocity},
    support::rider_carry,
};
use crate::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        CharacterEnvironment, CharacterMovementResult, CharacterStep, CharacterSupport, CollisionWorld,
        HorizontalVelocity, LadderMode, PortalSet, grounding_diagnostics, passable_fields,
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
}

// The same player policy runs for the rendered owner and the headless owner.
// Character movement remains reusable by actors, which do not use this controller.
pub fn step_player_movement(step: PlayerMovementStep<'_>) -> PlayerStepResult {
    let passable = passable_fields(step.held_keys, step.open_fields);
    let body = &step.gameplay_config.player;
    let mut stance = step.stance;
    let old_physics = stance.physics(body);
    let cfg = &step.map_settings.movement;
    let gravity = step.map_settings.gravity_for(step.has_low_gravity);
    let mut control = player_control_velocity(step.intent, cfg, step.has_speed, step.disabled);
    let environment = CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: step.collision_world,
        gravity: gravity * 0.5,
        passable_fields: &passable,
        physics: old_physics,
        ladder_climb_ratio: cfg.ladder_climb_ratio,
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
    let crouch = step.intent.crouch && !step.disabled;
    let mut start = step.start;
    if crouch != stance.crouched {
        let next_stance = crate::protocol::PlayerStance {
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
        stance.fraction + (target - stance.fraction).clamp(-step.delta / 0.2, step.delta / 0.2)
    } else {
        target
    };
    let physics = stance.physics(body);
    let speed = cfg.player.move_speed
        * if step.has_speed {
            cfg.player.move_speed_power_up
        } else {
            1.0
        };
    let mut wish = if step.disabled {
        Vec3::ZERO
    } else {
        step.intent.wish_velocity(speed, !grounded)
    };
    if grounded && stance.crouched {
        wish /= 3.0;
    }
    let blast = step.external_displacement / step.delta;
    let mut velocity = accelerate_player(
        step.horizontal_velocity,
        blast,
        wish,
        step.vertical_velocity,
        grounded,
        &cfg.player,
        step.delta,
    );
    let funnel = if !grounded && !step.disabled && step.intent.forward == 0.0 && step.intent.sideways == 0.0 {
        step.portal_set.funnel_correction(crate::physics::portals::FunnelStep {
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
        Default::default()
    };
    // Intent, not momentum, decides whether a nearby ladder is being mounted.
    if grounded && stance.crouched {
        control /= 3.0;
    }
    let mut movement = step_character_movement(
        CharacterStep {
            start,
            vertical_velocity: step.vertical_velocity,
            control_velocity: control,
            external_displacement: (velocity - control) * step.delta + step.external_displacement + funnel.displacement,
            delta: step.delta,
        },
        &CharacterEnvironment {
            // The generic motor applies half gravity before movement. Finish below.
            physics,
            ..environment
        },
    );
    velocity += funnel.velocity_change;
    if movement.support == CharacterSupport::Airborne {
        movement.vertical_velocity = (movement.vertical_velocity - gravity * step.delta * 0.5)
            .max(-crate::constants::CHARACTER_TERMINAL_VELOCITY);
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
    }
}
