use bevy::prelude::*;

use super::{
    cuboid_mesh::{tiled_cuboid, tiled_wall_surface_meshes},
    geometry_batch::{MapGeometryBatch, MapGeometryKind},
};
use crate::config::AssetSet;
use common::{constants::*, material_rules::FaceMaterials, protocol::*};

#[derive(Clone, Copy)]
enum CardinalDirection {
    North,
    South,
    East,
    West,
}

impl CardinalDirection {
    fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
        }
    }

    fn material_id(self, materials: &FaceMaterials) -> &str {
        match self {
            Self::North => &materials.north,
            Self::South => &materials.south,
            Self::East => &materials.east,
            Self::West => &materials.west,
        }
    }
}

// Spawn a wall segment entity based on a shared `Wall` config.
pub fn batch_wall(batcher: &mut MapGeometryBatch, asset_set: &AssetSet, wall: &Wall) {
    batcher.begin_segment();
    let center_x = f32::midpoint(wall.x1, wall.x2);
    let center_z = f32::midpoint(wall.z1, wall.z2);

    let dx = wall.x2 - wall.x1;
    let dz = wall.z2 - wall.z1;
    let length = dx.hypot(dz);

    // Put length on local X (visible faces will be the ±Z quads after rotation), width on Z is thickness.
    let mesh_size_x = length;
    let mesh_size_z = wall.width;
    let rotation = Quat::from_rotation_y(dz.atan2(dx));

    let level_y = f32::from(wall.level) * LEVEL_HEIGHT;
    let transform = Transform::from_xyz(center_x, level_y + WALL_HEIGHT / 2.0, center_z).with_rotation(rotation);
    let material_ids = asset_set.material_ids_for_wall(wall);

    if material_ids.is_uniform() {
        let material_def = asset_set.material_by_id(material_ids.primary());
        let mesh = tiled_cuboid(mesh_size_x, WALL_HEIGHT, mesh_size_z, material_def.tile_size());
        batcher.add_mesh(
            MapGeometryKind::Wall,
            wall.level,
            material_ids.primary(),
            &mesh,
            transform,
        );
        return;
    }

    let positive_z_direction = cardinal_direction(rotation * Vec3::Z);
    let negative_z_direction = positive_z_direction.opposite();
    let positive_x_direction = cardinal_direction(rotation * Vec3::X);
    let negative_x_direction = positive_x_direction.opposite();
    let positive_x_material_id = positive_x_direction.material_id(&material_ids);
    let negative_x_material_id = negative_x_direction.material_id(&material_ids);
    let positive_z_material_id = positive_z_direction.material_id(&material_ids);
    let negative_z_material_id = negative_z_direction.material_id(&material_ids);
    let positive_x_material_def = asset_set.material_by_id(positive_x_material_id);
    let negative_x_material_def = asset_set.material_by_id(negative_x_material_id);
    let positive_z_material_def = asset_set.material_by_id(positive_z_material_id);
    let negative_z_material_def = asset_set.material_by_id(negative_z_material_id);
    let top_material_def = asset_set.material_by_id(&material_ids.top);
    let bottom_material_def = asset_set.material_by_id(&material_ids.bottom);
    let surface_meshes = tiled_wall_surface_meshes(
        mesh_size_x,
        WALL_HEIGHT,
        mesh_size_z,
        positive_x_material_def.tile_size(),
        negative_x_material_def.tile_size(),
        positive_z_material_def.tile_size(),
        negative_z_material_def.tile_size(),
        top_material_def.tile_size(),
        bottom_material_def.tile_size(),
    );

    batcher.add_mesh(
        MapGeometryKind::Wall,
        wall.level,
        positive_x_material_id,
        &surface_meshes.local_positive_x,
        transform,
    );
    batcher.add_mesh(
        MapGeometryKind::Wall,
        wall.level,
        negative_x_material_id,
        &surface_meshes.local_negative_x,
        transform,
    );
    batcher.add_mesh(
        MapGeometryKind::Wall,
        wall.level,
        positive_z_material_id,
        &surface_meshes.local_positive_z,
        transform,
    );
    batcher.add_mesh(
        MapGeometryKind::Wall,
        wall.level,
        negative_z_material_id,
        &surface_meshes.local_negative_z,
        transform,
    );
    batcher.add_mesh(
        MapGeometryKind::Wall,
        wall.level,
        &material_ids.top,
        &surface_meshes.up,
        transform,
    );
    batcher.add_mesh(
        MapGeometryKind::Wall,
        wall.level,
        &material_ids.bottom,
        &surface_meshes.down,
        transform,
    );
}

fn cardinal_direction(direction: Vec3) -> CardinalDirection {
    if direction.x.abs() > direction.z.abs() {
        if direction.x >= 0.0 {
            CardinalDirection::East
        } else {
            CardinalDirection::West
        }
    } else if direction.z >= 0.0 {
        CardinalDirection::South
    } else {
        CardinalDirection::North
    }
}
