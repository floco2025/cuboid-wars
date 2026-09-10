pub(super) use super::super::*;
pub(super) use crate::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_RIM_SCALE, TICK_SECS},
    map::Carriers,
    physics::{
        CharacterEnvironment, CharacterStep, CharacterSupport, CollisionWorld, LadderMode, step_character_movement,
    },
    protocol::{
        BarrierKindId, BarrierKindTable, Carrier, CarrierId, FaceMaterials, Floor, MapLayout, Portal, PortalEnd,
        PortalPairId, Position, Ramp, Wall,
    },
    test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS},
};
pub(super) use bevy_math::Vec3;
pub(super) use std::f32::consts::PI;

use std::collections::{BTreeMap, HashMap};

use crate::{
    config::{
        CharacterPhysicsConfig, KnockbackConfig, MapMovementConfig, PlayerMovementConfig, gameplay::load_test_gameplay,
    },
    protocol::TextureSettings,
};

pub(crate) const CAP: f32 = 22.5;
pub(crate) const LADDER_CLIMB_RATIO: f32 = 0.4;
pub(crate) const TILE: CarrierId = CarrierId(1);

pub(crate) fn map_movement() -> MapMovementConfig {
    MapMovementConfig {
        player: PlayerMovementConfig {
            walk_speed: 6.0,
            run_speed: 9.0,
            speed_power_up: 1.6,
            jump_speed: 12.0,
        },
        actors: HashMap::new(),
        missile_speed: 16.0,
        projectile_speed: 90.0,
        gravity: 25.0,
        low_gravity: 5.0,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        knockback: KnockbackConfig {
            max_speed: 15.0,
            up_speed: 7.0,
            deceleration: 35.0,
        },
    }
}

pub(crate) fn player_physics() -> CharacterPhysicsConfig {
    load_test_gameplay()
        .expect("test gameplay config rejected")
        .player
        .physics()
}

pub(crate) fn portal(end: PortalEnd, pos: Vec3, normal: Vec3, yaw: f32) -> Portal {
    Portal {
        pair: PortalPairId(1),
        end,
        pos: pos.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw,
        carrier: CarrierId::WORLD,
    }
}

pub(crate) fn empty_world() -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default())
}

pub(crate) fn pair(a_pos: Vec3, a_normal: Vec3, b_pos: Vec3, b_normal: Vec3) -> PortalSet {
    PortalSet::rebuild(
        &[
            portal(PortalEnd::A, a_pos, a_normal, 0.0),
            portal(PortalEnd::B, b_pos, b_normal, 0.0),
        ],
        &empty_world(),
        &Carriers::default(),
    )
}

pub(crate) fn frames(set: &PortalSet) -> (&PortalFrame, &PortalFrame) {
    set.first_pair_frames().expect("portal set has no pair")
}

pub(crate) fn moving_projectile_portals(
    entry_travel: Vec3,
    exit_travel: Vec3,
    obstacles: &[Wall],
) -> (CollisionWorld, PortalSet) {
    let carrier = Carrier {
        parent: CarrierId::WORLD,
        level: 0,
        levels: 0,
        from: Position::default(),
        to: entry_travel.into(),
        travel_ticks: 1,
        pause_ticks: 0,
        phase_ticks: 0,
    };
    let wall = Wall {
        x1: -2.0,
        x2: 2.0,
        z1: -0.1,
        z2: -0.1,
        y: 0.0,
        height: 3.0,
        width: 0.2,
        level: 0,
        carrier: CarrierId(1),
    };
    let layout = MapLayout {
        carriers: vec![
            carrier,
            Carrier {
                from: (Vec3::X * 10.0).into(),
                to: (Vec3::X * 10.0 + exit_travel).into(),
                ..carrier
            },
        ],
        walls: [
            wall,
            Wall {
                carrier: CarrierId(2),
                ..wall
            },
        ]
        .into_iter()
        .chain(obstacles.iter().copied())
        .collect(),
        ..Default::default()
    };
    let (world, carriers) = tile_world(&layout, 1);
    let set = PortalSet::rebuild(
        &[
            Portal {
                carrier: CarrierId(1),
                ..portal(PortalEnd::A, Vec3::Y, Vec3::Z, 0.0)
            },
            Portal {
                carrier: CarrierId(2),
                ..portal(PortalEnd::B, Vec3::Y, Vec3::Z, 0.0)
            },
        ],
        &world,
        &carriers,
    );
    (world, set)
}

