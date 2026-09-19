use crate::protocol::{Barrier, BarrierId, BarrierKindId};

#[must_use]
pub fn passable_barriers(held_keys: &[BarrierKindId], open: &[BarrierId], barriers: &[Barrier]) -> Vec<BarrierId> {
    let mut passable = open.to_vec();
    passable.extend(
        barriers
            .iter()
            .filter(|barrier| held_keys.contains(&barrier.kind))
            .map(|barrier| barrier.id),
    );
    passable.sort_unstable();
    passable.dedup();
    passable
}

#[cfg(test)]
#[path = "tests/barriers.rs"]
mod tests;
