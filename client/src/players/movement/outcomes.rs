use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, NetworkConfig},
    constants::CHARACTER_FALL_DEATH_Y,
    map::Carriers,
    physics::{CharacterMovementResult, CollisionWorld},
    protocol::{MoveOutcome, Position},
};

use super::LocalMovementReports;

// What the owner's last tick decided, for everything that reports or shows it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct LocalMovementStep {
    pub start: Position,
    pub result: CharacterMovementResult,
    // The step's own horizontal velocity, before any crossing mapped it.
    pub horizontal_velocity: Vec3,
    // What the player asked for, which animation measures travel against.
    pub intent_velocity: Vec3,
    // The blast shove the step applied.
    pub knockback_displacement: Vec3,
    // Another body rejected the move and the retry stands in for it.
    pub hits_character: bool,
}

pub fn collect_move_outcomes(
    pos: &Position,
    step: &LocalMovementStep,
    physics: CharacterPhysicsConfig,
    collision: &CollisionWorld,
    carriers: &Carriers,
    reports: &mut LocalMovementReports,
    network: &NetworkConfig,
) -> Vec<MoveOutcome> {
    let mut outcomes = Vec::new();
    let crossing = reports.crossing_entrance;
    let sweep_end = crossing.unwrap_or(*pos);
    let touching = collision
        .character_eraser_contacts(pos, pos, physics, Some(carriers))
        .next()
        .is_some();
    let swept = collision
        .character_eraser_contacts(&step.start, &sweep_end, physics, Some(carriers))
        .next()
        .is_some();
    // A pickup update can arrive after contact, so the client inventory cannot
    // gate erasure: standing in a field keeps reporting, at the movement
    // cadence, and entry restarts that cadence so it is reported at once.
    if reports.erase_due(touching || swept, network) {
        outcomes.push(MoveOutcome::EraseEquipment);
    }
    if crossing.is_none() && step.result.impact_speed > 0.0 {
        outcomes.push(MoveOutcome::Landed {
            pos: *pos,
            impact_speed: step.result.impact_speed,
        });
    }
    if crossing.is_none() && step.result.crushed {
        outcomes.push(MoveOutcome::Crushed { pos: *pos });
    }
    if pos.y < CHARACTER_FALL_DEATH_Y && !reports.void_reported {
        reports.void_reported = true;
        outcomes.push(MoveOutcome::FellOutOfWorld);
    }
    outcomes
}

#[cfg(test)]
#[path = "tests/outcomes.rs"]
mod tests;
