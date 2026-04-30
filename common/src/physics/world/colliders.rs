use bevy_math::Vec3;
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, Group, InteractionGroups, InteractionTestMode, Pose, QueryFilter,
    Vector,
};

use crate::{
    constants::{LEVEL_HEIGHT, WALL_HEIGHT},
    map::{RampAxis, ramp_axis},
    protocol::{Floor, Ramp, Wall},
};

pub(super) const WALL_COLLISION_GROUP: Group = Group::GROUP_1;
pub(super) const FLOOR_COLLISION_GROUP: Group = Group::GROUP_2;
const RAMP_COLLISION_GROUP: Group = Group::GROUP_3;

pub(super) fn world_collision_groups() -> Group {
    WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

pub(super) fn character_collision_groups(has_phasing: bool) -> Group {
    let mut groups = world_collision_groups();

    if has_phasing {
        groups.remove(WALL_COLLISION_GROUP);
    }

    groups
}

pub(super) fn query_filter(groups: Group) -> QueryFilter<'static> {
    InteractionGroups::new(Group::ALL, groups, InteractionTestMode::And).into()
}

fn collider_interaction_groups(kind: ColliderKind) -> InteractionGroups {
    InteractionGroups::new(kind.group(), Group::ALL, InteractionTestMode::And)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ColliderKind {
    Wall,
    Floor,
    Ramp,
}

impl ColliderKind {
    fn group(self) -> Group {
        match self {
            Self::Wall => WALL_COLLISION_GROUP,
            Self::Floor => FLOOR_COLLISION_GROUP,
            Self::Ramp => RAMP_COLLISION_GROUP,
        }
    }

    fn user_data(self) -> u128 {
        match self {
            Self::Wall => 1,
            Self::Floor => 2,
            Self::Ramp => 3,
        }
    }

    #[cfg(test)]
    pub(super) fn from_user_data(user_data: u128) -> Option<Self> {
        match user_data {
            1 => Some(Self::Wall),
            2 => Some(Self::Floor),
            3 => Some(Self::Ramp),
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

    insert_cuboid_collider(colliders, center, half_extents, ColliderKind::Wall)
}

pub(super) fn insert_floor_collider(colliders: &mut ColliderSet, floor: &Floor) -> ColliderHandle {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
    let center = Vec3::new(
        f32::midpoint(min_x, max_x),
        floor.y - floor.thickness / 2.0,
        f32::midpoint(min_z, max_z),
    );
    let half_extents = Vec3::new((max_x - min_x) / 2.0, floor.thickness / 2.0, (max_z - min_z) / 2.0);

    insert_cuboid_collider(colliders, center, half_extents, ColliderKind::Floor)
}

fn insert_cuboid_collider(
    colliders: &mut ColliderSet,
    center: Vec3,
    half_extents: Vec3,
    kind: ColliderKind,
) -> ColliderHandle {
    colliders.insert(
        ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            .position(Pose::translation(center.x, center.y, center.z))
            .collision_groups(collider_interaction_groups(kind))
            .user_data(kind.user_data())
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
        .collision_groups(collider_interaction_groups(ColliderKind::Ramp))
        .user_data(ColliderKind::Ramp.user_data())
        .build();
    Some(colliders.insert(collider))
}
