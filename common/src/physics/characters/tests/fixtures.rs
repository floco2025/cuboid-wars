pub(super) use super::super::*;
pub(super) use crate::{
    config::CharacterPhysicsConfig,
    map::{Carriers, ramp_surface_at},
    physics::CollisionWorld,
    protocol::{Floor, Ladder, MapLayout, Position, Ramp, Wall},
    test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS},
};
use crate::{
    config::gameplay::load_test_gameplay,
    protocol::{BarrierKindTable, Carrier, CarrierId, PlateState},
};
pub(super) use bevy_math::Vec3;

// Movement tuning for movement tests (the shipping map's settings): gravity
// magnitude, ladder climb ratio, and the player's run speed.
pub(crate) const TEST_GRAVITY: f32 = 25.0;
pub(crate) const TEST_LADDER_CLIMB_RATIO: f32 = 0.4;
pub(crate) const TEST_PLAYER_SPEED: f32 = 9.0;

pub(crate) fn test_ramp() -> Ramp {
    Ramp {
        x1: 0.0,
        y1: 0.0,
        z1: 0.0,
        x2: 4.0,
        y2: LEVEL_HEIGHT,
        z2: 8.0,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn upper_floor_west_of_ramp() -> Floor {
    Floor {
        x1: -4.0,
        z1: 0.0,
        x2: 0.0,
        z2: 8.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn lower_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn upper_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn low_overhead_floor() -> Floor {
    let player_physics = player_physics();
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: player_physics.movement_collider.height + FLOOR_THICKNESS + 0.02,
        thickness: FLOOR_THICKNESS,
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

// Edge plane at z = 0 spanning x -0.5..0.5, climbable from the -Z rail side.
// Spans level 0 -> 1; the landing fixtures put floors on the +Z side.
pub(crate) fn test_ladder() -> Ladder {
    Ladder {
        x1: -0.5,
        z1: 0.0,
        x2: 0.5,
        z2: 0.0,
        nx: 0.0,
        nz: -1.0,
        level: 0,
        levels: 1,
        y: 0.0,
        height: LEVEL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

// Ground in front of `test_ladder` (the climb side).
pub(crate) fn ladder_front_base_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 0.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

// Two-storey variant of `test_ladder` on the same edge.
pub(crate) fn test_ladder_two_storey() -> Ladder {
    Ladder {
        levels: 2,
        height: 2.0 * LEVEL_HEIGHT,
        ..test_ladder()
    }
}

// Ground behind `test_ladder`, same storey as the front base floor. The
// fence still blocks front-side crossings here; back-side walkers pass
// through onto the front floor.
pub(crate) fn ladder_back_base_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: 0.0,
        x2: 4.0,
        z2: 4.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

// Landing behind `test_ladder`, one storey up.
pub(crate) fn ladder_back_landing_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: 0.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

// X-facing variant of `test_ladder`: edge plane at x = 0 spanning z -0.5..0.5
// — the character collider's wide (1.0 m) axis faces this plane.
pub(crate) fn test_ladder_facing_x() -> Ladder {
    Ladder {
        x1: 0.0,
        z1: -0.5,
        x2: 0.0,
        z2: 0.5,
        nx: -1.0,
        nz: 0.0,
        level: 0,
        levels: 1,
        y: 0.0,
        height: LEVEL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

// Landing behind `test_ladder_facing_x`, one storey up.
pub(crate) fn ladder_back_landing_floor_x() -> Floor {
    Floor {
        x1: 0.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn ladder_collision_world(floors: &[Floor], ladders: &[Ladder]) -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            floors: floors.to_vec(),
            ladders: ladders.to_vec(),
            ..Default::default()
        },
        &BarrierKindTable::default(),
    )
}

pub(crate) fn test_wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: -2.0,
        x2: 0.0,
        z2: 2.0,
        width: 0.2,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn horizontal_wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: 0.0,
        x2: 4.0,
        z2: 0.0,
        width: 0.2,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn upper_horizontal_wall() -> Wall {
    Wall {
        x1: 27.85,
        z1: 32.0,
        x2: 35.85,
        z2: 32.0,
        width: 0.3,
        level: 2,
        y: 2.0 * LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn collision_world(floors: &[Floor], ramps: &[Ramp]) -> CollisionWorld {
    collision_world_with(&[], floors, ramps)
}

pub(crate) fn collision_world_with(walls: &[Wall], floors: &[Floor], ramps: &[Ramp]) -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            walls: walls.to_vec(),
            ramps: ramps.to_vec(),
            floors: floors.to_vec(),
            ..Default::default()
        },
        &BarrierKindTable::default(),
    )
}

// The world `tick` ticks into its carriers' cycles: `previous` is the pose
// one tick earlier, and the colliders already sit at `current`.
pub(crate) fn world_at(layout: &MapLayout, tick: u32) -> (CollisionWorld, Carriers) {
    let mut world = CollisionWorld::from_map_layout(layout, &BarrierKindTable::default());
    let mut carriers = Carriers::from_layout(layout);
    carriers.advance(tick.wrapping_sub(1), &PlateState::default());
    carriers.advance(tick, &PlateState::default());
    world.set_carrier_poses(&carriers);
    (world, carriers)
}

// `world_at` for one carrier and the slab it carries, among static walls and floors.
pub(crate) fn carried_world(
    (carrier, floor): (Carrier, Floor),
    walls: &[Wall],
    floors: &[Floor],
    tick: u32,
) -> (CollisionWorld, Carriers) {
    let layout = MapLayout {
        walls: walls.to_vec(),
        floors: floors.iter().copied().chain([floor]).collect(),
        carriers: vec![carrier],
        ..Default::default()
    };
    world_at(&layout, tick)
}

pub(crate) const TILE: CarrierId = CarrierId(1);

// Slides from the origin four meters along +X in two seconds: 2 m/s, one
// fifteenth of a meter per tick. The tile is a slab centered on the
// carrier's origin.
pub(crate) fn slider() -> (Carrier, Floor) {
    (
        Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::default(),
            to: Position { x: 4.0, y: 0.0, z: 0.0 },
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        },
        Floor {
            x1: -1.5,
            z1: -1.5,
            x2: 1.5,
            z2: 1.5,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: TILE,
        },
    )
}

// One player step among `world`'s carriers.
pub(crate) fn ride(
    world: &CollisionWorld,
    carriers: &Carriers,
    start: Position,
    vertical_velocity: f32,
    control_velocity: Vec3,
    delta: f32,
) -> CharacterMovementResult {
    step_character_movement(
        CharacterStep {
            start,
            vertical_velocity,
            control_velocity,
            external_displacement: Vec3::ZERO,
            delta,
        },
        &test_environment(world, carriers, player_physics(), LadderMode::Automatic),
    )
}

pub(crate) fn player_physics() -> CharacterPhysicsConfig {
    load_test_gameplay()
        .expect("test gameplay config rejected")
        .player
        .physics()
}

// The environment every movement test steps in: no passable barrier kinds
// and no portals, in the given world and carriers.
pub(crate) fn test_environment<'a>(
    world: &'a CollisionWorld,
    carriers: &'a Carriers,
    physics: CharacterPhysicsConfig,
    ladder_mode: LadderMode,
) -> CharacterEnvironment<'a> {
    CharacterEnvironment {
        collision_world: world,
        gravity: TEST_GRAVITY,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: TEST_LADDER_CLIMB_RATIO,
        ladder_mode,
        portals: None,
        carriers,
    }
}

// One player step in a world without carriers.
pub(crate) fn step_in(world: &CollisionWorld, step: CharacterStep, ladder_mode: LadderMode) -> CharacterMovementResult {
    step_character_movement(
        step,
        &test_environment(world, &Carriers::default(), player_physics(), ladder_mode),
    )
}

pub(crate) fn character_step_toward(
    start: Position,
    vertical_velocity: f32,
    target_x: f32,
    target_z: f32,
    delta: f32,
) -> CharacterStep {
    CharacterStep {
        start,
        vertical_velocity,
        control_velocity: Vec3::new((target_x - start.x) / delta, 0.0, (target_z - start.z) / delta),
        external_displacement: Vec3::ZERO,
        delta,
    }
}
