use std::path::PathBuf;

use crate::map::MapConfig;
use anyhow::{Context, Result};
use common::{
    map::MapGeometry,
    protocol::{BarrierKindTable, MapLayout},
};

use super::{definition, material_rules::MaterialRules};

pub struct GeneratedMap {
    pub layout: MapLayout,
    pub config: MapConfig,
    pub geometry: MapGeometry,
    pub barrier_kinds: BarrierKindTable,
}

// Load the named map's definition from disk and compile it, with the barrier
// kind table the map file declares.
pub fn generate_map(map_name: &str) -> Result<GeneratedMap> {
    let path = map_path(map_name);
    let map_def = definition::load_map(&path).with_context(|| format!("failed to load map at {}", path.display()))?;
    let barrier_kinds = BarrierKindTable::from_ids(map_def.barrier_kinds.clone())
        .with_context(|| format!("invalid barrier_kinds in map at {}", path.display()))?;
    let assets = MaterialRules::from_def(&map_def);
    let (layout, config, geometry) = definition::compile_map(&map_def, &assets, &barrier_kinds)
        .with_context(|| format!("failed to compile map at {}", path.display()))?;
    Ok(GeneratedMap {
        layout,
        config,
        geometry,
        barrier_kinds,
    })
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
        let error = generate_map("definitely-not-a-real-map")
            .err()
            .expect("missing map must fail");

        assert!(error.to_string().contains("failed to load map at"));
        assert!(error.to_string().contains("definitely-not-a-real-map.json"));
    }

    #[test]
    fn shipped_maps_declare_their_barrier_kinds() {
        let hotel = generate_map("hotel").expect("generate the hotel map");
        assert_eq!(hotel.barrier_kinds.ids(), ["treasure", "basement", "gravity", "lobby"]);
        assert!(
            generate_map("obby")
                .expect("generate the obby map")
                .barrier_kinds
                .is_empty()
        );
    }
}
