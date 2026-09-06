mod compile;
mod geometry;
mod load;
mod schema;
mod validation;

#[cfg(test)]
mod tests;

pub(super) use compile::compile_map;
pub(super) use load::{load_map, load_map_tree};
pub(crate) use schema::MapDef;
pub(super) use schema::{WallLightDef, WallSide};
