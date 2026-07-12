use std::path::PathBuf;

use crate::resources::MapConfig;
use common::{
    map_geometry::MapGeometry,
    protocol::{BarrierKindTable, MapLayout},
};

use super::{definition, material_rules::MaterialRules};

// Load the named map's definition from disk and compile it to a `MapLayout` +
// `MapConfig` + `MapGeometry`. Hard-fails the server on any parse or
// validation error so the map file stays canonical.
#[must_use]
pub fn generate_map(kind_table: &BarrierKindTable, map_name: &str) -> (MapLayout, MapConfig, MapGeometry) {
    let path = map_path(map_name);
    let map_def =
        definition::load_map(&path).unwrap_or_else(|err| panic!("failed to load map at {}: {err:?}", path.display()));
    let assets = MaterialRules::load_from_map_path(&path)
        .unwrap_or_else(|err| panic!("failed to load asset material rules from {}: {err:?}", path.display()));
    definition::compile_map(&map_def, &assets, kind_table)
        .unwrap_or_else(|err| panic!("failed to compile map: {err:?}"))
}

pub(crate) fn map_path(map_name: &str) -> PathBuf {
    // Look up the map relative to the server crate's manifest, so it
    // works whether the binary is run via `cargo run` or from the target
    // directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../config/server/maps")
        .join(format!("{map_name}.json"))
}
