use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use super::{BarrierKindId, BridgeKindId, PlatePurpose};

// What the pressure plates currently hold: barrier kinds open (passable and
// invisible) and bridge kinds powered (solid and lit). One value on both
// sides — the server's plate system writes it, every snapshot carries it,
// and both sides feed it to the collision filters. Both lists stay sorted
// so equality diffs are stable.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PlateState {
    pub open_barrier_kinds: Vec<BarrierKindId>,
    pub powered_bridge_kinds: Vec<BridgeKindId>,
}

impl PlateState {
    // Every held purpose, sorted; the plate system diffs consecutive ticks
    // on this.
    #[must_use]
    pub fn purposes(&self) -> Vec<PlatePurpose> {
        let mut purposes: Vec<PlatePurpose> = self
            .open_barrier_kinds
            .iter()
            .map(|kind| PlatePurpose::Barrier(*kind))
            .chain(self.powered_bridge_kinds.iter().map(|kind| PlatePurpose::Bridge(*kind)))
            .collect();
        purposes.sort();
        purposes
    }

    // Fireworks are momentary, so they never hold.
    pub fn from_purposes(purposes: impl IntoIterator<Item = PlatePurpose>) -> Self {
        let mut state = Self::default();
        for purpose in purposes {
            match purpose {
                PlatePurpose::Barrier(kind) => state.open_barrier_kinds.push(kind),
                PlatePurpose::Bridge(kind) => state.powered_bridge_kinds.push(kind),
                PlatePurpose::Firework => {}
            }
        }
        state.open_barrier_kinds.sort();
        state.powered_bridge_kinds.sort();
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purposes_round_trip_sorted_and_without_fireworks() {
        let state = PlateState::from_purposes([
            PlatePurpose::Bridge(BridgeKindId(1)),
            PlatePurpose::Firework,
            PlatePurpose::Barrier(BarrierKindId(2)),
            PlatePurpose::Barrier(BarrierKindId(0)),
        ]);
        assert_eq!(state.open_barrier_kinds, [BarrierKindId(0), BarrierKindId(2)]);
        assert_eq!(state.powered_bridge_kinds, [BridgeKindId(1)]);
        assert_eq!(
            state.purposes(),
            [
                PlatePurpose::Barrier(BarrierKindId(0)),
                PlatePurpose::Barrier(BarrierKindId(2)),
                PlatePurpose::Bridge(BridgeKindId(1)),
            ]
        );
        assert_eq!(PlateState::from_purposes(state.purposes()), state);
    }
}
