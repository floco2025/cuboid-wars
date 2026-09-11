use crate::config::fixtures;
use std::fs;

use bevy::math::Vec3;
use common::{
    constants::{CHARACTER_CARRIER_RIDE_TOLERANCE, TICK_SECS},
    map::Carriers,
    physics::{CharacterEnvironment, CharacterStep, CollisionWorld, LadderMode, step_character_movement},
    protocol::{CarrierId, MapLayout, MapSettings, PlateState, Position},
};
use rand::random;

use super::generation::generate_map_at;

fn carrier_fixture(settings: &MapSettings) -> MapLayout {
    let floor = |col: i32, row: i32| format!(r#"{{"col": {col}, "row": {row}, "all": "basement-floor"}}"#);
    let text = format!(
        r#"{{"map": {{
            "grid_cols": 8, "grid_rows": 8,
            "player_spawn_zones": [{{"level": 0, "cols": [0, 1], "rows": [0, 1]}}],
            "levels": [{{"floors": [{}]}}, {{}}],
            "nested_maps": [
                {{"map": "tile", "level": 0, "from": [2, 2], "to": [6, 2], "travel_secs": 2.0, "pause_secs": 0.5}},
                {{"map": "room", "level": 0, "from": [2, 5], "to": [2, 5], "to_level": 1, "travel_secs": 3.0, "pause_secs": 0.5}}
            ],
            "nested_geometry": {{
                "tile": {{"grid_cols": 1, "grid_rows": 1, "levels": [{{"floors": [{}]}}]}},
                "room": {{"grid_cols": 2, "grid_rows": 2, "levels": [{{"floors": [{}, {}, {}, {}]}}]}}
            }}
        }}}}"#,
        floor(0, 0),
        floor(0, 0),
        floor(0, 0),
        floor(1, 0),
        floor(0, 1),
        floor(1, 1),
    );
    let directory = std::env::temp_dir().join(format!("cuboid_carriers_{}", random::<u64>()));
    fs::create_dir(&directory).expect("temporary map directory unavailable");
    let path = directory.join("layout.json");
    fs::write(&path, text).expect("temporary layout unwritable");
    let generated = generate_map_at(&path, "carriers", 30, settings);
    fs::remove_dir_all(&directory).expect("temporary map directory cleanup failed");
    let layout = generated.expect("carrier fixture failed to generate").layout;
    assert_eq!(layout.carriers.len(), 2);
    layout
}

#[test]
fn every_carrier_carries_a_standing_player_through_its_cycle() {
    let server_gameplay = fixtures::server_config();
    let gameplay = server_gameplay.gameplay_config();
    let physics = gameplay.player.physics();
    let map_settings = &server_gameplay.maps["hotel"].settings;
    let layout = carrier_fixture(map_settings);
    let mut world = CollisionWorld::from_map_layout(&layout);
    let mut carriers = Carriers::from_layout(&layout);
    let plates = PlateState::default();

    for (index, carrier) in layout.carriers.iter().enumerate() {
        let id = CarrierId::from_carried_index(index);
        carriers.advance(0, &plates);
        world.set_carrier_poses(&carriers);
        let mut pos = Position::from(carriers.pose(id).translation);
        let mut vertical_velocity = 0.0;
        let cycle = 2 * (carrier.travel_ticks + carrier.pause_ticks);
        for tick in 1..=cycle {
            carriers.advance(tick, &plates);
            world.set_carrier_poses(&carriers);
            let step = step_character_movement(
                CharacterStep {
                    start: pos,
                    vertical_velocity,
                    control_velocity: Vec3::ZERO,
                    external_displacement: Vec3::ZERO,
                    delta: TICK_SECS,
                },
                &CharacterEnvironment {
                    ladder_mode: LadderMode::Automatic,
                    collision_world: &world,
                    gravity: map_settings.movement.gravity,
                    passable_kinds: &[],
                    physics,
                    ladder_climb_ratio: map_settings.movement.ladder_climb_ratio,
                    portals: None,
                    carriers: &carriers,
                },
            );
            pos = step.position;
            vertical_velocity = step.vertical_velocity;
            let surface = carriers.pose(id).translation;
            let gap = Vec3::from(pos) - surface;
            assert!(
                gap.length() <= CHARACTER_CARRIER_RIDE_TOLERANCE,
                "carrier {} lost its rider at tick {tick}: feet {pos:?}, surface {surface}",
                id.0
            );
        }
    }
}
