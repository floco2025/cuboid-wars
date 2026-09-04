use std::path::PathBuf;

use crate::map::MapConfig;
use anyhow::{Context, Result};
use common::{
    config::MapGeometryConfig,
    map::MapGeometry,
    protocol::{BarrierKindTable, BridgeKindTable, MapLayout},
};

use super::{definition, material_rules::MaterialRules};

pub struct GeneratedMap {
    pub layout: MapLayout,
    pub config: MapConfig,
    pub geometry: MapGeometry,
}

pub fn generate_map(
    map_name: &str,
    sizes: MapGeometryConfig,
    barrier_kinds: &BarrierKindTable,
    bridge_kinds: &BridgeKindTable,
) -> Result<GeneratedMap> {
    let path = map_path(map_name);
    let map_def = definition::load_map(&path).with_context(|| format!("failed to load map at {}", path.display()))?;
    let assets = MaterialRules::from_def(&map_def, sizes);
    let (layout, config, geometry) = definition::compile_map(&map_def, sizes, &assets, barrier_kinds, bridge_kinds)
        .with_context(|| format!("failed to compile map at {}", path.display()))?;
    Ok(GeneratedMap {
        layout,
        config,
        geometry,
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
    use crate::test_geometry::sizes;

    #[test]
    fn missing_map_returns_contextual_error() {
        let error = generate_map(
            "definitely-not-a-real-map",
            sizes(),
            &BarrierKindTable::default(),
            &BridgeKindTable::default(),
        )
        .err()
        .expect("missing map must fail");

        assert!(error.to_string().contains("failed to load map at"));
        assert!(error.to_string().contains("definitely-not-a-real-map.json"));
    }
}
