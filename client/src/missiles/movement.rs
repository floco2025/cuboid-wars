use bevy::prelude::*;

use crate::{
    characters::PreviousTickPosition, constants::RECON_MISSILE_SNAP_DISTANCE, missiles::MissileVelocity,
    network::ServerReconciliation,
};
use common::protocol::{MissileId, MissileMarker, MovementDivergence, Position};

// Dead-reckon the last server velocity on all three axes (missiles fly; no
// gravity, no local collision — the server owns detonation) plus the shared
// reconciliation bleed, applied on all three axes too. Runs in `FixedUpdate`
// for 30 Hz parity with the server's integration. Captures its own
// `PreviousTickPosition` — the shared capture system only covers characters.
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
            let divergence = MovementDivergence {
                delta: recon.correction_delta,
                limit: RECON_MISSILE_SNAP_DISTANCE,
            };
            if !divergence.within_limit() {
                warn!(
                    "missile#{} out of sync, {divergence}; snapping to server position",
                    missile_id.0
                );
                *pos = recon.server_pos;
                velocity.0 = recon.server_velocity;
                commands.entity(entity).remove::<ServerReconciliation>();
                prev.0 = *pos;
                continue;
            }
            let fraction = recon.correction_fraction(delta);
            if recon.applied_fraction >= 1.0 {
                commands.entity(entity).remove::<ServerReconciliation>();
            }
            recon.correction_delta * fraction
        } else {
            Vec3::ZERO
        };

        *pos += velocity.0 * delta + correction;
    }
}
