pub(super) use super::super::{CollisionWorld, colliders::ColliderKind};
pub(super) use crate::{
    physics::characters::character_movement_shape,
    protocol::{
        Barrier, BarrierKindId, BarrierKindTable, BridgeKindId, Carrier, CarrierId, Floor, LightBridge, MapLayout,
        Position, Wall,
    },
    test_geometry::{BARRIER_THICKNESS, BRIDGE_THICKNESS, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS},
};
pub(super) use bevy_math::Vec3;
pub(super) use rapier3d::prelude::Pose;

use crate::{
    config::{CharacterPhysicsConfig, HitboxConfig, MovementColliderConfig},
    protocol::Ramp,
};

pub(crate) fn test_map_layout() -> MapLayout {
    MapLayout {
        walls: vec![Wall {
            x1: 0.0,
            z1: 0.0,
            x2: 4.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 1,
            y: LEVEL_HEIGHT,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: 0.0,
            z1: 0.0,
            x2: 4.0,
            z2: 4.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ramps: vec![Ramp {
            x1: 0.0,
            y1: 0.0,
            z1: 0.0,
            x2: 4.0,
            y2: LEVEL_HEIGHT,
            z2: 8.0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    }
}

// A wall end at the origin, running north (negative z) along x = 0.
pub(crate) fn wall_end_world() -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: 0.0,
                z1: 0.0,
                x2: 0.0,
                z2: -8.0,
                width: WALL_THICKNESS,
                level: 0,
                y: 0.0,
                height: WALL_HEIGHT,
                carrier: CarrierId::WORLD,
            }],
            floors: vec![Floor {
                x1: -8.0,
                z1: -8.0,
                x2: 8.0,
                z2: 8.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        },
        &BarrierKindTable::default(),
    )
}

pub(crate) fn wide_body() -> CharacterPhysicsConfig {
    CharacterPhysicsConfig {
        hitbox: HitboxConfig {
            width: 1.8,
            height: 1.0,
            depth: 1.4,
            bottom_offset: 0.45,
        },
        movement_collider: MovementColliderConfig {
            diameter: 1.8,
            height: 1.8,
        },
    }
}

pub(crate) fn slider_layout() -> MapLayout {
    MapLayout {
        carriers: vec![Carrier {
            switch_inverted: false,

            parent: CarrierId::WORLD,
            level: 1,
            levels: 0,
            from: Position {
                x: 0.0,
                y: LEVEL_HEIGHT,
                z: 0.0,
            },
            to: Position {
                x: 8.0,
                y: LEVEL_HEIGHT,
                z: 0.0,
            },
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        }],
        floors: vec![Floor {
            x1: -1.5,
            z1: -1.5,
            x2: 1.5,
            z2: 1.5,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId(1),
        }],
        ..Default::default()
    }
}
