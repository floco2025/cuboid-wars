use std::{iter::once, path::PathBuf};

use crate::map::MapConfig;
use anyhow::{Context, Result, ensure};
use common::{
    config::MapGeometryConfig,
    protocol::{BarrierKindTable, BridgeKindTable, MapLayout, MapSettings, validate_texture_materials},
};

use super::definition::{self, load_map_tree};

pub struct GeneratedMap {
    pub layout: MapLayout,
    pub config: MapConfig,
}

// `nested_geometry` is the registry lookup for the maps this one nests.
pub fn generate_map(
    map_name: &str,
    settings: &MapSettings,
    nested_geometry: &dyn Fn(&str) -> Option<MapGeometryConfig>,
    barrier_kinds: &BarrierKindTable,
    bridge_kinds: &BridgeKindTable,
) -> Result<GeneratedMap> {
    let sizes = settings.geometry;
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
    for (name, map) in once((map_name, &map_def)).chain(nested.iter().map(|(name, map)| (name.as_str(), map))) {
        for (level, tier) in map.levels.iter().enumerate() {
            for (index, floor) in tier.floors.iter().chain(&tier.inaccessible_floors).enumerate() {
                validate_texture_materials(
                    &floor.materials,
                    &settings.textures,
                    &format!("map {name:?} level {level} floor {index}"),
                )?;
            }
            for (index, wall) in tier.walls.iter().enumerate() {
                validate_texture_materials(
                    &wall.materials,
                    &settings.textures,
                    &format!("map {name:?} level {level} wall {index}"),
                )?;
            }
        }
        for (index, ramp) in map.ramps.iter().enumerate() {
            validate_texture_materials(
                &ramp.materials,
                &settings.textures,
                &format!("map {name:?} ramp {index}"),
            )?;
        }
    }
    let (layout, config) = definition::compile_map(&map_def, sizes, &nested, barrier_kinds, bridge_kinds)
        .with_context(|| format!("failed to compile map at {}", path.display()))?;
    Ok(GeneratedMap { layout, config })
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
        let error = generate_map(
            "definitely-not-a-real-map",
            &crate::config::ServerGameplayConfig::load_default()
                .expect("gameplay config is invalid")
                .maps["hotel"]
                .settings,
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
    fn a_map_cannot_reference_an_alias_outside_its_host_catalog() {
        let config = crate::config::ServerGameplayConfig::load_default().expect("gameplay config is invalid");
        let mut settings = config.maps["obby"].settings.clone();
        settings.textures.remove("basement-floor");
        let error = generate_map(
            "obby",
            &settings,
            &|_| None,
            &BarrierKindTable::default(),
            &BridgeKindTable::default(),
        )
        .err()
        .expect("undeclared map material was accepted");
        assert!(error.to_string().contains("basement-floor"), "{error}");
    }

    #[test]
    fn every_shipped_map_generates() {
        let server_gameplay =
            crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        for (name, entry) in &server_gameplay.maps {
            let (barrier_kinds, bridge_kinds) = entry.settings.kind_tables().expect("shipped kind tables rejected");
            generate_map(
                name,
                &entry.settings,
                &|nested| server_gameplay.maps.get(nested).map(|map| map.settings.geometry),
                &barrier_kinds,
                &bridge_kinds,
            )
            .unwrap_or_else(|error| panic!("shipped map {name:?} failed to generate: {error:#}"));
        }
    }
}
