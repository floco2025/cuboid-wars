use crate::protocol::FieldId;

// What one body passes through: the fields that are off, plus the fields it
// holds the key to.
#[must_use]
pub fn passable_fields(held_keys: &[FieldId], open: &[FieldId]) -> Vec<FieldId> {
    let mut passable = [open, held_keys].concat();
    passable.sort_unstable();
    passable.dedup();
    passable
}

#[cfg(test)]
#[path = "tests/fields.rs"]
mod tests;
