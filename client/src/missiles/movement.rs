use bevy::prelude::*;

use crate::{
    characters::PreviousTickPosition,
    constants::{RECON_CORRECTION_TIME_RTT_MULTIPLIER, RECON_MISSILE_SNAP_DISTANCE},
    missiles::MissileVelocity,
    network::{ServerReconciliation, worst_axis_divergence},
};
use common::{
    constants::SNAPSHOT_SECS,
    protocol::{MissileId, MissileMarker, Position},
};

// Same RTT-scaled window as the actor pipeline, duplicated deliberately —
// the player/actor/missile reconciliation copies are kept separate.
fn missile_correction_factor(rtt: f32) -> f32 {
    (SNAPSHOT_SECS / (rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER)).clamp(0.0, 1.0)
}

// Dead-reckon the last server velocity on all three axes (missiles fly; no
// gravity, no local collision — the server owns detonation) plus the usual
// additive reconciliation bleed. Runs in `FixedUpdate` for 30 Hz parity with
// the server's integration. Captures its own `PreviousTickPosition` — the
// shared capture system only covers characters.
pub fn missiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<
        (
            Entity,
            &MissileId,
            &mut Position,
            &mut PreviousTickPosition,
            &mut MissileVelocity,
            Option<&mut ServerReconciliation>,
        ),
        With<MissileMarker>,
    >,
) {
    let delta = time.delta_secs();
    for (entity, missile_id, mut pos, mut prev, mut velocity, mut recon_option) in &mut query {
        prev.0 = *pos;

        let correction = if let Some(recon) = recon_option.as_mut() {
            let correction_factor = missile_correction_factor(recon.rtt);
            recon.correction_progress += delta * correction_factor;
            if recon.correction_progress >= SNAPSHOT_SECS {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let correction_delta = recon.extrapolated_delta();
            let (worst_axis, worst_magnitude) = worst_axis_divergence(correction_delta);
            if worst_magnitude >= RECON_MISSILE_SNAP_DISTANCE {
                warn!(
                    "missile#{} out of sync: |{worst_axis}|={worst_magnitude:.2} >= {:.2}; snapping to server position",
                    missile_id.0, RECON_MISSILE_SNAP_DISTANCE
                );
                *pos = recon.server_pos;
                velocity.0 = recon.server_velocity;
                commands.entity(entity).remove::<ServerReconciliation>();
                prev.0 = *pos;
                continue;
            }

            correction_delta * delta * correction_factor / SNAPSHOT_SECS
        } else {
            Vec3::ZERO
        };

        *pos += velocity.0 * delta + correction;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_factor_follows_the_rtt_window() {
        let rtt = 0.2;
        let expected = SNAPSHOT_SECS / (rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER);
        assert!((missile_correction_factor(rtt) - expected).abs() < 1e-6);
    }

    #[test]
    fn zero_rtt_saturates_the_correction_factor() {
        assert_eq!(missile_correction_factor(0.0), 1.0);
    }
}
