use bevy::prelude::*;
use common::{
    config::{NetworkConfig, UpdateCadence},
    map::Carriers,
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{CMove, CarrierId, ClientMessage, FaceYaw, PlayerGeneration, PlayerMoveIntent, Position},
};

use super::{outcomes::LocalMovementStep, player_movement_state};
use crate::{
    network::ClientToServerChannel,
    players::{LocalPlayerInfo, LocalPlayerMarker},
};

#[derive(Default)]
pub struct LocalMovementReports {
    pub(super) generation: PlayerGeneration,
    seq: u32,
    portal_crossing: u32,
    last_carrier: Option<CarrierId>,
    pub(super) crossing_entrance: Option<Position>,
    pub(super) void_reported: bool,
}

impl LocalMovementReports {
    pub fn begin_crossing(&mut self, entrance: Position) {
        self.crossing_entrance = Some(entrance);
        self.portal_crossing = self.portal_crossing.wrapping_add(1);
    }

    pub fn clear_crossings(&mut self) {
        self.crossing_entrance = None;
    }

    pub fn begin_body(&mut self, generation: PlayerGeneration) {
        self.generation = generation;
        self.void_reported = false;
        self.portal_crossing = 0;
        self.last_carrier = None;
        self.clear_crossings();
    }

    // A new body reports at once through the carrier change, so the cadence keeps its phase across bodies.
    fn report_due(&mut self, cadence: &mut UpdateCadence, carrier: CarrierId) -> bool {
        // Observers time samples by simulation steps, including steps with no report.
        self.seq = self.seq.wrapping_add(1);
        let periodic = cadence.ready();
        let changed_carrier = self.last_carrier != Some(carrier);
        self.last_carrier = Some(carrier);
        self.crossing_entrance.take().is_some() || changed_carrier || periodic
    }
}

pub fn report_player_movement_system(
    to_server: Res<ClientToServerChannel>,
    network: Res<NetworkConfig>,
    mut cadence: Local<Option<UpdateCadence>>,
    carriers: Res<Carriers>,
    mut local: ResMut<LocalPlayerInfo>,
    query: Query<
        (
            &Position,
            &PlayerMoveIntent,
            &FaceYaw,
            &CharacterVerticalVelocity,
            &AirborneMomentum,
            &KnockbackVelocity,
            &LocalMovementStep,
        ),
        With<LocalPlayerMarker>,
    >,
) {
    if local.is_dead {
        return;
    }
    let Ok((pos, intent, yaw, vertical, momentum, knockback, step)) = query.single() else {
        return;
    };
    let reports = &mut local.reports;
    // A crossing lands in world space; otherwise the frame is the one the
    // motor actually rode.
    let carrier = if reports.crossing_entrance.is_some() {
        CarrierId::WORLD
    } else {
        step.carrier
    };
    if !reports.report_due(cadence.get_or_insert_with(|| network.update_cadence()), carrier) {
        return;
    }
    let mut movement = player_movement_state(*pos, *intent, yaw, vertical, momentum, knockback, step.support);
    movement.carrier = carrier;
    movement.pos = carriers.pose(carrier).inverse_transform_position(pos);
    to_server.send(ClientMessage::Move(CMove {
        generation: reports.generation,
        seq: reports.seq,
        portal_crossing: reports.portal_crossing,
        movement,
    }));
}

#[cfg(test)]
#[path = "tests/reports.rs"]
mod tests;
