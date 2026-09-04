use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use super::{BarrierKindId, BridgeKindId, PlatePurpose};

// What a holding plate holds while enough of its plates are pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HeldPurpose {
    Barrier(BarrierKindId),
    Bridge(BridgeKindId),
}

impl PlatePurpose {
    // Fireworks are momentary, so this is the one place that says they hold
    // nothing.
    #[must_use]
    pub fn held(self) -> Option<HeldPurpose> {
        match self {
            Self::Barrier(kind) => Some(HeldPurpose::Barrier(kind)),
            Self::Bridge(kind) => Some(HeldPurpose::Bridge(kind)),
            Self::Firework => None,
        }
    }
}

// What the pressure plates currently hold: barrier kinds open (passable and
// invisible) and bridge kinds powered (solid and lit). One value on both
// sides — the server's plate system writes it, every snapshot carries it,
// the collision filters read the open kinds and `powered_bridges_sync_system`
// applies the powered kinds to the bridge colliders. Both lists stay sorted
// so equality diffs are stable.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PlateState {
    pub open_barrier_kinds: Vec<BarrierKindId>,
    pub powered_bridge_kinds: Vec<BridgeKindId>,
}

impl PlateState {
    pub fn from_held(held: impl IntoIterator<Item = HeldPurpose>) -> Self {
        let mut state = Self::default();
        for purpose in held {
            match purpose {
                HeldPurpose::Barrier(kind) => state.open_barrier_kinds.push(kind),
                HeldPurpose::Bridge(kind) => state.powered_bridge_kinds.push(kind),
            }
        }
        state.open_barrier_kinds.sort();
        state.powered_bridge_kinds.sort();
        state
    }

    pub fn held(&self) -> impl Iterator<Item = HeldPurpose> + '_ {
        self.open_barrier_kinds
            .iter()
            .map(|kind| HeldPurpose::Barrier(*kind))
            .chain(self.powered_bridge_kinds.iter().map(|kind| HeldPurpose::Bridge(*kind)))
    }

    #[must_use]
    pub fn contains(&self, purpose: HeldPurpose) -> bool {
        match purpose {
            HeldPurpose::Barrier(kind) => self.open_barrier_kinds.contains(&kind),
            HeldPurpose::Bridge(kind) => self.powered_bridge_kinds.contains(&kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_barrier_and_bridge_plates_hold() {
        assert_eq!(
            PlatePurpose::Barrier(BarrierKindId(2)).held(),
            Some(HeldPurpose::Barrier(BarrierKindId(2)))
        );
        assert_eq!(
            PlatePurpose::Bridge(BridgeKindId(1)).held(),
            Some(HeldPurpose::Bridge(BridgeKindId(1)))
        );
        assert_eq!(PlatePurpose::Firework.held(), None);
    }

    #[test]
    fn held_purposes_round_trip_sorted() {
        let state = PlateState::from_held([
            HeldPurpose::Bridge(BridgeKindId(1)),
            HeldPurpose::Barrier(BarrierKindId(2)),
            HeldPurpose::Barrier(BarrierKindId(0)),
        ]);
        assert_eq!(state.open_barrier_kinds, [BarrierKindId(0), BarrierKindId(2)]);
        assert_eq!(state.powered_bridge_kinds, [BridgeKindId(1)]);
        assert_eq!(
            state.held().collect::<Vec<_>>(),
            [
                HeldPurpose::Barrier(BarrierKindId(0)),
                HeldPurpose::Barrier(BarrierKindId(2)),
                HeldPurpose::Bridge(BridgeKindId(1)),
            ]
        );
        assert!(state.contains(HeldPurpose::Bridge(BridgeKindId(1))));
        assert!(!state.contains(HeldPurpose::Barrier(BarrierKindId(1))));
        assert_eq!(PlateState::from_held(state.held()), state);
    }
}
