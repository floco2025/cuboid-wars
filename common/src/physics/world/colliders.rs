use bevy_math::Vec3;
use rapier3d::prelude::{
    Collider, ColliderBuilder, ColliderHandle, ColliderSet, Group, InteractionGroups, InteractionTestMode, Pose,
    QueryFilter, Vector,
};

use super::shape_cast::FieldKind;

use crate::{
    map::{RampAxis, ramp_axis},
    math::to_rapier,
    protocol::{Barrier, BarrierId, BridgeId, CarrierId, Floor, LightBridge, Ramp, Wall},
};

pub(super) const WALL_COLLISION_GROUP: Group = Group::GROUP_1;
pub(super) const FLOOR_COLLISION_GROUP: Group = Group::GROUP_2;
const RAMP_COLLISION_GROUP: Group = Group::GROUP_3;
pub(super) const BRIDGE_COLLISION_GROUP: Group = Group::GROUP_4;
pub(super) const BARRIER_COLLISION_GROUP: Group = Group::GROUP_5;

// Field IDs and surface material indices share bit 40; the collider tag distinguishes them.
const COLLIDER_KIND_MASK: u128 = 0xff;
const KIND_SHIFT: u32 = 40;
const ID_MASK: u128 = 0xffff;
const CARRIER_SHIFT: u32 = 24;

pub(super) fn barrier_blocks(collider: &Collider, passable: &[BarrierId]) -> bool {
    match ColliderKind::field_kind_from_user_data(collider.user_data) {
        Some(FieldKind::Barrier(id)) => !passable.contains(&id),
        _ => true,
    }
}

// World geometry that bounces projectiles (walls, floors, ramps), on any
// carrier. Barriers terminate projectiles instead, so they're NOT in this
// mask. Which queries see powered bridges: projectile bounces, sight,
// rain/scorch/wheel ground probes, portal backing, and the world and wall
// rays are bridge-blind (this mask); character movement, attacks, portal
// shots, missile flight, and the camera arm see them
// (`surface_collision_groups`, `character_collision_groups`).
pub(super) fn world_collision_groups() -> Group {
    WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

// Standable surfaces include powered bridges.
pub(super) fn surface_collision_groups() -> Group {
    world_collision_groups() | BRIDGE_COLLISION_GROUP
}

pub(super) fn ground_collision_groups() -> Group {
    FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

pub(super) fn character_collision_groups() -> Group {
    surface_collision_groups() | BARRIER_COLLISION_GROUP
}

pub(super) fn query_filter(groups: Group) -> QueryFilter<'static> {
    InteractionGroups::new(Group::ALL, groups, InteractionTestMode::And).into()
}

pub(super) fn collider_interaction_groups(group: Group) -> InteractionGroups {
    InteractionGroups::new(group, Group::ALL, InteractionTestMode::And)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ColliderKind {
    Wall,
    Floor,
    Ramp,
    Barrier,
    Bridge,
}

impl ColliderKind {
    fn user_data(self, carrier: CarrierId) -> u128 {
        let tag: u128 = match self {
            Self::Wall => 1,
            Self::Floor => 2,
            Self::Ramp => 3,
            Self::Barrier => 4,
            Self::Bridge => 5,
        };
        tag | (u128::from(carrier.0) << CARRIER_SHIFT)
    }

    fn barrier_user_data(kind: BarrierId, carrier: CarrierId) -> u128 {
        Self::Barrier.user_data(carrier) | (u128::from(kind.0) << KIND_SHIFT)
    }

    fn bridge_user_data(kind: BridgeId, carrier: CarrierId) -> u128 {
        Self::Bridge.user_data(carrier) | (u128::from(kind.0) << KIND_SHIFT)
    }

    pub(super) fn field_kind_from_user_data(user_data: u128) -> Option<FieldKind> {
        let id = ((user_data >> KIND_SHIFT) & u128::from(u32::MAX)) as u32;
        match Self::from_user_data(user_data)? {
            Self::Barrier => Some(FieldKind::Barrier(BarrierId(id))),
            Self::Bridge => Some(FieldKind::Bridge(BridgeId(id))),
            _ => None,
        }
    }

    pub(super) fn carrier_from_user_data(user_data: u128) -> CarrierId {
        CarrierId(((user_data >> CARRIER_SHIFT) & ID_MASK) as u16)
    }

    pub(super) fn from_user_data(user_data: u128) -> Option<Self> {
        match user_data & COLLIDER_KIND_MASK {
            1 => Some(Self::Wall),
            2 => Some(Self::Floor),
            3 => Some(Self::Ramp),
            4 => Some(Self::Barrier),
            5 => Some(Self::Bridge),
            _ => None,
        }
    }
}

pub(super) fn insert_wall_collider(colliders: &mut ColliderSet, wall: &Wall) -> ColliderHandle {
    let (center, half_extents) = edge_cuboid(wall.x1, wall.z1, wall.x2, wall.z2, wall.width, wall.y, wall.height);
    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::Wall.user_data(wall.carrier),
        WALL_COLLISION_GROUP,
    )
}

pub(super) fn insert_floor_collider(colliders: &mut ColliderSet, floor: &Floor) -> ColliderHandle {
    let (center, half_extents) = slab_cuboid(floor.bounds_xz(), floor.y, floor.thickness);
    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::Floor.user_data(floor.carrier),
        FLOOR_COLLISION_GROUP,
    )
}

