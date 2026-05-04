mod compile;
mod load;
mod schema;
mod validation;

#[cfg(test)]
mod tests;

pub(super) use compile::compile_map;
pub(super) use load::load_map;

#[cfg(test)]
use validation::validate_map;

use schema::{ActorSpawnZoneDef, LevelDef, MapDef, MapFile, PlayerSpawnZoneDef, RampDef};

const SUPPORTED_VERSION: u32 = 1;
