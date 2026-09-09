use std::{iter::once, path::PathBuf};

use crate::map::MapConfig;
use anyhow::{Context, Result, ensure};
use common::protocol::{BarrierKindTable, BridgeKindTable, MapLayout, MapSettings, validate_texture_materials};

use super::definition;

pub struct GeneratedMap {
    pub layout: MapLayout,
    pub config: MapConfig,
}

pub fn generate_map(
    map_name: &str,
    settings: &MapSettings,
    barrier_kinds: &BarrierKindTable,
    bridge_kinds: &BridgeKindTable,
) -> Result<GeneratedMap> {
    let sizes = settings.geometry;
    let path = map_path(map_name);
    let source = definition::load_map(&path).with_context(|| format!("failed to load map at {}", path.display()))?;
    let map_def = source.geometry;
    let nested = source.nested_geometry;
    ensure!(
        !map_def.player_spawn_zones.is_empty(),
        "map {map_name:?} needs at least one player_spawn_zones entry"
    );
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
        .join(map_name)
        .join("layout.json")
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
            &BarrierKindTable::default(),
            &BridgeKindTable::default(),
        )
        .err()
        .expect("missing map must fail");

        assert!(error.to_string().contains("failed to load map at"));
        let missing = PathBuf::from("definitely-not-a-real-map").join("layout.json");
        assert!(
            error
                .to_string()
                .contains(missing.to_str().expect("test path is not UTF-8"))
        );
    }

    #[test]
    fn a_map_cannot_reference_an_alias_outside_its_host_catalog() {
        let config = crate::config::ServerGameplayConfig::load_default().expect("gameplay config is invalid");
        let mut settings = config.maps["obby"].settings.clone();
        settings.textures.remove("basement-floor");
        let error = generate_map(
            "obby",
            &settings,
            &BarrierKindTable::default(),
            &BridgeKindTable::default(),
        )
        .err()
        .expect("undeclared map material was accepted");
        assert!(error.to_string().contains("basement-floor"), "{error}");
    }
}
