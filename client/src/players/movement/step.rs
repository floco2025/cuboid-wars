use super::{
    movement::{CharacterStart, step_character_movement_from},
    player_control::{PlayerWish, accelerate_player, player_wish_velocity},
};
use crate::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        CharacterEnvironment, CharacterMovementResult, CharacterStep, CollisionWorld, HorizontalVelocity, LadderMode,
        PortalSet, passable_fields, portals::FunnelStep,
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
    // The blast shove this tick. Ordinary and portal velocity share horizontal_velocity.
    pub knockback_displacement: Vec3,
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
    // What the player asked for, which animation measures travel against.
    pub intent_velocity: Vec3,
    // The blast shove the step applied; a blocked retry keeps only its vertical part.
    pub knockback_displacement: Vec3,
}

// The same player policy runs for the rendered owner and the headless owner.
// Character movement remains reusable by actors, which do not use this controller.
pub fn step_player_movement(step: PlayerMovementStep<'_>) -> PlayerStepResult {
    step_player(step, false)
}

// A body-blocked owner retries with what the body it hit leaves it: with
// `along`, the unit direction from that body, the velocity into it is
// removed and the rest kept, as a wall contact does; without one, vertical
// travel alone. Neither locomotion nor the funnel act, so support and
// landing outcomes describe the accepted position.
pub fn step_player_movement_blocked(step: PlayerMovementStep<'_>, along: Option<Vec3>) -> PlayerStepResult {
    let keep = |velocity: Vec3| along.map_or(Vec3::ZERO, |normal| velocity - normal * velocity.dot(normal).min(0.0));
    let shove = step.knockback_displacement;
    let mut result = step_player(
        PlayerMovementStep {
            horizontal_velocity: keep(step.horizontal_velocity),
            knockback_displacement: keep(shove.with_y(0.0)) + shove * Vec3::Y,
            ..step
        },
        true,
    );
    result.horizontal_velocity = keep(result.horizontal_velocity);
    result
}

// `blocked` is the body-blocked retry: no locomotion and no funnel.
fn step_player(step: PlayerMovementStep<'_>, blocked: bool) -> PlayerStepResult {
    let passable = passable_fields(step.held_keys, step.open_fields);
    let body = &step.gameplay_config.player;
    let mut stance = step.stance;
    let cfg = &step.map_settings.movement;
    let gravity = step.map_settings.gravity_for(step.has_low_gravity);
    let blast = step.knockback_displacement / step.delta;
    let environment = |physics| CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: step.collision_world,
        gravity,
        passable_fields: &passable,
        physics,
        portals: Some(step.portal_set),
        carriers: step.carriers,
    };
    let probe = |start: Position, physics| {
        CharacterStart::probe(
            &CharacterStep {
                start,
                vertical_velocity: step.vertical_velocity,
                intent_velocity: Vec3::ZERO,
                velocity: Vec3::ZERO,
                displacement: Vec3::ZERO,
                delta: step.delta,
            },
            &environment(physics),
        )
    };
    let mut physics = stance.physics(body);
    let mut start = step.start;
    let mut probed = probe(start, physics);
    let crouch = step.intent.crouch && !step.disabled;
    if crouch != stance.crouched {
        let next_stance = PlayerStance { crouched: crouch };
        let next_physics = next_stance.physics(body);
        let mut candidate = start;
        // Air ducking changes the hull about its centre; grounded ducking keeps feet planted.
        if !probed.grounded {
            candidate.y += (physics.movement_collider.height - next_physics.movement_collider.height) * 0.5;
        }
        if crouch
            || !step.collision_world.character_penetrates_solid(
                &Position::from(Vec3::from(candidate) + probed.carry),
                next_physics,
                &passable,
            )
        {
            stance = next_stance;
            start = candidate;
            physics = next_physics;
            // The hull changed under the probe; the same rules judge the new one.
            probed = probe(start, physics);
        }
    }
    let grounded = probed.grounded;
    // A body in a ladder's front volume moves at the ladder speed under direct
    // control, like the climb itself: the ladder rules see the whole intent,
    // there is no momentum to build or brake, and a dismount clears the
    // volume before the ladder can catch the body again.
    let on_ladder = !grounded && probed.ladder.is_some();
    let wish = player_wish_velocity(
        step.intent,
        &cfg.player,
        PlayerWish {
            has_speed: step.has_speed,
            disabled: step.disabled || blocked,
            airborne: !grounded,
            crouched: stance.crouched,
            on_ladder,
        },
    );
    // A held climber is not flying: its wish is its velocity exactly, so
    // nothing but a shove reaches the motor as displacement.
    let velocity = if on_ladder {
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
    let funnel = if !blocked && !grounded && !on_ladder && !step.disabled && !steering {
        step.portal_set.funnel_correction(FunnelStep {
            origin: start.into(),
            physics,
            velocity: (velocity + blast).with_y(step.vertical_velocity),
            gravity,
            brake: cfg.player.air_deceleration,
            delta: step.delta,
            config: step.gameplay_config.portals.funnel,
            world: step.collision_world,
            passable_fields: &passable,
        })
    } else {
        None
    }
    .unwrap_or_default();
    let movement = step_character_movement_from(
        &probed,
        CharacterStep {
            start,
            vertical_velocity: step.vertical_velocity,
            intent_velocity: wish,
            velocity,
            displacement: step.knockback_displacement + funnel,
            delta: step.delta,
        },
        &environment(physics),
    );
    let mut horizontal = HorizontalVelocity(velocity);
    horizontal.finish_step(&movement);
    PlayerStepResult {
        start,
        movement,
        horizontal_velocity: horizontal.0,
        stance,
        intent_velocity: wish,
        knockback_displacement: step.knockback_displacement,
    }
}
