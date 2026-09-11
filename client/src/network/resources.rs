use bevy::prelude::*;
use std::{collections::VecDeque, time::Duration};

use common::protocol::sequence_is_newer;

// Newest `SSnapshot.tick` applied; an older snapshot is ignored. `None`
// until the first one.
#[derive(Resource, Default)]
pub struct LastSnapshotTick(pub Option<u32>);

// Newest `SPlayerMoves.tick` applied; same contract as `LastSnapshotTick`.
#[derive(Resource, Default)]
pub struct LastPlayerMovesTick(pub Option<u32>);

// Records `tick` as the newest applied and says whether it was newer than
// the last; a first tick is always newest.
pub fn accept_newer_tick(last: &mut Option<u32>, tick: u32) -> bool {
    if last.is_some_and(|last| !sequence_is_newer(tick, last)) {
        return false;
    }
    *last = Some(tick);
    true
}

// Round-trip time to server.
#[derive(Resource, Default)]
pub struct RoundTripTime {
    pub rtt: Duration,
    pub measurements: VecDeque<Duration>,
}

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
