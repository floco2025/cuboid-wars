use bevy_math::Vec3;
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, Group, InteractionGroups, InteractionTestMode, Pose, QueryFilter,
    Vector,
};

use crate::{
    constants::{BARRIER_HEIGHT, BARRIER_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT},
    map::{RampAxis, ramp_axis},
    protocol::{Barrier, BarrierKindId, Floor, Ramp, Wall},
};

pub(super) const WALL_COLLISION_GROUP: Group = Group::GROUP_1;
pub(super) const FLOOR_COLLISION_GROUP: Group = Group::GROUP_2;
const RAMP_COLLISION_GROUP: Group = Group::GROUP_3;
// Barrier kinds occupy bits 3..31 inclusive (29 slots, matching
// `BARRIER_KIND_MAX`). `BarrierKindId(n)` → bit `3 + n`.
const BARRIER_GROUP_BIT_OFFSET: u32 = 3;
const COLLIDER_KIND_MASK: u128 = 0xff;
const BARRIER_KIND_SHIFT: u32 = 8;

#[must_use]
pub(crate) fn barrier_collision_group(kind: BarrierKindId) -> Group {
    Group::from_bits_retain(1u32 << (BARRIER_GROUP_BIT_OFFSET + u32::from(kind.0)))
}

// Static world geometry that bounces projectiles (walls, floors, ramps).
// Barriers terminate projectiles instead, so they're NOT in this mask.
pub(super) fn world_collision_groups() -> Group {
    WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

pub(super) fn ground_collision_groups() -> Group {
    FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

// Filter for character (player + actor) movement. Starts from world groups
// plus every configured barrier kind, then removes:
//   * `WALL_COLLISION_GROUP` if the player is phasing — phasing is a wall
//     hack, NOT a universal "pass through anything"; barrier groups stay
//   * each kind in `passable_kinds` (the per-player union of held keys
//     and pressure-plate open kinds — caller merges via
//     `crate::physics::passable_barrier_kinds`).
// Actors call with `passable_kinds: &[]` and never get a free pass.
pub(super) fn character_collision_groups(
    has_phasing: bool,
    passable_kinds: &[BarrierKindId],
    all_barriers: Group,
) -> Group {
    let mut groups = world_collision_groups() | all_barriers;
    if has_phasing {
        groups.remove(WALL_COLLISION_GROUP);
    }
    for kind in passable_kinds {
        groups.remove(barrier_collision_group(*kind));
    }
    groups
}

pub(super) fn query_filter(groups: Group) -> QueryFilter<'static> {
    InteractionGroups::new(Group::ALL, groups, InteractionTestMode::And).into()
}

fn collider_interaction_groups(group: Group) -> InteractionGroups {
    InteractionGroups::new(group, Group::ALL, InteractionTestMode::And)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ColliderKind {
    Wall,
    Floor,
    Ramp,
    Barrier,
}

impl ColliderKind {
    fn user_data(self) -> u128 {
        match self {
            Self::Wall => 1,
            Self::Floor => 2,
            Self::Ramp => 3,
            Self::Barrier => 4,
        }
    }

    fn barrier_user_data(kind: BarrierKindId) -> u128 {
        Self::Barrier.user_data() | (u128::from(kind.0) << BARRIER_KIND_SHIFT)
    }

    pub(super) fn barrier_kind_from_user_data(user_data: u128) -> Option<BarrierKindId> {
        (Self::from_user_data(user_data) == Some(Self::Barrier))
            .then_some(BarrierKindId((user_data >> BARRIER_KIND_SHIFT) as u16))
    }

    pub(super) fn from_user_data(user_data: u128) -> Option<Self> {
        match user_data & COLLIDER_KIND_MASK {
            1 => Some(Self::Wall),
            2 => Some(Self::Floor),
            3 => Some(Self::Ramp),
            4 => Some(Self::Barrier),
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
        WALL_HEIGHT / 2.0,
        if is_horizontal { wall_half_thickness } else { dz / 2.0 },
    );
    let center = Vec3::new(
        f32::midpoint(wall.x1, wall.x2),
        f32::from(wall.level).mul_add(LEVEL_HEIGHT, WALL_HEIGHT / 2.0),
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
    let half_thickness = BARRIER_THICKNESS / 2.0;
    let is_horizontal = dx > dz;
    let half_extents = Vec3::new(
        if is_horizontal { dx / 2.0 } else { half_thickness },
        BARRIER_HEIGHT / 2.0,
        if is_horizontal { half_thickness } else { dz / 2.0 },
    );
    let center = Vec3::new(
        f32::midpoint(barrier.x1, barrier.x2),
        f32::from(barrier.level).mul_add(LEVEL_HEIGHT, BARRIER_HEIGHT / 2.0),
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
    fn character_collision_groups_keeps_all_barriers_for_actors() {
        let all = barrier_mask(3);
        let groups = character_collision_groups(false, &[], all);
        assert_eq!(groups & all, all);
        assert!(groups.contains(WALL_COLLISION_GROUP));
    }

    #[test]
    fn character_collision_groups_removes_held_key_kinds() {
        let all = barrier_mask(3);
        let held = [BarrierKindId(1)];
        let groups = character_collision_groups(false, &held, all);
        assert!(!groups.contains(barrier_collision_group(BarrierKindId(1))));
        assert!(groups.contains(barrier_collision_group(BarrierKindId(0))));
        assert!(groups.contains(barrier_collision_group(BarrierKindId(2))));
        assert!(groups.contains(WALL_COLLISION_GROUP));
    }

    #[test]
    fn character_collision_groups_phasing_only_removes_walls_not_barriers() {
        let all = barrier_mask(2);
        let groups = character_collision_groups(true, &[], all);
        assert!(!groups.contains(WALL_COLLISION_GROUP));
        assert_eq!(groups & all, all);
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
        let groups = character_collision_groups(false, &merged, all);
        assert!(!groups.contains(barrier_collision_group(BarrierKindId(1))));
        assert!(!groups.contains(barrier_collision_group(BarrierKindId(3))));
        assert!(groups.contains(barrier_collision_group(BarrierKindId(0))));
        assert!(groups.contains(barrier_collision_group(BarrierKindId(2))));
    }
}
