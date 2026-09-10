use bevy::prelude::*;
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{MovementDivergence, Position},
};

use crate::{
    characters::PreviousTickPosition, constants::RECON_CHARACTER_SNAP_DISTANCE, network::ServerReconciliation,
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
    let divergence = MovementDivergence {
        delta: recon.correction_delta,
        limit: RECON_CHARACTER_SNAP_DISTANCE,
    };
    if !divergence.within_limit() {
        warn!("{character_name}#{character_id} out of sync, {divergence}; snapping to server position");
        *pos = recon.server_pos;
        // Adopt server vy only; horizontal motion comes from `move_intent`.
        vertical_velocity.0 = recon.server_velocity.y;
        commands
            .entity(entity)
            .remove::<ServerReconciliation>()
            // Keep render interpolation from smearing the snap across one frame.
            .insert(PreviousTickPosition(*pos));
        return CharacterReconciliationOutcome::Snapped;
    }

    let fraction = recon.correction_fraction(delta);
    if recon.applied_fraction >= 1.0 {
        commands.entity(entity).remove::<ServerReconciliation>();
    }
    CharacterReconciliationOutcome::Displacement(Vec3::new(
        recon.correction_delta.x * fraction,
        0.0,
        recon.correction_delta.z * fraction,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::RoundTripTime;
    use bevy::ecs::system::SystemState;
    use std::time::Duration;

    #[test]
    fn drift_is_bled_off_horizontally_and_a_large_gap_snaps_with_the_server_fall() {
        let rtt = RoundTripTime {
            rtt: Duration::from_millis(200),
            ..default()
        };
        for (error, snaps) in [(1.0, false), (RECON_CHARACTER_SNAP_DISTANCE, true)] {
            let mut world = World::new();
            let server_pos = Position {
                x: error,
                y: 0.0,
                z: 0.0,
            };
            let mut recon = ServerReconciliation::new(Vec3::X * error, server_pos, Vec3::NEG_Y * 2.0, &rtt);
            let entity = world.spawn(PreviousTickPosition(Position::default())).id();
            let mut pos = Position::default();
            let mut vertical = CharacterVerticalVelocity(0.0);
            let mut state = SystemState::<Commands>::new(&mut world);
            let outcome = reconcile_character(
                &mut state.get_mut(&mut world).expect("commands unavailable"),
                entity,
                7,
                "actor",
                &mut pos,
                &mut vertical,
                &mut recon,
                1.0 / 30.0,
            );
            state.apply(&mut world);
            match outcome {
                CharacterReconciliationOutcome::Displacement(displacement) => {
                    assert!(!snaps);
                    assert!((displacement.x - error * (1.0 / 30.0) / 0.8).abs() < 1e-5);
                    assert_eq!(displacement.y, 0.0);
                    assert_eq!(pos, Position::default());
                }
                CharacterReconciliationOutcome::Snapped => {
                    assert!(snaps);
                    assert_eq!(pos, server_pos);
                    assert_eq!(vertical.0, -2.0);
                    assert_eq!(
                        world
                            .get::<PreviousTickPosition>(entity)
                            .expect("previous position missing")
                            .0,
                        server_pos
                    );
                }
            }
        }
    }
}
