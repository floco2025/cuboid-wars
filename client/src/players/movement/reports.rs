use common::{
    config::{NetworkConfig, UpdateCadence},
    map::Carriers,
    protocol::{CMove, CarrierId, PlayerGeneration, PlayerMovementState, Position},
};

// What the owner has told the server about its body: the sequence its
// reports run on, the crossing it is in, and the cadences its periodic
// reports keep.
#[derive(Default)]
pub struct LocalMovementReports {
    pub generation: PlayerGeneration,
    seq: u32,
    portal_crossing: u32,
    last_carrier: Option<CarrierId>,
    pub(super) crossing_entrance: Option<Position>,
    pub(super) void_reported: bool,
    cadence: Option<UpdateCadence>,
    eraser_cadence: Option<UpdateCadence>,
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

    pub fn movement_report(
        &mut self,
        network: &NetworkConfig,
        mut movement: PlayerMovementState,
        motor_carrier: CarrierId,
        carriers: &Carriers,
    ) -> Option<CMove> {
        // A crossing lands in world space; otherwise use the frame the motor rode.
        let carrier = if self.crossing_entrance.is_some() {
            CarrierId::WORLD
        } else {
            motor_carrier
        };
        if !self.report_due(network, carrier) {
            return None;
        }
        movement.carrier = carrier;
        movement.pos = carriers.pose(carrier).inverse_transform_position(&movement.pos);
        Some(CMove {
            generation: self.generation,
            seq: self.seq,
            portal_crossing: self.portal_crossing,
            movement,
        })
    }

    // A new body reports at once through the carrier change, so the cadence keeps its phase across bodies.
    fn report_due(&mut self, network: &NetworkConfig, carrier: CarrierId) -> bool {
        // Observers time samples by simulation steps, including steps with no report.
        self.seq = self.seq.wrapping_add(1);
        let periodic = self.cadence.get_or_insert_with(|| network.update_cadence()).ready();
        let changed_carrier = self.last_carrier != Some(carrier);
        self.last_carrier = Some(carrier);
        self.crossing_entrance.take().is_some() || changed_carrier || periodic
    }

    // Whether eraser contact is reported this tick: at once on entry, then at
    // the movement cadence while it lasts.
    pub(super) fn erase_due(&mut self, contact: bool, network: &NetworkConfig) -> bool {
        if !contact {
            self.eraser_cadence = None;
            return false;
        }
        self.eraser_cadence
            .get_or_insert_with(|| network.update_cadence())
            .ready()
    }
}

#[cfg(test)]
#[path = "tests/reports.rs"]
mod tests;
