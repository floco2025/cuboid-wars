use bevy_math::Vec3;

use crate::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{ActorMoveIntent, BarrierKindId, MapSettings, Position},
};

use super::{
    ladder::LadderMode,
    movement::{CharacterEnvironment, CharacterStep, step_character_movement},
    types::CharacterMovementResult,
};

pub struct ActorMovementStep<'a> {
    pub start: Position,
    pub vertical_velocity: f32,
    pub intent: ActorMoveIntent,
    pub external_displacement: Vec3,
    pub delta: f32,
    pub can_use_ladders: bool,
    pub physics: CharacterPhysicsConfig,
    // Barrier kinds the pressure plates hold open (`PlateState`); actors
    // hold no keys.
    pub open_kinds: &'a [BarrierKindId],
    pub collision_world: &'a CollisionWorld,
    pub map_settings: &'a MapSettings,
    pub carriers: &'a Carriers,
}

#[must_use]
pub fn step_actor_movement(step: ActorMovementStep<'_>) -> CharacterMovementResult {
    step_character_movement(
        CharacterStep {
            start: step.start,
            vertical_velocity: step.vertical_velocity,
            control_velocity: step.intent.to_horizontal_velocity(),
            external_displacement: step.external_displacement,
            delta: step.delta,
        },
        &CharacterEnvironment {
            ladder_mode: LadderMode::for_actor(step.can_use_ladders, step.intent),
            collision_world: step.collision_world,
            gravity: step.map_settings.movement.gravity,
            passable_kinds: step.open_kinds,
            physics: step.physics,
            ladder_climb_ratio: step.map_settings.movement.ladder_climb_ratio,
            portals: None,
            carriers: step.carriers,
        },
    )
}
