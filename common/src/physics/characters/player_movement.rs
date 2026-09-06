use bevy_math::Vec3;

use crate::{
    config::GameplayConfig,
    map::Carriers,
    physics::{CollisionWorld, PortalSet, passable_barrier_kinds},
    protocol::{BarrierKindId, MapSettings, Position},
};

use super::{
    movement::{CharacterEnvironment, CharacterStep, step_character_movement},
    types::{AirborneMomentum, CharacterMovementResult, KnockbackVelocity, momentum_displacement},
};

pub struct PlayerMovementStep<'a> {
    pub start: Position,
    pub vertical_velocity: f32,
    pub control_velocity: Vec3,
    pub additional_displacement: Vec3,
    pub delta: f32,
    pub has_low_gravity: bool,
    pub held_keys: &'a [BarrierKindId],
    // Barrier kinds the pressure plates hold open (`PlateState`).
    pub open_kinds: &'a [BarrierKindId],
    pub knockback: Option<&'a KnockbackVelocity>,
    pub airborne_momentum: Option<&'a mut AirborneMomentum>,
    pub collision_world: &'a CollisionWorld,
    pub map_settings: &'a MapSettings,
    pub gameplay_config: &'a GameplayConfig,
    pub portal_set: &'a PortalSet,
    pub carriers: &'a Carriers,
}

#[must_use]
pub fn step_player_movement(mut step: PlayerMovementStep<'_>) -> CharacterMovementResult {
    let passable_kinds = passable_barrier_kinds(step.held_keys, step.open_kinds);
    let external_displacement = step.additional_displacement
        + momentum_displacement(step.knockback, step.airborne_momentum.as_deref(), step.delta);
    let movement = step_character_movement(
        CharacterStep {
            start: step.start,
            vertical_velocity: step.vertical_velocity,
            control_velocity: step.control_velocity,
            external_displacement,
            delta: step.delta,
        },
        &CharacterEnvironment {
            collision_world: step.collision_world,
            gravity: step.map_settings.gravity_for(step.has_low_gravity),
            passable_kinds: &passable_kinds,
            physics: step.gameplay_config.player.physics(),
            ladder_climb_ratio: step.map_settings.movement.ladder_climb_ratio,
            portals: Some(step.portal_set),
            carriers: step.carriers,
        },
    );
    if let Some(momentum) = step.airborne_momentum.as_mut() {
        momentum.finish_step(&movement);
    }
    movement
}
