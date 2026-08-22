use std::path::PathBuf;

use crate::map::MapConfig;
use anyhow::{Context, Result};
use common::{
    map::MapGeometry,
    protocol::{BarrierKindTable, MapLayout},
};

use super::{definition, material_rules::MaterialRules};

// Load the named map's definition from disk and compile it to a `MapLayout` +
// `MapConfig` + `MapGeometry`.
pub fn generate_map(kind_table: &BarrierKindTable, map_name: &str) -> Result<(MapLayout, MapConfig, MapGeometry)> {
    let path = map_path(map_name);
    let map_def = definition::load_map(&path).with_context(|| format!("failed to load map at {}", path.display()))?;
    let assets = MaterialRules::from_def(&map_def);
    definition::compile_map(&map_def, &assets, kind_table)
        .with_context(|| format!("failed to compile map at {}", path.display()))
}

pub(crate) fn map_path(map_name: &str) -> PathBuf {
    // Look up the map relative to the server crate's manifest, so it
    // works whether the binary is run via `cargo run` or from the target
    // directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../config/server/maps")
        .join(format!("{map_name}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_map_returns_contextual_error() {
        let kinds = BarrierKindTable::from_ids(vec!["red".to_owned()]).expect("build barrier table");

        let error = generate_map(&kinds, "definitely-not-a-real-map")
            .err()
            .expect("missing map must fail");

        assert!(error.to_string().contains("failed to load map at"));
        assert!(error.to_string().contains("definitely-not-a-real-map.json"));
    }
}
