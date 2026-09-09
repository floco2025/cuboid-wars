use bevy::prelude::*;
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{ActorId, Position},
};

use crate::{
    characters::PreviousTickPosition,
    constants::{RECON_ACTOR_SNAP_DISTANCE, RECON_CORRECTION_MIN_SECS, RECON_CORRECTION_TIME_RTT_MULTIPLIER},
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
    let window = actor_correction_window(recon.rtt);

    // Snapshots replace this component, so in steady state this is an
    // exponential pull toward a moving target; when the stream pauses the
    // last correction finishes linearly, capped so it cannot overshoot.
    let fraction = (delta / window).min(1.0 - recon.applied_fraction);
    recon.applied_fraction += fraction;
    if recon.applied_fraction >= 1.0 {
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
        correction_delta.x * fraction,
        0.0,
        correction_delta.z * fraction,
    ))
}

// The correction window scales with the RTT and never drops below
// `RECON_CORRECTION_MIN_SECS`. Unlike players, actors get no motion-aware
// window: their speeds are simple enough that one RTT-scaled window fits.
fn actor_correction_window(rtt: f32) -> f32 {
    (rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER).max(RECON_CORRECTION_MIN_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_window_scales_with_the_rtt() {
        let rtt = 0.2;
        let expected = rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER;
        assert!((actor_correction_window(rtt) - expected).abs() < 1e-6);
    }

    #[test]
    fn zero_rtt_keeps_the_minimum_window() {
        assert_eq!(actor_correction_window(0.0), RECON_CORRECTION_MIN_SECS);
    }
}
