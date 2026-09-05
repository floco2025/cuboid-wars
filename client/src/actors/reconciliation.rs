use bevy::prelude::*;
use common::{
    constants::SNAPSHOT_SECS,
    physics::CharacterVerticalVelocity,
    protocol::{ActorId, Position},
};

use crate::{
    characters::PreviousTickPosition,
    constants::{RECON_ACTOR_SNAP_DISTANCE, RECON_CORRECTION_TIME_RTT_MULTIPLIER},
    network::{ServerReconciliation, worst_axis_divergence},
};

pub(super) enum ActorReconciliationOutcome {
    Displacement(Vec3),
    Snapped,
}

pub(super) fn reconcile_actor(
    commands: &mut Commands,
    entity: Entity,
    actor_id: &ActorId,
    actor_kind: &str,
    pos: &mut Position,
    vertical_velocity: &mut CharacterVerticalVelocity,
    recon: &mut ServerReconciliation,
    delta: f32,
) -> ActorReconciliationOutcome {
    let correction_factor = actor_correction_factor(recon.rtt);

    // Each tick applies `delta / correction window` of the fixed delta, so
    // the accumulator reaching `SNAPSHOT_SECS` coincides with exactly 100%
    // of the correction applied — removing the component here is what stops
    // over-correction, doubling as the dropped-snapshot fallback (normally
    // the next snapshot replaces this component first).
    recon.correction_progress += delta * correction_factor;
    if recon.correction_progress >= SNAPSHOT_SECS {
        commands.entity(entity).remove::<ServerReconciliation>();
    }

    let correction_delta = recon.correction_delta;
    let (worst_axis, worst_magnitude) = worst_axis_divergence(correction_delta);
    if worst_magnitude >= RECON_ACTOR_SNAP_DISTANCE {
        warn!(
            "{actor_kind}#{} out of sync: |{worst_axis}|={worst_magnitude:.2} >= {:.2} (Δ x={:.2}, y={:.2}, z={:.2}); snapping to server position",
            actor_id.0, RECON_ACTOR_SNAP_DISTANCE, correction_delta.x, correction_delta.y, correction_delta.z
        );
        *pos = recon.server_pos;
        // Adopt server vy only; horizontal motion comes from `move_intent`.
        vertical_velocity.0 = recon.server_velocity.y;
        commands.entity(entity).remove::<ServerReconciliation>();
        // Keep render interpolation from smearing the snap across one frame.
        commands.entity(entity).insert(PreviousTickPosition(*pos));
        return ActorReconciliationOutcome::Snapped;
    }

    ActorReconciliationOutcome::Displacement(Vec3::new(
        correction_delta.x * delta * correction_factor / SNAPSHOT_SECS,
        0.0,
        correction_delta.z * delta * correction_factor / SNAPSHOT_SECS,
    ))
}

// Fraction of the correction delta applied per `SNAPSHOT_SECS` of real time
// — `SNAPSHOT_SECS / correction window`, with the window scaled from the
// RTT. A near-zero RTT saturates to 1.0 via the clamp. Unlike players,
// actors get no motion-aware window: their speeds are simple enough that
// one RTT-scaled window fits.
fn actor_correction_factor(rtt: f32) -> f32 {
    (SNAPSHOT_SECS / (rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_factor_follows_the_rtt_window() {
        let rtt = 0.2;
        let expected = SNAPSHOT_SECS / (rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER);
        assert!((actor_correction_factor(rtt) - expected).abs() < 1e-6);
    }

    #[test]
    fn zero_rtt_saturates_the_correction_factor() {
        assert_eq!(actor_correction_factor(0.0), 1.0);
    }
}
