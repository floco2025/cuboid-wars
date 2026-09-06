use std::path::PathBuf;

use crate::map::MapConfig;
use anyhow::{Context, Result, ensure};
use common::{
    config::MapGeometryConfig,
    protocol::{BarrierKindTable, BridgeKindTable, MapLayout},
};

use super::definition::{self, load_map_tree};

pub struct GeneratedMap {
    pub layout: MapLayout,
    pub config: MapConfig,
    // What compile skipped; logged by the caller, since compile runs before
    // the log plugin is installed.
    pub warnings: Vec<String>,
}

// `nested_geometry` is the registry lookup for the maps this one nests.
pub fn generate_map(
    map_name: &str,
    sizes: MapGeometryConfig,
    nested_geometry: &dyn Fn(&str) -> Option<MapGeometryConfig>,
    barrier_kinds: &BarrierKindTable,
    bridge_kinds: &BridgeKindTable,
) -> Result<GeneratedMap> {
    let path = map_path(map_name);
    let map_def = definition::load_map(&path).with_context(|| format!("failed to load map at {}", path.display()))?;
    // A nested file may leave spawning to its host; the map being played
    // must offer somewhere to spawn.
    ensure!(
        !map_def.player_spawn_zones.is_empty(),
        "map {map_name:?} needs at least one player_spawn_zones entry"
    );
    let nested = load_map_tree(map_name, &map_def, sizes, nested_geometry, &mut |name| {
        definition::load_map(&map_path(name))
    })?;
    let (layout, config, warnings) = definition::compile_map(&map_def, sizes, &nested, barrier_kinds, bridge_kinds)
        .with_context(|| format!("failed to compile map at {}", path.display()))?;
    Ok(GeneratedMap {
        layout,
        config,
        warnings,
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
            &|_| None,
            &BarrierKindTable::default(),
            &BridgeKindTable::default(),
        )
        .err()
        .expect("missing map must fail");

        assert!(error.to_string().contains("failed to load map at"));
        assert!(error.to_string().contains("definitely-not-a-real-map.json"));
    }

    #[test]
    fn every_shipped_map_generates() {
        let server_gameplay =
            crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        for (name, entry) in &server_gameplay.maps {
            let (barrier_kinds, bridge_kinds) = entry.settings.kind_tables().expect("shipped kind tables rejected");
            generate_map(
                name,
                entry.settings.geometry,
                &|nested| server_gameplay.maps.get(nested).map(|map| map.settings.geometry),
                &barrier_kinds,
                &bridge_kinds,
            )
            .unwrap_or_else(|error| panic!("shipped map {name:?} failed to generate: {error:#}"));
        }
    }
}
