use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use super::{BarrierId, BridgeId, CarrierId, SwitchId};
use crate::map::CarrierRun;

// What the pressure plates currently hold: the active switches and what
// they drive — the barriers open (passable and invisible), the bridges
// powered (solid and lit), and each switched carrier's run. One value on
// both sides — the server's plate system writes it, `SInit` and every
// snapshot carry it, the collision filters read the open barriers,
// `powered_bridges_sync_system` applies the powered bridges to their
// colliders, `Carriers::advance` places switched carriers from their runs,
// and the actor spawner reads the active switches for its zones. Every list
// stays sorted so equality diffs are stable.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PlateState {
    pub active_switches: Vec<SwitchId>,
    pub open_barriers: Vec<BarrierId>,
    pub powered_bridges: Vec<BridgeId>,
    pub carrier_runs: Vec<(CarrierId, CarrierRun)>,
}

impl PlateState {
    #[must_use]
    pub fn is_active(&self, switch: SwitchId) -> bool {
        self.active_switches.binary_search(&switch).is_ok()
    }

    // A switched carrier's run; `None` for a free carrier or one whose
    // switch has never been active, which rests at run 0.
    #[must_use]
    pub fn carrier_run(&self, carrier: CarrierId) -> Option<CarrierRun> {
        self.carrier_runs
            .binary_search_by_key(&carrier, |(id, _)| *id)
            .ok()
            .map(|index| self.carrier_runs[index].1)
    }

    pub fn sort(&mut self) {
        self.active_switches.sort_unstable();
        self.open_barriers.sort_unstable();
        self.powered_bridges.sort_unstable();
        self.carrier_runs.sort_unstable_by_key(|(id, _)| *id);
    }
}

#[cfg(test)]
#[path = "tests/plates.rs"]
mod tests;
