use bevy_math::Vec3;
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, Group, InteractionGroups, InteractionTestMode, Pose, QueryFilter,
    Vector,
};

use crate::{
    map::{RampAxis, ramp_axis},
    protocol::{
        Barrier, BarrierKindId, BridgeKindId, Floor, KindId, LightBridge, MovingFloor, MovingFloorId, Ramp, Wall,
    },
};

// Rapier's 32 collision groups, split once here: walls/floors/ramps take
// bits 0..2, every light bridge shares bit 3 (a bridge collider is a member
// only while its kind is powered — `CollisionWorld::set_powered_bridges`),
// every moving floor shares bit 4 (a walkable surface that is not world
// geometry: sight passes through it, and only a portal shot sees it, through
// `portal_surface_collision_groups`), and barrier kinds take bits 5..31
// (`BarrierKindId(n)` → bit `5 + n`), which is where `BarrierKindId::MAX`
// comes from.
pub(super) const WALL_COLLISION_GROUP: Group = Group::GROUP_1;
pub(super) const FLOOR_COLLISION_GROUP: Group = Group::GROUP_2;
const RAMP_COLLISION_GROUP: Group = Group::GROUP_3;
pub(super) const BRIDGE_COLLISION_GROUP: Group = Group::GROUP_4;
pub(super) const MOVING_FLOOR_COLLISION_GROUP: Group = Group::GROUP_5;
const BARRIER_GROUP_BIT_OFFSET: u32 = 5;
const _: () = assert!(matches!(BarrierKindId::MAX, Some(max) if BARRIER_GROUP_BIT_OFFSET as usize + max == 32));
const COLLIDER_KIND_MASK: u128 = 0xff;
const KIND_SHIFT: u32 = 8;

#[must_use]
pub(crate) fn barrier_collision_group(kind: BarrierKindId) -> Group {
    Group::from_bits_retain(1u32 << (BARRIER_GROUP_BIT_OFFSET + u32::from(kind.0)))
}

// Static world geometry that bounces projectiles (walls, floors, ramps).
// Barriers terminate projectiles instead, so they're NOT in this mask.
pub(super) fn world_collision_groups() -> Group {
    WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

// The static world plus the powered light bridges and the moving floors:
// what characters stand on and projectiles bounce off right now.
pub(super) fn surface_collision_groups() -> Group {
    world_collision_groups() | BRIDGE_COLLISION_GROUP | MOVING_FLOOR_COLLISION_GROUP
}

// What a portal shot may land on: the static world and the moving floors
// (a portal rides its tile), never a bridge.
pub(super) fn portal_surface_collision_groups() -> Group {
    world_collision_groups() | MOVING_FLOOR_COLLISION_GROUP
}

pub(super) fn ground_collision_groups() -> Group {
    FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

// Filter for character (player + actor) movement. Starts from the surface
// groups plus every configured barrier kind, then removes each kind in
// `passable_kinds`: for players the union of held keys and pressure-plate
// open kinds (`crate::physics::passable_barrier_kinds`), for actors the open
// kinds alone.
pub(super) fn character_collision_groups(passable_kinds: &[BarrierKindId], all_barriers: Group) -> Group {
    let mut groups = surface_collision_groups() | all_barriers;
    for kind in passable_kinds {
        groups.remove(barrier_collision_group(*kind));
    }
    groups
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
    MovingFloor,
}

impl ColliderKind {
    fn user_data(self) -> u128 {
        match self {
            Self::Wall => 1,
            Self::Floor => 2,
            Self::Ramp => 3,
            Self::Barrier => 4,
            Self::Bridge => 5,
            Self::MovingFloor => 6,
        }
    }

    fn barrier_user_data(kind: BarrierKindId) -> u128 {
        Self::Barrier.user_data() | (u128::from(kind.0) << KIND_SHIFT)
    }

    fn bridge_user_data(kind: BridgeKindId) -> u128 {
        Self::Bridge.user_data() | (u128::from(kind.0) << KIND_SHIFT)
    }

    fn moving_floor_user_data(id: MovingFloorId) -> u128 {
        Self::MovingFloor.user_data() | (u128::from(id.0) << KIND_SHIFT)
    }

    pub(super) fn barrier_kind_from_user_data(user_data: u128) -> Option<BarrierKindId> {
        (Self::from_user_data(user_data) == Some(Self::Barrier))
            .then_some(BarrierKindId((user_data >> KIND_SHIFT) as u16))
    }

    pub(super) fn from_user_data(user_data: u128) -> Option<Self> {
        match user_data & COLLIDER_KIND_MASK {
            1 => Some(Self::Wall),
            2 => Some(Self::Floor),
            3 => Some(Self::Ramp),
            4 => Some(Self::Barrier),
            5 => Some(Self::Bridge),
            6 => Some(Self::MovingFloor),
            _ => None,
        }
    }
}

pub(super) fn insert_wall_collider(colliders: &mut ColliderSet, wall: &Wall) -> ColliderHandle {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_thickness = wall.width / 2.0;
    let is_horizontal = dx > dz;
    let half_extents = Vec3::new(
        if is_horizontal { dx / 2.0 } else { wall_half_thickness },
        wall.height / 2.0,
        if is_horizontal { wall_half_thickness } else { dz / 2.0 },
    );
    let center = Vec3::new(
        f32::midpoint(wall.x1, wall.x2),
        wall.y + wall.height / 2.0,
        f32::midpoint(wall.z1, wall.z2),
    );

    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::Wall.user_data(),
        WALL_COLLISION_GROUP,
    )
}

pub(super) fn insert_floor_collider(colliders: &mut ColliderSet, floor: &Floor) -> ColliderHandle {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
    let center = Vec3::new(
        f32::midpoint(min_x, max_x),
        floor.y - floor.thickness / 2.0,
        f32::midpoint(min_z, max_z),
    );
    let half_extents = Vec3::new((max_x - min_x) / 2.0, floor.thickness / 2.0, (max_z - min_z) / 2.0);

    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::Floor.user_data(),
        FLOOR_COLLISION_GROUP,
    )
}

