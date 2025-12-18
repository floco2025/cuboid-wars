use bevy_ecs::prelude::*;

use crate::{collision::check_player_player_sweep, protocol::Position};

// ============================================================================
// Planned Move - Used in two-pass movement system
// ============================================================================

// Represents a player's intended movement after wall collision but before player collision
#[derive(Copy, Clone)]
pub struct PlannedMove {
    pub entity: Entity,
    pub start: Position,
    pub target: Position,
    pub hits_wall: bool,
}

// Check if a planned move would overlap with any other player's planned position
#[must_use]
pub fn overlaps_other_player(candidate: &PlannedMove, planned_moves: &[PlannedMove]) -> bool {
    planned_moves
        .iter()
        .any(|other| {
            other.entity != candidate.entity
                && check_player_player_sweep(&candidate.start, &candidate.target, &other.start, &other.target)
        })
}
