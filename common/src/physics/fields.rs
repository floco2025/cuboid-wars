use crate::protocol::{Barrier, FieldId, FieldKindId, LightBridge};

// What one body passes through: the fields that are off, plus every barrier
// and light bridge of a kind it holds the key to.
#[must_use]
pub fn passable_fields(
    held_keys: &[FieldKindId],
    open: &[FieldId],
    barriers: &[Barrier],
    bridges: &[LightBridge],
) -> Vec<FieldId> {
    let mut passable = open.to_vec();
    passable.extend(
        barriers
            .iter()
            .filter(|barrier| held_keys.contains(&barrier.kind))
            .map(|barrier| FieldId::Barrier(barrier.id)),
    );
    passable.extend(
        bridges
            .iter()
            .filter(|bridge| held_keys.contains(&bridge.kind))
            .map(|bridge| FieldId::Bridge(bridge.id)),
    );
    passable.sort_unstable();
    passable.dedup();
    passable
}

#[cfg(test)]
#[path = "tests/fields.rs"]
mod tests;
