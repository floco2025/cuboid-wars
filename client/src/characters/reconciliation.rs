use bevy::prelude::*;
use common::{physics::CharacterVerticalVelocity, protocol::Position};

use crate::{
    characters::PreviousTickPosition,
    constants::RECON_CHARACTER_SNAP_DISTANCE,
    network::{ServerReconciliation, worst_axis_divergence},
};

pub(crate) enum CharacterReconciliationOutcome {
    Displacement(Vec3),
    Snapped,
}

pub(crate) fn reconcile_character(
    commands: &mut Commands,
    entity: Entity,
    character_id: u32,
    character_name: &str,
    pos: &mut Position,
    vertical_velocity: &mut CharacterVerticalVelocity,
    recon: &mut ServerReconciliation,
    delta: f32,
) -> CharacterReconciliationOutcome {
    let fraction = recon.correction_fraction(delta);
    if recon.applied_fraction >= 1.0 {
        commands.entity(entity).remove::<ServerReconciliation>();
    }

    let correction_delta = recon.correction_delta;
    let (worst_axis, worst_magnitude) = worst_axis_divergence(correction_delta);
    if worst_magnitude >= RECON_CHARACTER_SNAP_DISTANCE {
        warn!(
            "{character_name}#{character_id} out of sync: |{worst_axis}|={worst_magnitude:.2} >= {:.2} (Δ x={:.2}, y={:.2}, z={:.2}); snapping to server position",
            RECON_CHARACTER_SNAP_DISTANCE, correction_delta.x, correction_delta.y, correction_delta.z
        );
        *pos = recon.server_pos;
        // Adopt server vy only; horizontal motion comes from `move_intent`.
        vertical_velocity.0 = recon.server_velocity.y;
        commands.entity(entity).remove::<ServerReconciliation>();
        // Keep render interpolation from smearing the snap across one frame.
        commands.entity(entity).insert(PreviousTickPosition(*pos));
        return CharacterReconciliationOutcome::Snapped;
    }

    CharacterReconciliationOutcome::Displacement(Vec3::new(
        correction_delta.x * fraction,
        0.0,
        correction_delta.z * fraction,
    ))
}
