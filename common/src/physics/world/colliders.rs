use std::collections::HashMap;

use bevy_math::Vec3;
use rapier3d::{
    parry::shape::ConvexPolyhedron,
    prelude::{
        Collider, ColliderBuilder, ColliderHandle, ColliderSet, Group, InteractionGroups, InteractionTestMode, Pose,
        QueryFilter, SharedShape, Vector,
    },
};

use crate::{
    constants::PRESSURE_PLATE_HEIGHT,
    map::{DecorationKind, Grounds, ROCK_HULL_SUBDIVISIONS, rock_shape},
    math::{rapier_pose, to_rapier},
    protocol::{Barrier, CarrierId, FieldId, Floor, LightBridge, PressurePlate, Ramp, Wall},
};

pub(super) const WALL_COLLISION_GROUP: Group = Group::GROUP_1;
pub(super) const FLOOR_COLLISION_GROUP: Group = Group::GROUP_2;
const RAMP_COLLISION_GROUP: Group = Group::GROUP_3;
pub(super) const BRIDGE_COLLISION_GROUP: Group = Group::GROUP_4;
pub(super) const BARRIER_COLLISION_GROUP: Group = Group::GROUP_5;

// Field ids and surface material indices share bit 40; the collider tag distinguishes them.
const COLLIDER_KIND_MASK: u128 = 0xff;
const FIELD_SHIFT: u32 = 40;
const ID_MASK: u128 = 0xffff;
const CARRIER_SHIFT: u32 = 24;

// A barrier or light bridge blocks a query unless its field is among the ones
// the caller passes through: those that are off (`SwitchState.open_fields`)
// and, for a body's own movement, those it holds a key to (`passable_fields`).
pub(super) fn field_blocks(collider: &Collider, passable: &[FieldId]) -> bool {
    ColliderKind::field_from_user_data(collider.user_data).is_none_or(|field| !passable.contains(&field))
}

// World geometry that bounces projectiles (walls, floors, ramps), on any
// carrier. Fields terminate projectiles instead, so they're NOT in this
// mask. Which queries see light bridges: projectile bounces, sight,
// rain/scorch/wheel ground probes, portal backing, and the world and wall
// rays are bridge-blind (this mask); character movement, attacks, portal
// shots, missile flight, and the camera arm see the ones that are on
// (`surface_collision_groups`, `character_collision_groups`, each with the
// caller's passable fields).
pub(super) fn world_collision_groups() -> Group {
    WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

// Standable surfaces include light bridges.
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
    Grounds,
    Decoration,
    PressurePlate,
}

impl ColliderKind {
    fn user_data(self, carrier: CarrierId) -> u128 {
        let tag: u128 = match self {
            Self::Wall => 1,
            Self::Floor => 2,
            Self::Ramp => 3,
            Self::Barrier => 4,
            Self::Bridge => 5,
            Self::Grounds => 6,
            Self::Decoration => 7,
            Self::PressurePlate => 8,
        };
        tag | (u128::from(carrier.0) << CARRIER_SHIFT)
    }

    fn field_user_data(self, field: FieldId, carrier: CarrierId) -> u128 {
        self.user_data(carrier) | (u128::from(field.0) << FIELD_SHIFT)
    }

    pub(super) fn field_from_user_data(user_data: u128) -> Option<FieldId> {
        matches!(Self::from_user_data(user_data)?, Self::Barrier | Self::Bridge)
            .then(|| FieldId(((user_data >> FIELD_SHIFT) & ID_MASK) as u16))
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
            6 => Some(Self::Grounds),
            7 => Some(Self::Decoration),
            8 => Some(Self::PressurePlate),
            _ => None,
        }
    }
}