// Barriers mirror walls geometrically (a thin cuboid along a grid edge),
// but with per-kind collision groups so each player's filter can drop
// the kinds they hold keys for.
pub(super) fn insert_barrier_collider(colliders: &mut ColliderSet, barrier: &Barrier) -> ColliderHandle {
    let dx = (barrier.x2 - barrier.x1).abs();
    let dz = (barrier.z2 - barrier.z1).abs();
    let half_thickness = barrier.width / 2.0;
    let is_horizontal = dx > dz;
    let half_extents = Vec3::new(
        if is_horizontal { dx / 2.0 } else { half_thickness },
        barrier.height / 2.0,
        if is_horizontal { half_thickness } else { dz / 2.0 },
    );
    let center = Vec3::new(
        f32::midpoint(barrier.x1, barrier.x2),
        barrier.y + barrier.height / 2.0,
        f32::midpoint(barrier.z1, barrier.z2),
    );
    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::barrier_user_data(barrier.kind),
        barrier_collision_group(barrier.kind),
    )
}

// A light bridge is a floor slab that starts unpowered: a member of no group,
// so no query sees it until `set_powered_bridges` moves it into
// `BRIDGE_COLLISION_GROUP`.
pub(super) fn insert_bridge_collider(colliders: &mut ColliderSet, bridge: &LightBridge) -> ColliderHandle {
    let (min_x, max_x, min_z, max_z) = bridge.bounds_xz();
    let center = Vec3::new(
        f32::midpoint(min_x, max_x),
        bridge.y - bridge.thickness / 2.0,
        f32::midpoint(min_z, max_z),
    );
    let half_extents = Vec3::new((max_x - min_x) / 2.0, bridge.thickness / 2.0, (max_z - min_z) / 2.0);
    insert_cuboid_collider(
        colliders,
        center,
        half_extents,
        ColliderKind::bridge_user_data(bridge.kind),
        Group::empty(),
    )
}

