use bevy::prelude::*;
use rand::{RngExt, rng};

use super::{
    helpers::{tiled_cuboid, tiled_wall_surface_meshes},
    materials::MapMaterialCache,
};
use crate::{config::AssetSet, markers::*};
use common::{assets::DirectionalMaterials, constants::*, protocol::*};

#[derive(Bundle)]
struct WallBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: WallMarker,
}

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

    fn material_id(self, materials: &DirectionalMaterials) -> &str {
        match self {
            Self::North => &materials.north,
            Self::South => &materials.south,
            Self::East => &materials.east,
            Self::West => &materials.west,
        }
    }
}

// Spawn a wall segment entity based on a shared `Wall` config.
pub fn spawn_wall(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    material_cache: &mut MapMaterialCache,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    wall: &Wall,
    debug_colors: bool,
) {
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

    if debug_colors || material_ids.is_uniform() {
        let material_def = asset_set.material_by_id(material_ids.first());
        let wall_material = if debug_colors {
            materials.add(random_debug_material())
        } else {
            material_cache.standard(material_ids.first(), material_def, asset_server, materials)
        };

        let mut mesh = tiled_cuboid(mesh_size_x, WALL_HEIGHT, mesh_size_z, material_def.tile_size());
        let _ = mesh.generate_tangents();
        spawn_wall_mesh(commands, meshes, wall_material, mesh, transform, wall.level);
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

    let positive_x_material =
        material_cache.standard(positive_x_material_id, positive_x_material_def, asset_server, materials);
    let negative_x_material =
        material_cache.standard(negative_x_material_id, negative_x_material_def, asset_server, materials);
    let positive_z_material =
        material_cache.standard(positive_z_material_id, positive_z_material_def, asset_server, materials);
    let negative_z_material =
        material_cache.standard(negative_z_material_id, negative_z_material_def, asset_server, materials);
    let top_material = material_cache.standard(&material_ids.top, top_material_def, asset_server, materials);
    let bottom_material = material_cache.standard(&material_ids.bottom, bottom_material_def, asset_server, materials);

    spawn_wall_mesh(
        commands,
        meshes,
        positive_x_material,
        surface_meshes.local_positive_x,
        transform,
        wall.level,
    );
    spawn_wall_mesh(
        commands,
        meshes,
        negative_x_material,
        surface_meshes.local_negative_x,
        transform,
        wall.level,
    );
    spawn_wall_mesh(
        commands,
        meshes,
        positive_z_material,
        surface_meshes.local_positive_z,
        transform,
        wall.level,
    );
    spawn_wall_mesh(
        commands,
        meshes,
        negative_z_material,
        surface_meshes.local_negative_z,
        transform,
        wall.level,
    );
    spawn_wall_mesh(commands, meshes, top_material, surface_meshes.up, transform, wall.level);
    spawn_wall_mesh(
        commands,
        meshes,
        bottom_material,
        surface_meshes.down,
        transform,
        wall.level,
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

fn spawn_wall_mesh(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    material: Handle<StandardMaterial>,
    mut mesh: Mesh,
    transform: Transform,
    level: u8,
) {
    let _ = mesh.generate_tangents();
    commands.spawn((
        WallBundle {
            mesh: Mesh3d(meshes.add(mesh)),
            material: MeshMaterial3d(material),
            transform,
            visibility: Visibility::default(),
            marker: WallMarker,
        },
        MapLevel(level),
    ));
}

fn random_debug_material() -> StandardMaterial {
    let mut rng = rng();
    StandardMaterial {
        base_color: Color::srgb(
            rng.random_range(0.2..1.0),
            rng.random_range(0.2..1.0),
            rng.random_range(0.2..1.0),
        ),
        ..default()
    }
}
