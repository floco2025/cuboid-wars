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
        .join(format!("{map_name}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec3;
    use common::{
        map::Carriers,
        physics::{CollisionWorld, compute_portal_placement},
    };

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
            generate_map(name, &entry.settings, &barrier_kinds, &bridge_kinds)
                .unwrap_or_else(|error| panic!("shipped map {name:?} failed to generate: {error:#}"));
        }
    }
    #[test]
    fn switchyard_customs_has_a_portal_route_back_with_the_gun_and_up_to_the_seal() {
        let server = crate::config::ServerGameplayConfig::load_default().expect("gameplay config rejected");
        let settings = &server.maps["switchyard"].settings;
        let (barriers, bridges) = settings.kind_tables().expect("kind catalogs rejected");
        let generated =
            generate_map("switchyard", settings, &barriers, &bridges).expect("Switchyard failed to compile");
        let geometry = generated.config.root_grid().geometry;
        let layout = &generated.layout;
        let carriers = Carriers::from_layout(layout);
        let world = CollisionWorld::from_map_layout(layout, &barriers);
        let floor_y = geometry.level_y(1);
        let eye = floor_y + server.gameplay_config().player.eye_height();
        let point = |col, row, y| Vec3::new(geometry.cell_center_x(col), y, geometry.cell_center_z(row));
        let place = |origin: Vec3, target: Vec3| {
            compute_portal_placement(
                origin,
                target - origin,
                0.0,
                100.0,
                &world,
                layout,
                &carriers,
                &[],
                &settings.textures,
            )
            .expect("customs portal route blocked")
        };
        let across_gap = place(point(8, 2, eye), point(4, 2, floor_y));
        assert!((across_gap.pos.y - floor_y).abs() < 0.01);
        let entry = place(point(9, 5, eye), point(9, 5, floor_y));
        assert!((entry.pos.y - floor_y).abs() < 0.01);
        let balcony = Vec3::new(
            geometry.cell_center_x(3),
            geometry.level_y(2) + 1.6,
            geometry.cell_to_world_z(3),
        );
        let exit = place(point(3, 6, eye), balcony);
        assert!(exit.pos.y > geometry.level_y(2));
        assert!(exit.normal.abs_diff_eq(Vec3::Z, 0.01));
        assert!(
            world
                .portal_surface_along_ray(point(6, 4, eye), Vec3::X, 30.0, &[])
                .is_none()
        );
    }
}