// A moving floor starts at its first end; `CollisionWorld::set_moving_floor_centers`
// moves it every tick.
pub(super) fn insert_moving_floor_collider(
    colliders: &mut ColliderSet,
    floor: &MovingFloor,
    id: MovingFloorId,
) -> ColliderHandle {
    insert_cuboid_collider(
        colliders,
        floor.end1() - Vec3::Y * (floor.thickness / 2.0),
        Vec3::new(floor.half_x, floor.thickness / 2.0, floor.half_z),
        ColliderKind::moving_floor_user_data(id),
        MOVING_FLOOR_COLLISION_GROUP,
    )
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
            .position(Pose::translation(center.x, center.y, center.z))
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
        .user_data(ColliderKind::Ramp.user_data())
        .build();
    Some(colliders.insert(collider))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn barrier_mask(count: u16) -> Group {
        let mut mask = Group::empty();
        for i in 0..count {
            mask |= barrier_collision_group(BarrierKindId(i));
        }
        mask
    }

    #[test]
    fn barrier_collision_group_is_unique_per_kind() {
        let g0 = barrier_collision_group(BarrierKindId(0));
        let g1 = barrier_collision_group(BarrierKindId(1));
        let g2 = barrier_collision_group(BarrierKindId(2));
        assert_ne!(g0, g1);
        assert_ne!(g1, g2);
        assert_ne!(g0, g2);
        // Barrier bits must not overlap reserved world groups.
        assert!((g0 & (WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP)).is_empty());
        assert!((g1 & (WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP)).is_empty());
    }

    #[test]
    fn barrier_user_data_round_trips_kind() {
        let kind = BarrierKindId(7);
        let user_data = ColliderKind::barrier_user_data(kind);
        assert_eq!(ColliderKind::from_user_data(user_data), Some(ColliderKind::Barrier));
        assert_eq!(ColliderKind::barrier_kind_from_user_data(user_data), Some(kind));
    }

    #[test]
    fn bridge_user_data_is_not_a_barrier() {
        let user_data = ColliderKind::bridge_user_data(BridgeKindId(2));
        assert_eq!(ColliderKind::from_user_data(user_data), Some(ColliderKind::Bridge));
        assert_eq!(ColliderKind::barrier_kind_from_user_data(user_data), None);
    }

    #[test]
    fn moving_floor_user_data_carries_its_index() {
        let user_data = ColliderKind::moving_floor_user_data(MovingFloorId(3));
        assert_eq!(ColliderKind::from_user_data(user_data), Some(ColliderKind::MovingFloor));
        assert_eq!(user_data >> KIND_SHIFT, 3);
        assert_eq!(ColliderKind::barrier_kind_from_user_data(user_data), None);
    }

    #[test]
    fn every_barrier_kind_is_disjoint_from_the_shared_surface_groups() {
        let max = u16::try_from(BarrierKindId::MAX.expect("barrier kinds carry no collision-group cap"))
            .expect("barrier kind cap exceeds u16");
        let reserved = surface_collision_groups();
        let mut seen = Group::empty();
        for idx in 0..max {
            let group = barrier_collision_group(BarrierKindId(idx));
            assert!((group & (reserved | seen)).is_empty(), "barrier {idx} overlaps");
            seen |= group;
        }
    }

    #[test]
    fn character_collision_groups_includes_the_bridge_and_moving_floor_groups() {
        let groups = character_collision_groups(&[], barrier_mask(2));
        assert!(groups.contains(BRIDGE_COLLISION_GROUP));
        assert!(groups.contains(MOVING_FLOOR_COLLISION_GROUP));
        assert!(groups.contains(FLOOR_COLLISION_GROUP));
    }

    #[test]
    fn moving_floors_are_surfaces_but_not_world_geometry() {
        assert!(surface_collision_groups().contains(MOVING_FLOOR_COLLISION_GROUP));
        assert!(!world_collision_groups().contains(MOVING_FLOOR_COLLISION_GROUP));
        assert!(!ground_collision_groups().contains(MOVING_FLOOR_COLLISION_GROUP));
    }

    #[test]
    fn portal_surface_groups_include_tiles_but_not_bridges() {
        let groups = portal_surface_collision_groups();
        assert!(groups.contains(MOVING_FLOOR_COLLISION_GROUP));
        assert!(groups.contains(FLOOR_COLLISION_GROUP));
        assert!(!groups.contains(BRIDGE_COLLISION_GROUP));
    }

    #[test]
    fn character_collision_groups_keeps_all_barriers_without_passable_kinds() {
        let all = barrier_mask(3);
        let groups = character_collision_groups(&[], all);
        assert_eq!(groups & all, all);
        assert!(groups.contains(WALL_COLLISION_GROUP));
    }

    #[test]
    fn character_collision_groups_removes_held_key_kinds() {
        let all = barrier_mask(3);
        let held = [BarrierKindId(1)];
        let groups = character_collision_groups(&held, all);
        assert!(!groups.contains(barrier_collision_group(BarrierKindId(1))));
        assert!(groups.contains(barrier_collision_group(BarrierKindId(0))));
        assert!(groups.contains(barrier_collision_group(BarrierKindId(2))));
        assert!(groups.contains(WALL_COLLISION_GROUP));
    }

    // After pressure plates, callers union `held_keys` with `open_kinds` via
    // `passable_barrier_kinds` and pass the result here. Validate that
    // merged input removes BOTH groups — same as if the caller had
    // hand-rolled the union.
    #[test]
    fn character_collision_groups_removes_union_of_held_and_open() {
        let all = barrier_mask(4);
        let held = [BarrierKindId(1)];
        let open = [BarrierKindId(3)];
        let merged = crate::physics::passable_barrier_kinds(&held, &open);
        let groups = character_collision_groups(&merged, all);
        assert!(!groups.contains(barrier_collision_group(BarrierKindId(1))));
        assert!(!groups.contains(barrier_collision_group(BarrierKindId(3))));
        assert!(groups.contains(barrier_collision_group(BarrierKindId(0))));
        assert!(groups.contains(barrier_collision_group(BarrierKindId(2))));
    }
}
