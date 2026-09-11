use std::fs;

use bevy::math::Vec3;
use common::{
    constants::{CHARACTER_CARRIER_RIDE_TOLERANCE, TICK_SECS},
    map::{CarrierRun, Carriers},
    physics::{CharacterEnvironment, CharacterStep, CollisionWorld, LadderMode, step_character_movement},
    protocol::{CarrierId, MapLayout, MapSettings, PlateState, Position},
};
use rand::random;

use super::{generate_map, generation::generate_map_at};
use crate::config::{ServerGameplayConfig, validate_map_actor_kinds, validate_map_quests};

#[test]
fn every_registered_map_loads_and_validates() {
    let server = ServerGameplayConfig::load_default().expect("server gameplay config rejected");
    for (name, entry) in &server.maps {
        let map = generate_map(name, 30, &entry.settings)
            .unwrap_or_else(|error| panic!("shipped map {name:?} failed to generate: {error:#}"));
        validate_map_actor_kinds(&server, &map.config).unwrap_or_else(|error| panic!("{name}: {error}"));
        validate_map_quests(
            &entry.quests,
            &map.config,
            entry.random_items.as_ref(),
            map.fireworks_switch,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

// Simulate a player climbing every authored ladder in every shipped map:
// stand in the climb volume at the base, hold walk-speed input into the face,
// and require the ascent to gain at least one full storey. Catches authoring
// mistakes the permissive placement rules allow — most importantly a ladder
// facing the wrong way, whose climb volume sits under the very slab it should
// arrive on (the ascent stalls on the slab's underside).
#[test]
fn every_shipped_ladder_ascends_at_least_one_storey() {
    let server_gameplay = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let gameplay = server_gameplay.gameplay_config();
    let physics = gameplay.player.physics();
    for (map_name, map_server_config) in &server_gameplay.maps {
        let map_settings = &map_server_config.settings;
        let map_sizes = map_settings.geometry;
        let layout = generate_map(map_name, 30, map_settings)
            .expect("map failed to generate")
            .layout;
        let world = CollisionWorld::from_map_layout(&layout);
        let carriers = Carriers::from_layout(&layout);

        for ladder in &layout.ladders {
            // A carried ladder's record is in its carrier's frame.
            let pose = carriers.pose(ladder.carrier);
            let mid_x = f32::midpoint(ladder.x1, ladder.x2);
            let mid_z = f32::midpoint(ladder.z1, ladder.z2);
            let mut pos = Position::from(pose.transform_point(Vec3::new(
                ladder.nx.mul_add(0.6, mid_x),
                ladder.y,
                ladder.nz.mul_add(0.6, mid_z),
            )));
            let mut vertical_velocity = 0.0;
            let one_storey_up = pose.translation.y + ladder.y + map_sizes.level_height - 0.05;
            let speed = map_settings.movement.player.walk_speed;
            let mut reached = false;
            for _ in 0..600 {
                let step = step_character_movement(
                    CharacterStep {
                        start: pos,
                        vertical_velocity,
                        control_velocity: Vec3::new(-ladder.nx * speed, 0.0, -ladder.nz * speed),
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
                if pos.y >= one_storey_up {
                    reached = true;
                    break;
                }
            }
            assert!(
                reached,
                "{}: ladder at ({:.1}, {:.1}) level {} stalled at y={:.2} (needed {:.2}) — \
                 likely facing the wrong way (landing over the climb side)",
                map_name, mid_x, mid_z, ladder.level, pos.y, one_storey_up
            );
        }
    }
}

// A host nesting a sliding tile and a rising room, generated like a shipped
// map, so the ride is exercised while no shipped map nests one.
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

// Every carrier, shipped or in the fixture, carries a standing player
// through a whole cycle: the feet stay on the surface at its origin at every
// tick. A tile's origin is its one cell, a room's the cell its grid is
// centered on.
#[test]
fn every_carrier_carries_a_standing_player_through_its_cycle() {
    let server_gameplay = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let gameplay = server_gameplay.gameplay_config();
    let physics = gameplay.player.physics();
    let hotel_settings = &server_gameplay.maps["hotel"].settings;
    let shipped = server_gameplay.maps.iter().map(|(map_name, map_server_config)| {
        let map_settings = &map_server_config.settings;
        let layout = generate_map(map_name, 30, map_settings)
            .expect("map failed to generate")
            .layout;
        (map_name.as_str(), map_settings, layout)
    });
    let fixture = ("fixture", hotel_settings, carrier_fixture(hotel_settings));
    for (map_name, map_settings, layout) in shipped.chain([fixture]) {
        let mut world = CollisionWorld::from_map_layout(&layout);
        let mut carriers = Carriers::from_layout(&layout);
        // Every switched carrier running from tick 0, so it cycles like a free one.
        let mut plates = PlateState::default();
        for (index, carrier) in layout.carriers.iter().enumerate() {
            if carrier.switch.is_some() {
                plates.carrier_runs.push((
                    CarrierId::from_carried_index(index),
                    CarrierRun::STOPPED.set_running(true, 0),
                ));
            }
        }

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
                    "{}: carrier {} lost its rider at tick {tick}: feet {pos:?}, surface {surface}",
                    map_name,
                    id.0
                );
            }
        }
    }
}
