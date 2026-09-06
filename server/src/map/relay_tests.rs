use bevy::math::Vec3;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{CollisionWorld, PortalPlacement, compute_portal_placement},
    protocol::{BarrierKindId, BarrierKindTable, BridgeKindId, ItemType, PlatePurpose, PortalMode},
};

use super::{GeneratedMap, generate_map};
use crate::config::{MapServerConfig, ServerGameplayConfig};

struct Relay {
    map: GeneratedMap,
    entry: MapServerConfig,
    gameplay: GameplayConfig,
    world: CollisionWorld,
    carriers: Carriers,
    barriers: BarrierKindTable,
}

impl Relay {
    fn load() -> Self {
        let config = ServerGameplayConfig::load_default().expect("server gameplay config rejected");
        let entry = config.maps.get("relay").expect("relay map settings missing").clone();
        let (barriers, bridges) = entry.settings.kind_tables().expect("relay kind catalogs rejected");
        let map = generate_map(
            "relay",
            entry.settings.geometry,
            &|name| config.maps.get(name).map(|entry| entry.settings.geometry),
            &barriers,
            &bridges,
        )
        .expect("relay map failed to generate");
        let carriers = Carriers::from_layout(&map.layout);
        let mut world = CollisionWorld::from_map_layout(&map.layout, &barriers);
        world.set_carrier_poses(&carriers);
        Self {
            map,
            entry,
            gameplay: config.gameplay_config(),
            world,
            carriers,
            barriers,
        }
    }

    fn eye(&self, level: u8, col: i32, row: i32) -> Vec3 {
        let geometry = self.map.config.root_grid().geometry;
        Vec3::new(
            geometry.cell_center_x(col),
            geometry.level_y(level) + self.gameplay.player.eye_height(),
            geometry.cell_center_z(row),
        )
    }

    fn shot(&self, origin: Vec3, target: Vec3, open: &[BarrierKindId]) -> Option<PortalPlacement> {
        compute_portal_placement(
            origin,
            (target - origin).normalize(),
            0.0,
            self.gameplay.portals.range,
            &self.world,
            &self.map.layout,
            &self.carriers,
            self.entry.settings.portal_shots,
            open,
        )
    }
}

#[test]
fn cyan_plate_exposes_a_fitting_portal_surface_in_the_first_vault() {
    let relay = Relay::load();
    let geometry = relay.map.config.root_grid().geometry;
    let origin = relay.eye(1, 3, 3);
    let target = Vec3::new(
        geometry.cell_to_world_x(14) - geometry.wall_half_thickness(),
        origin.y,
        origin.z,
    );
    assert!(relay.shot(origin, target, &[]).is_none());
    let cyan = relay
        .barriers
        .resolve("sightline")
        .expect("sightline barrier kind missing");
    let placement = relay
        .shot(origin, target, &[cyan])
        .expect("open cyan gates do not expose the vault");
    assert!((placement.pos.x - target.x).abs() < 1e-4);
    assert!(placement.normal.abs_diff_eq(Vec3::NEG_X, 1e-4));
}

#[test]
fn upper_landing_can_portal_into_the_lower_vault_only_with_bridges_unpowered() {
    let mut relay = Relay::load();
    let geometry = relay.map.config.root_grid().geometry;
    let origin = relay.eye(2, 12, 8);
    let target = Vec3::new(geometry.cell_center_x(10), geometry.level_y(1), origin.z);
    for powered in [false, true, false] {
        relay
            .world
            .set_powered_bridges(if powered { &[BridgeKindId(0)] } else { &[] });
        let placement = relay.shot(origin, target, &[]);
        assert_eq!(placement.is_some(), !powered);
        if let Some(placement) = placement {
            assert!((placement.pos.y - geometry.level_y(1)).abs() < 1e-4);
            assert!(placement.normal.abs_diff_eq(Vec3::Y, 1e-4));
        }
    }
}

