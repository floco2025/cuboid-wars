use std::{
    iter::once,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use common::{
    config::validate_non_negative_finite,
    protocol::{
        BarrierKindTable, BridgeKindTable, MapLayout, MapSettings, SwitchId, SwitchTable, validate_texture_materials,
    },
};

use super::{FireworksConfig, MapConfig, definition};

pub struct GeneratedMap {
    pub layout: MapLayout,
    pub config: MapConfig,
    // The map's settings with the layout's switch catalog filled in.
    pub settings: MapSettings,
    pub barrier_kinds: BarrierKindTable,
    pub bridge_kinds: BridgeKindTable,
    pub switch_table: SwitchTable,
    pub fireworks: Option<FireworksConfig>,
    // The fireworks switch, resolved and operated by a placed plate.
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
    let source = definition::load_map(path).with_context(|| format!("failed to load map at {}", path.display()))?;
    let (barrier_kinds, bridge_kinds) = settings.kind_tables()?;
    let switch_table = SwitchTable::from_switch_defs(&source.switch_kinds)
        .with_context(|| format!("invalid switch_kinds in {}", path.display()))?;
    let mut settings = settings.clone();
    settings.switches = source.switch_kinds;
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
    let (layout, config) = definition::compile_map(
        &map_def,
        server_hz,
        &settings,
        &nested,
        &barrier_kinds,
        &bridge_kinds,
        &switch_table,
    )
    .with_context(|| format!("failed to compile map at {}", path.display()))?;
    let fireworks_switch = validate_fireworks(source.fireworks.as_ref(), &config, &switch_table)
        .with_context(|| format!("invalid fireworks in {}", path.display()))?;
    Ok(GeneratedMap {
        layout,
        config,
        settings,
        barrier_kinds,
        bridge_kinds,
        switch_table,
        fireworks: source.fireworks,
        fireworks_switch,
    })
}

fn validate_fireworks(
    fireworks: Option<&FireworksConfig>,
    map_config: &MapConfig,
    switches: &SwitchTable,
) -> Result<Option<SwitchId>> {
    let Some(fireworks) = fireworks else {
        return Ok(None);
    };
    let switch = switches.resolve(&fireworks.switch)?;
    if !map_config.pressure_plates.iter().any(|plate| plate.switch == switch) {
        bail!(
            "fireworks switch {:?} is operated by no pressure plate in the map",
            fireworks.switch
        );
    }
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
