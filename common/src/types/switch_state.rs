use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use super::{CarrierId, FieldId, SwitchId};
use crate::map::CarrierRun;

// The active switches and what they drive: the barriers and light bridges
// that are off, switched or not, and each switched carrier's run. One value
// on both sides — the server's switch system writes it, `SInit` and every
// snapshot carry it, the collision filters read the open fields,
// `Carriers::advance` places switched carriers from their runs, and the
// actor spawner reads the active switches for its zones. Every list stays
// sorted so equality diffs are stable.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SwitchState {
    pub active_switches: Vec<SwitchId>,
    pub open_fields: Vec<FieldId>,
    pub carrier_runs: Vec<(CarrierId, CarrierRun)>,
}

impl SwitchState {
    #[must_use]
    pub fn is_active(&self, switch: SwitchId) -> bool {
        self.active_switches.binary_search(&switch).is_ok()
    }

    // A switched carrier's run; when absent, use `CarrierRun::initial`
    // so its initial state applies at startup.
    #[must_use]
    pub fn carrier_run(&self, carrier: CarrierId) -> Option<CarrierRun> {
        self.carrier_runs
            .binary_search_by_key(&carrier, |(id, _)| *id)
            .ok()
            .map(|index| self.carrier_runs[index].1)
    }

    pub fn sort(&mut self) {
        self.active_switches.sort_unstable();
        self.open_fields.sort_unstable();
        self.carrier_runs.sort_unstable_by_key(|(id, _)| *id);
    }
}

#[cfg(test)]
#[path = "tests/switch_state.rs"]
mod tests;