pub(super) fn insert_grounds_colliders(colliders: &mut ColliderSet, grounds: &Grounds) -> Vec<ColliderHandle> {
    let mesh = grounds.mesh();
    let collider = ColliderBuilder::trimesh(mesh.vertices.into_iter().map(to_rapier).collect(), mesh.triangles)
        .expect("grounds mesh contains invalid triangles")
        .user_data(ColliderKind::Grounds.user_data(CarrierId::WORLD))
        .collision_groups(collider_interaction_groups(FLOOR_COLLISION_GROUP))
        .build();
    let mut handles = vec![colliders.insert(collider)];
    let mut hulls = HashMap::new();
    for decoration in grounds.collidable_decorations() {
        let (shape, position) = match decoration.kind {
            DecorationKind::Tree => (
                ColliderBuilder::cylinder(2.0 * decoration.scale.y, 0.32 * decoration.scale.x),
                decoration.position + Vec3::Y * (2.0 * decoration.scale.y),
            ),
            DecorationKind::Rock(class) => {
                let hull = hulls.entry((class, decoration.variant)).or_insert_with(|| {
                    let points: Vec<Vector> = rock_shape(class, decoration.variant, ROCK_HULL_SUBDIVISIONS)
                        .vertices
                        .into_iter()
                        .map(to_rapier)
                        .collect();
                    ConvexPolyhedron::from_convex_hull(&points).expect("rock hull points are coplanar")
                });
                // Parry scales hull normals as if the scale were uniform.
                assert!(
                    decoration.scale.x == decoration.scale.y && decoration.scale.y == decoration.scale.z,
                    "rock scale is not uniform"
                );
                let shape = hull
                    .clone()
                    .scaled(to_rapier(decoration.scale))
                    .expect("rock hull scale is degenerate");
                (
                    ColliderBuilder::new(SharedShape::new(shape))
                        .position(rapier_pose(Vec3::ZERO, decoration.rotation)),
                    decoration.position,
                )
            }
        };
        handles.push(
            colliders.insert(
                shape
                    .translation(to_rapier(position))
                    .user_data(ColliderKind::Decoration.user_data(CarrierId::WORLD))
                    .collision_groups(collider_interaction_groups(FLOOR_COLLISION_GROUP))
                    .build(),
            ),
        );
    }
    handles
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

pub(super) fn insert_pressure_plate_collider(colliders: &mut ColliderSet, plate: &PressurePlate) -> ColliderHandle {
    insert_cuboid_collider(
        colliders,
        Vec3::new(
            plate.center_x,
            plate.center_y + PRESSURE_PLATE_HEIGHT / 2.0,
            plate.center_z,
        ),
        Vec3::new(plate.side / 2.0, PRESSURE_PLATE_HEIGHT / 2.0, plate.side / 2.0),
        ColliderKind::PressurePlate.user_data(plate.carrier),
        FLOOR_COLLISION_GROUP,
    )
}

// Barriers mirror walls geometrically (a thin cuboid along a grid edge),
// tagged with their field so each query can exclude the fields it passes.
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
        ColliderKind::Barrier.field_user_data(barrier.field, barrier.carrier),
        BARRIER_COLLISION_GROUP,
    )
}

// Light bridges mirror floor slabs the same way.
pub(super) fn insert_bridge_collider(colliders: &mut ColliderSet, bridge: &LightBridge) -> ColliderHandle {
    let (center, half_extents) = slab_cuboid(bridge.bounds_xz(), bridge.y, bridge.thickness);
    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::Bridge.field_user_data(bridge.field, bridge.carrier),
        BRIDGE_COLLISION_GROUP,
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
    let points: Vec<_> = ramp.prism().points().map(to_rapier).collect();
    let collider = ColliderBuilder::convex_hull(&points)?
        .collision_groups(collider_interaction_groups(RAMP_COLLISION_GROUP))
        .user_data(ColliderKind::Ramp.user_data(ramp.carrier))
        .build();
    Some(colliders.insert(collider))
}

#[cfg(test)]
#[path = "tests/colliders.rs"]
mod tests;