#[test]
fn rooms_have_fitting_return_portal_surfaces() {
    let relay = Relay::load();
    let geometry = relay.map.config.root_grid().geometry;
    for (level, col, row, wall_col, normal_x) in [
        (1, 3, 3, 2, 1.0),
        (1, 11, 3, 14, -1.0),
        (2, 3, 8, 2, 1.0),
        (2, 12, 8, 15, -1.0),
    ] {
        let origin = relay.eye(level, col, row);
        let target = Vec3::new(
            geometry.cell_to_world_x(wall_col) + normal_x * geometry.wall_half_thickness(),
            origin.y,
            origin.z,
        );
        let placement = relay
            .shot(origin, target, &[])
            .expect("room has no fitting return portal surface");
        assert!((placement.pos.x - target.x).abs() < 1e-4);
    }
}

#[test]
fn upper_amber_gate_requires_entering_the_bridge_corridor_before_shooting_across() {
    let relay = Relay::load();
    let geometry = relay.map.config.root_grid().geometry;
    let origin = relay.eye(2, 5, 8);
    let target = Vec3::new(
        geometry.cell_to_world_x(15) - geometry.wall_half_thickness(),
        origin.y,
        origin.z,
    );
    assert!(relay.shot(origin, target, &[]).is_none());
    let placement = relay
        .shot(relay.eye(2, 6, 8), target, &[])
        .expect("bridge corridor does not expose the far landing");
    assert!((placement.pos.x - target.x).abs() < 1e-4);
}

#[test]
fn lower_vault_wall_caps_prevent_shooting_inside_from_the_recovery_floor() {
    let relay = Relay::load();
    let geometry = relay.map.config.root_grid().geometry;
    for col in [3, 6, 8] {
        let origin = relay.eye(0, col, 8);
        let target = Vec3::new(
            geometry.cell_to_world_x(12) - geometry.wall_half_thickness(),
            geometry.level_y(2) - geometry.floor_thickness() - 0.01,
            origin.z,
        );
        if let Some(placement) = relay.shot(origin, target, &[]) {
            assert!(
                placement.pos.y < geometry.level_y(1) || placement.pos.x < geometry.cell_to_world_x(9),
                "shot from column {col} reached inside the lower vault: {placement:?}"
            );
        }
    }
}

#[test]
fn lower_vault_has_a_portal_escape_even_without_a_saved_return_portal() {
    let relay = Relay::load();
    let geometry = relay.map.config.root_grid().geometry;
    let origin = relay.eye(1, 10, 8);
    let target = Vec3::new(
        geometry.cell_center_x(12),
        geometry.level_y(3) - geometry.floor_thickness(),
        origin.z,
    );
    let placement = relay
        .shot(origin, target, &[])
        .expect("unpowered lower vault has no ceiling escape");
    assert!((placement.pos.y - target.y).abs() < 1e-4);
    assert!(placement.normal.abs_diff_eq(Vec3::NEG_Y, 1e-4));
}

#[test]
fn relay_has_two_tokens_recoverable_keys_and_one_plate_per_purpose() {
    let relay = Relay::load();
    assert!(relay.map.config.actor_spawn_zones.is_empty());
    assert!(relay.entry.random_items.is_none());
    assert_eq!(relay.entry.settings.weapons.portals, PortalMode::Both);
    assert!(relay.entry.settings.portal_shots.barriers_block);
    assert!(relay.entry.settings.portal_shots.light_bridges_block);
    assert_eq!(
        relay
            .map
            .config
            .placed_items
            .iter()
            .filter(|item| item.item_type == ItemType::Cookie)
            .count(),
        2
    );
    assert_eq!(relay.map.config.key_kinds().len(), 2);
    assert!(relay.entry.placed_items.respawn_secs.key <= 5.0);
    assert!(relay.entry.placed_items.respawn_secs.cookie >= 86400.0);
    let purposes: Vec<_> = relay
        .map
        .config
        .pressure_plates
        .iter()
        .map(|plate| plate.purpose)
        .collect();
    assert_eq!(purposes.len(), 3);
    assert_eq!(
        purposes
            .iter()
            .filter(|purpose| matches!(purpose, PlatePurpose::Barrier { .. }))
            .count(),
        1
    );
    assert_eq!(
        purposes
            .iter()
            .filter(|purpose| matches!(purpose, PlatePurpose::Bridge { .. }))
            .count(),
        1
    );
    assert_eq!(
        purposes
            .iter()
            .filter(|purpose| matches!(purpose, PlatePurpose::Firework))
            .count(),
        1
    );
    assert_eq!(relay.map.layout.carriers.len(), 1);
    assert_eq!(relay.map.layout.ladders.len(), 2);
}
