use bevy::math::Vec3;
use common::{
    constants::{CHARACTER_CARRIER_RIDE_TOLERANCE, TICK_SECS},
    map::Carriers,
    physics::{CharacterEnvironment, CharacterStep, CollisionWorld, LadderMode, step_character_movement},
    protocol::{CarrierId, Position},
};

use super::generate_map;
use crate::config::{ServerGameplayConfig, validate_map_actor_kinds, validate_map_quests};

#[test]
fn every_registered_map_loads_and_validates() {
    let server = ServerGameplayConfig::load_default().expect("server gameplay config rejected");
    for (name, entry) in &server.maps {
        let (barrier_kinds, bridge_kinds) = entry.settings.kind_tables().expect("shipped kind tables rejected");
        let map = generate_map(name, &entry.settings, &barrier_kinds, &bridge_kinds)
            .unwrap_or_else(|error| panic!("shipped map {name:?} failed to generate: {error:#}"));
        validate_map_actor_kinds(&server, &map.config).unwrap_or_else(|error| panic!("{name}: {error}"));
        validate_map_quests(&entry.quests, &map.config, entry.random_items.as_ref())
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
        let (kind_table, bridge_table) = map_settings.kind_tables().expect("shipped kind tables rejected");
        let map_sizes = map_settings.geometry;
        let layout = generate_map(map_name, map_settings, &kind_table, &bridge_table)
            .expect("map failed to generate")
            .layout;
        let world = CollisionWorld::from_map_layout(&layout, &kind_table);
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

// Every shipped carrier carries a standing player through a whole cycle:
// the feet stay on the surface at its origin at every tick. A tile's
// origin is its one cell, a room's the cell its grid is centered on.
#[test]
fn every_shipped_carrier_carries_a_standing_player_through_its_cycle() {
    let server_gameplay = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let gameplay = server_gameplay.gameplay_config();
    let physics = gameplay.player.physics();
    let mut checked = 0;
    for (map_name, map_server_config) in &server_gameplay.maps {
        let map_settings = &map_server_config.settings;
        let (kind_table, bridge_table) = map_settings.kind_tables().expect("shipped kind tables rejected");
        let layout = generate_map(map_name, map_settings, &kind_table, &bridge_table)
            .expect("map failed to generate")
            .layout;
        let mut world = CollisionWorld::from_map_layout(&layout, &kind_table);
        let mut carriers = Carriers::from_layout(&layout);

        for (index, carrier) in layout.carriers.iter().enumerate() {
            let id = CarrierId::from_carried_index(index);
            carriers.advance(0);
            world.set_carrier_poses(&carriers);
            let mut pos = Position::from(carriers.pose(id).translation);
            let mut vertical_velocity = 0.0;
            let cycle = 2 * (carrier.travel_ticks + carrier.pause_ticks);
            for tick in 1..=cycle {
                carriers.advance(tick);
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
            checked += 1;
        }
    }
    assert!(checked > 0, "no shipped map has a carrier to check");
}
