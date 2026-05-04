use std::path::PathBuf;

use crate::resources::MapConfig;
use common::{material_rules::MaterialRules, protocol::MapLayout};

use super::definition;

// Load the map definition from disk and compile it to a `MapLayout` +
// `MapConfig`. Hard-fails the server on any parse or validation error so the
// map file stays canonical.
#[must_use]
pub fn generate_map() -> (MapLayout, MapConfig) {
    let path = map_path();
    let map_def =
        definition::load_map(&path).unwrap_or_else(|err| panic!("failed to load map at {}: {err:?}", path.display()));
    let assets =
        MaterialRules::load_default().unwrap_or_else(|err| panic!("failed to load asset material rules: {err:?}"));
    definition::compile_map(&map_def, &assets)
}

fn map_path() -> PathBuf {
    // Look up the map relative to the server crate's manifest, so it
    // works whether the binary is run via `cargo run` or from the target
    // directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../config/server")
        .join("map.json")
}