// Barriers mirror walls geometrically (a thin cuboid along a grid edge),
// with instance IDs so each player's query can exclude passable barriers.
pub(super) fn insert_barrier_collider(colliders: &mut ColliderSet, barrier: &Barrier) -> ColliderHandle {
    let (center, half_extents) = edge_cuboid(
        barrier.x1,
        barrier.z1,
        barrier.x2,
        barrier.z2,
        barrier.width,
        barrier.y,
        barrier.height,
    );
    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::barrier_user_data(barrier.id, barrier.carrier),
        BARRIER_COLLISION_GROUP,
    )
}

// A light bridge is a floor slab that starts unpowered: a member of no group,
// so no query sees it until `set_powered_bridges` moves it into
// `BRIDGE_COLLISION_GROUP`.
pub(super) fn insert_bridge_collider(colliders: &mut ColliderSet, bridge: &LightBridge) -> ColliderHandle {
    let (center, half_extents) = slab_cuboid(bridge.bounds_xz(), bridge.y, bridge.thickness);
    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::bridge_user_data(bridge.id, bridge.carrier),
        Group::empty(),
    )
}

// Center and half extents of a thin cuboid along a grid edge from
// (x1, z1) to (x2, z2), `width` across it and `height` up from `y`.
fn edge_cuboid(x1: f32, z1: f32, x2: f32, z2: f32, width: f32, y: f32, height: f32) -> (Vec3, Vec3) {
    let dx = (x2 - x1).abs();
    let dz = (z2 - z1).abs();
    let half_thickness = width / 2.0;
    let is_horizontal = dx > dz;
    let half_extents = Vec3::new(
        if is_horizontal { dx / 2.0 } else { half_thickness },
        height / 2.0,
        if is_horizontal { half_thickness } else { dz / 2.0 },
    );
    let center = Vec3::new(f32::midpoint(x1, x2), y + height / 2.0, f32::midpoint(z1, z2));
    (center, half_extents)
}

// Center and half extents of a slab whose walking surface is at `y`,
// `thickness` deep below it, over the (min_x, max_x, min_z, max_z) bounds.
fn slab_cuboid((min_x, max_x, min_z, max_z): (f32, f32, f32, f32), y: f32, thickness: f32) -> (Vec3, Vec3) {
    let center = Vec3::new(
        f32::midpoint(min_x, max_x),
        y - thickness / 2.0,
        f32::midpoint(min_z, max_z),
    );
    let half_extents = Vec3::new((max_x - min_x) / 2.0, thickness / 2.0, (max_z - min_z) / 2.0);
    (center, half_extents)
}

fn insert_cuboid_collider(
    colliders: &mut ColliderSet,
    center: Vec3,
    half_extents: Vec3,
    user_data: u128,
    group: Group,
) -> ColliderHandle {
    colliders.insert(
        ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            .position(Pose::from_translation(to_rapier(center)))
            .collision_groups(collider_interaction_groups(group))
            .user_data(user_data)
            .build(),
    )
}

pub(super) fn insert_ramp_collider(colliders: &mut ColliderSet, ramp: &Ramp) -> Option<ColliderHandle> {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    let (min_y, max_y) = ramp.bounds_y();
    let high_is_second = ramp.y2 >= ramp.y1;
    let points = match ramp_axis(ramp) {
        RampAxis::X => {
            let high_x = if high_is_second { ramp.x2 } else { ramp.x1 };
            vec![
                Vector::new(min_x, min_y, min_z),
                Vector::new(min_x, min_y, max_z),
                Vector::new(max_x, min_y, min_z),
                Vector::new(max_x, min_y, max_z),
                Vector::new(high_x, max_y, min_z),
                Vector::new(high_x, max_y, max_z),
            ]
        }
        RampAxis::Z => {
            let high_z = if high_is_second { ramp.z2 } else { ramp.z1 };
            vec![
                Vector::new(min_x, min_y, min_z),
                Vector::new(max_x, min_y, min_z),
                Vector::new(min_x, min_y, max_z),
                Vector::new(max_x, min_y, max_z),
                Vector::new(min_x, max_y, high_z),
                Vector::new(max_x, max_y, high_z),
            ]
        }
    };

    let collider = ColliderBuilder::convex_hull(&points)?
        .collision_groups(collider_interaction_groups(RAMP_COLLISION_GROUP))
        .user_data(ColliderKind::Ramp.user_data(ramp.carrier))
        .build();
    Some(colliders.insert(collider))
}

#[cfg(test)]
#[path = "tests/colliders.rs"]
mod tests;
