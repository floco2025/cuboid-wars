use bevy::math::Vec3;

use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterEnvironment, CharacterMovementResult, CharacterStep, CollisionWorld,
        KnockbackVelocity, LadderMode, PortalSet, step_character_movement,
    },
    protocol::{BarrierId, BarrierKindId, MapSettings, Position},
};

pub(crate) struct PlayerMovementStep<'a> {
    pub start: Position,
    pub vertical_velocity: f32,
    pub control_velocity: Vec3,
    pub delta: f32,
    pub has_low_gravity: bool,
    pub held_keys: &'a [BarrierKindId],
    // Barrier kinds the pressure plates hold open (`PlateState`).
    pub open_kinds: &'a [BarrierId],
    pub knockback: &'a KnockbackVelocity,
    pub airborne_momentum: &'a mut AirborneMomentum,
    pub collision_world: &'a CollisionWorld,
    pub map_settings: &'a MapSettings,
    pub gameplay_config: &'a GameplayConfig,
    pub portal_set: &'a PortalSet,
    pub carriers: &'a Carriers,
}

#[must_use]
pub(crate) fn step_player_movement(step: PlayerMovementStep<'_>) -> CharacterMovementResult {
    let passable_kinds = step.collision_world.passable_barriers(step.held_keys, step.open_kinds);
    let external_displacement = momentum_displacement(Some(step.knockback), Some(&*step.airborne_momentum), step.delta);
    let movement = step_character_movement(
        CharacterStep {
            start: step.start,
            vertical_velocity: step.vertical_velocity,
            control_velocity: step.control_velocity,
            external_displacement,
            delta: step.delta,
        },
        &CharacterEnvironment {
            ladder_mode: LadderMode::Automatic,
            collision_world: step.collision_world,
            gravity: step.map_settings.gravity_for(step.has_low_gravity),
            passable_kinds: &passable_kinds,
            physics: step.gameplay_config.player.physics(),
            ladder_climb_ratio: step.map_settings.movement.ladder_climb_ratio,
            portals: Some(step.portal_set),
            carriers: step.carriers,
        },
    );
    step.airborne_momentum.finish_step(&movement);
    movement
}

#[must_use]
pub(crate) fn momentum_displacement(
    knockback: Option<&KnockbackVelocity>,
    momentum: Option<&AirborneMomentum>,
    delta: f32,
) -> Vec3 {
    knockback.map_or(Vec3::ZERO, |velocity| velocity.step(delta))
        + momentum.map_or(Vec3::ZERO, |momentum| momentum.step(delta))
}

#[cfg(test)]
#[path = "tests/step.rs"]
mod tests;