// One 12 m wall along X at z = 0 (level 0) with the room floor on +Z.
pub(crate) fn placement_layout() -> MapLayout {
    MapLayout {
        walls: vec![Wall {
            x1: -6.0,
            z1: 0.0,
            x2: 6.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -6.0,
            z1: 0.0,
            x2: 6.0,
            z2: 6.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    }
}

pub(crate) fn textured_layout(layout: &MapLayout) -> MapLayout {
    MapLayout {
        wall_materials: vec![FaceMaterials::uniform("test"); layout.walls.len()],
        floor_materials: vec![FaceMaterials::uniform("test"); layout.floors.len()],
        ramp_materials: vec![FaceMaterials::uniform("test"); layout.ramps.len()],
        ..layout.clone()
    }
}

pub(crate) fn test_textures() -> BTreeMap<String, TextureSettings> {
    BTreeMap::from([
        ("test".to_owned(), TextureSettings { portalable: true }),
        ("blocked".to_owned(), TextureSettings { portalable: false }),
    ])
}

pub(crate) fn place_on_geometry(
    origin: Vec3,
    direction: Vec3,
    yaw: f32,
    range: f32,
    world: &CollisionWorld,
    layout: &MapLayout,
    carriers: &Carriers,
    open: &[BarrierKindId],
) -> Option<PortalPlacement> {
    compute_portal_placement(
        origin,
        direction,
        yaw,
        range,
        world,
        &textured_layout(layout),
        carriers,
        open,
        &test_textures(),
    )
    .ok()
}

pub(crate) fn place(layout: &MapLayout, origin: Vec3, toward: Vec3, yaw: f32) -> Option<PortalPlacement> {
    let world = CollisionWorld::from_map_layout(layout, &BarrierKindTable::default());
    place_on_geometry(
        origin,
        (toward - origin).normalize(),
        yaw,
        40.0,
        &world,
        layout,
        &Carriers::default(),
        &[],
    )
}

pub(crate) fn material_shot(
    layout: &MapLayout,
    origin: Vec3,
    direction: Vec3,
) -> Result<PortalPlacement, PortalPlacementFailure> {
    let world = CollisionWorld::from_map_layout(layout, &BarrierKindTable::default());
    compute_portal_placement(
        origin,
        direction,
        0.0,
        40.0,
        &world,
        layout,
        &Carriers::from_layout(layout),
        &[],
        &test_textures(),
    )
}

// A slider tile at the origin travelling +X at 2 m/s, and a wall with a
// floor in front of it for the other end. `beside_floor` adds a static
// floor a hand's width past the tile's +X edge at rest.
pub(crate) fn tile_wall_layout(beside_floor: bool) -> MapLayout {
    let mut layout = MapLayout {
        walls: vec![Wall {
            x1: -6.0,
            z1: -10.0,
            x2: 6.0,
            z2: -10.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![
            Floor {
                x1: -6.0,
                z1: -10.0,
                x2: 6.0,
                z2: -4.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
                carrier: CarrierId::WORLD,
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
        ],
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::default(),
            to: Position { x: 4.0, y: 0.0, z: 0.0 },
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
        }],
        ..Default::default()
    };
    if beside_floor {
        layout.floors.push(Floor {
            x1: 1.6,
            z1: -3.0,
            x2: 8.0,
            z2: 3.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        });
    }
    layout
}

// The world with the tile at `tick`, its collider placed there.
pub(crate) fn tile_world(layout: &MapLayout, tick: u32) -> (CollisionWorld, Carriers) {
    let mut world = CollisionWorld::from_map_layout(layout, &BarrierKindTable::default());
    let mut carriers = Carriers::from_layout(layout);
    carriers.advance(tick.wrapping_sub(1));
    carriers.advance(tick);
    world.set_carrier_poses(&carriers);
    (world, carriers)
}

pub(crate) fn advance_tile(world: &mut CollisionWorld, carriers: &mut Carriers, set: &mut PortalSet, tick: u32) {
    carriers.advance(tick);
    world.set_carrier_poses(carriers);
    set.refresh(carriers);
}

pub(crate) fn tile_center(carriers: &Carriers) -> Vec3 {
    carriers.pose(TILE).translation
}

// A floor portal on the tile, at the carrier's origin; yaw 0 lays its long
// axis across the slide (along Z), a quarter turn along it.
pub(crate) fn carried_portal(yaw: f32) -> Portal {
    Portal {
        carrier: TILE,
        ..portal(PortalEnd::A, Vec3::ZERO, Vec3::Y, yaw)
    }
}

pub(crate) fn wall_portal() -> Portal {
    portal(PortalEnd::B, Vec3::new(0.0, 1.6, -9.85), Vec3::Z, 0.0)
}

// Runs the shared step from `first_tick` for `ticks`, the tile advancing
// and the gates refreshing before each, and stops at the first hop.
pub(crate) fn run_ticks(
    world: &mut CollisionWorld,
    carriers: &mut Carriers,
    set: &mut PortalSet,
    physics: CharacterPhysicsConfig,
    mut pos: Position,
    first_tick: u32,
    ticks: u32,
) -> (Option<(u32, CharacterPortalHop)>, Position, CharacterSupport) {
    let mut vertical_velocity = 0.0;
    let mut support = CharacterSupport::Airborne;
    for tick in first_tick..first_tick + ticks {
        advance_tile(world, carriers, set, tick);
        let env = CharacterEnvironment {
            ladder_mode: LadderMode::Automatic,
            collision_world: world,
            gravity: 25.0,
            passable_kinds: &[],
            physics,
            ladder_climb_ratio: LADDER_CLIMB_RATIO,
            portals: Some(set),
            carriers,
        };
        let from = pos;
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity,
                control_velocity: Vec3::ZERO,
                external_displacement: Vec3::ZERO,
                delta: TICK_SECS,
            },
            &env,
        );
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
        support = result.support;
        if let Some(hop) = set.character_hop(
            Vec3::from(from),
            Vec3::from(pos),
            physics,
            CharacterHopBody {
                control_velocity: Vec3::ZERO,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity,
                yaw: 0.0,
            },
            CAP,
        ) {
            return (Some((tick, hop)), pos, support);
        }
    }
    (None, pos, support)
}
