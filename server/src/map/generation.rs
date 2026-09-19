use std::{
    iter::once,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use bevy::log::warn;
use common::{
    config::validate_non_negative_finite,
    protocol::{FieldTable, MapLayout, MapSettings, SwitchId, SwitchTable, validate_texture_materials},
};

use super::{FireworksConfig, MapConfig, definition};

pub struct GeneratedMap {
    pub layout: MapLayout,
    pub config: MapConfig,
    // The map's settings with the layout's catalogs filled in.
    pub settings: MapSettings,
    pub fields: FieldTable,
    pub switch_table: SwitchTable,
    pub fireworks: Option<FireworksConfig>,
    pub fireworks_switch: Option<SwitchId>,
}

pub fn generate_map(map_name: &str, server_hz: u32, settings: &MapSettings) -> Result<GeneratedMap> {
    generate_map_at(&map_path(map_name), map_name, server_hz, settings)
}

pub(crate) fn generate_map_at(
    path: &Path,
    map_name: &str,
    server_hz: u32,
    settings: &MapSettings,
) -> Result<GeneratedMap> {
    let source = map_core::load_map(path).with_context(|| format!("failed to load map at {}", path.display()))?;
    for warning in &source.warnings {
        warn!("map {map_name:?}: {warning}");
    }
    let fields =
        FieldTable::from_field_defs(&source.fields).with_context(|| format!("invalid fields in {}", path.display()))?;
    let switch_table = SwitchTable::from_switch_defs(&source.switches)
        .with_context(|| format!("invalid switches in {}", path.display()))?;
    let mut settings = settings.clone();
    settings.switches = source.switches;
    settings.fields = source.fields;
    let map_def = source.geometry;
    let nested = source.nested_geometry;
    for (name, map) in once((map_name, &map_def)).chain(nested.iter().map(|(name, map)| (name.as_str(), map))) {
        for (level, tier) in map.levels.iter().enumerate() {
            for (index, floor) in tier.floors.iter().chain(&tier.inaccessible_floors).enumerate() {
                validate_texture_materials(
                    &floor.materials,
                    &settings.textures,
                    &format!("map {name:?} level {level} floor {index}"),
                )?;
            }
            for (index, terrain) in tier.terrain.iter().enumerate() {
                let mut authored_faces = terrain.materials.clone();
                authored_faces.top = authored_faces.bottom.clone();
                validate_texture_materials(
                    &authored_faces,
                    &settings.textures,
                    &format!("map {name:?} level {level} terrain {index}"),
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
    let (layout, config) = definition::compile_map(&map_def, server_hz, &settings, &nested, &fields, &switch_table)
        .with_context(|| format!("failed to compile map at {}", path.display()))?;
    let fireworks_switch = validate_fireworks(source.fireworks.as_ref(), &switch_table)
        .with_context(|| format!("invalid fireworks in {}", path.display()))?;
    Ok(GeneratedMap {
        layout,
        config,
        settings,
        fields,
        switch_table,
        fireworks: source.fireworks,
        fireworks_switch,
    })
}

fn validate_fireworks(fireworks: Option<&FireworksConfig>, switches: &SwitchTable) -> Result<Option<SwitchId>> {
    let Some(fireworks) = fireworks else {
        return Ok(None);
    };
    let switch = switches.resolve(&fireworks.switch)?;
    validate_non_negative_finite(fireworks.cooldown_secs, "fireworks.cooldown_secs")?;
    Ok(Some(switch))
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
#[path = "tests/generation.rs"]
mod tests;
