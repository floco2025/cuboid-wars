use bevy::{
    asset::{AssetPath, RenderAssetUsages},
    image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::PrimitiveTopology,
};

use crate::{constants::*, markers::*};
use common::{constants::*, protocol::*};

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct WallBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: WallMarker,
}

#[derive(Bundle)]
struct RoofBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: RoofMarker,
}

#[derive(Bundle)]
struct RoofWallBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: RoofWallMarker,
}

#[derive(Bundle)]
struct RampBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: RampMarker,
}

// ============================================================================
// Mesh Helpers
// ============================================================================

// Build a cuboid mesh with UVs that tile based on a single tile size.
// Maps U to X extent on ±X faces, and to Z extent on ±Z faces; V maps to Y on side faces.
fn tiled_cuboid(size_x: f32, size_y: f32, size_z: f32, tile_size: f32) -> Mesh {
    let hx = size_x / 2.0;
    let hy = size_y / 2.0;
    let hz = size_z / 2.0;

    let repeat_x = size_x / tile_size;
    let repeat_y = size_y / tile_size;
    let repeat_z = size_z / tile_size;

    let mut positions = Vec::with_capacity(36);
    let mut normals = Vec::with_capacity(36);
    let mut uvs = Vec::with_capacity(36);

    // Helper to push two triangles (quad) given four corner positions (p0..p3) in CCW order.
    // For UV rotation: pass in uv coordinates that are already arranged for the desired orientation.
    let mut push_face = |p0: [f32; 3],
                         p1: [f32; 3],
                         p2: [f32; 3],
                         p3: [f32; 3],
                         normal: [f32; 3],
                         uv0: [f32; 2],
                         uv1: [f32; 2],
                         uv2: [f32; 2],
                         uv3: [f32; 2]| {
        // Triangle 1: p0, p1, p2
        positions.extend_from_slice(&[p0, p1, p2]);
        normals.extend_from_slice(&[normal; 3]);
        uvs.extend_from_slice(&[uv0, uv1, uv2]);

        // Triangle 2: p0, p2, p3
        positions.extend_from_slice(&[p0, p2, p3]);
        normals.extend_from_slice(&[normal; 3]);
        uvs.extend_from_slice(&[uv0, uv2, uv3]);
    };

    // +X face (rotated 90° clockwise for proper texture orientation)
    push_face(
        [hx, -hy, -hz],
        [hx, hy, -hz],
        [hx, hy, hz],
        [hx, -hy, hz],
        [1.0, 0.0, 0.0],
        [repeat_z, 0.0],      // p0
        [repeat_z, repeat_y], // p1
        [0.0, repeat_y],      // p2
        [0.0, 0.0],           // p3
    );

    // -X face (rotated 90° clockwise for proper texture orientation)
    push_face(
        [-hx, -hy, hz],
        [-hx, hy, hz],
        [-hx, hy, -hz],
        [-hx, -hy, -hz],
        [-1.0, 0.0, 0.0],
        [repeat_z, 0.0],      // p0
        [repeat_z, repeat_y], // p1
        [0.0, repeat_y],      // p2
        [0.0, 0.0],           // p3
    );

    // +Y face (u along X, v along Z)
    push_face(
        [-hx, hy, -hz],
        [-hx, hy, hz],
        [hx, hy, hz],
        [hx, hy, -hz],
        [0.0, 1.0, 0.0],
        [0.0, 0.0],
        [repeat_z, 0.0],
        [repeat_z, repeat_x],
        [0.0, repeat_x],
    );

    // -Y face (u along X, v along Z)
    push_face(
        [-hx, -hy, hz],
        [-hx, -hy, -hz],
        [hx, -hy, -hz],
        [hx, -hy, hz],
        [0.0, -1.0, 0.0],
        [0.0, 0.0],
        [repeat_z, 0.0],
        [repeat_z, repeat_x],
        [0.0, repeat_x],
    );

    // +Z face (u along length X, v along Y)
    push_face(
        [-hx, -hy, hz],
        [hx, -hy, hz],
        [hx, hy, hz],
        [-hx, hy, hz],
        [0.0, 0.0, 1.0],
        [0.0, 0.0],
        [repeat_x, 0.0],
        [repeat_x, repeat_y],
        [0.0, repeat_y],
    );

    // -Z face
    push_face(
        [hx, -hy, -hz],
        [-hx, -hy, -hz],
        [-hx, hy, -hz],
        [hx, hy, -hz],
        [0.0, 0.0, -1.0],
        [0.0, 0.0],
        [repeat_x, 0.0],
        [repeat_x, repeat_y],
        [0.0, repeat_y],
    );

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

// Build ramp meshes split into top (uses floor texture) and sides (use wall texture).
#[allow(clippy::many_single_char_names)]
fn build_ramp_meshes(x1: f32, z1: f32, x2: f32, z2: f32, y_low: f32, y_high: f32) -> (Mesh, Mesh) {
    // Protocol: (x1, z1, y_low) is low corner, (x2, z2, y_high) is high corner
    let min_x = x1.min(x2);
    let max_x = x1.max(x2);
    let min_z = z1.min(z2);
    let max_z = z1.max(z2);

    let slope_axis_x = (x2 - x1).abs() >= (z2 - z1).abs();
    let (y_lo, y_hi) = if y_low <= y_high {
        (y_low, y_high)
    } else {
        (y_high, y_low)
    };
    let tile_top = TEXTURE_FLOOR_TILE_SIZE;
    let tile_side = TEXTURE_WALL_TILE_SIZE;

    // Determine direction: does the ramp go in positive or negative direction?
    let x_direction_positive = x2 > x1; // true if ramp rises in +X direction
    let z_direction_positive = z2 > z1; // true if ramp rises in +Z direction

    // Build vertices: low edge at (x1, z1), high edge at (x2, z2)
    let (a, b, c, d, e, f) = if slope_axis_x {
        // Ramp along X axis: x1 is low edge, x2 is high edge
        (
            [x1, y_lo, min_z], // a: low south
            [x1, y_lo, max_z], // b: low north
            [x2, y_hi, min_z], // c: high south (top)
            [x2, y_hi, max_z], // d: high north (top)
            [x2, y_lo, min_z], // e: high south (bottom)
            [x2, y_lo, max_z], // f: high north (bottom)
        )
    } else {
        // Ramp along Z axis: z1 is low edge, z2 is high edge
        (
            [min_x, y_lo, z1], // a: low west
            [max_x, y_lo, z1], // b: low east
            [min_x, y_hi, z2], // c: high west (top)
            [max_x, y_hi, z2], // d: high east (top)
            [min_x, y_lo, z2], // e: high west (bottom)
            [max_x, y_lo, z2], // f: high east (bottom)
        )
    };

    let mut positions_top = Vec::with_capacity(6);
    let mut normals_top = Vec::with_capacity(6);
    let mut uvs_top = Vec::with_capacity(6);

    let mut positions_side = Vec::with_capacity(12);
    let mut normals_side = Vec::with_capacity(12);
    let mut uvs_side = Vec::with_capacity(12);

    let mut push_top = |p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], uv0: [f32; 2], uv1: [f32; 2], uv2: [f32; 2]| {
        let u = Vec3::new(p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
        let v = Vec3::new(p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]);
        let normal = u.cross(v).normalize_or_zero();

        positions_top.extend_from_slice(&[p0, p1, p2]);
        normals_top.extend_from_slice(&[[normal.x, normal.y, normal.z]; 3]);
        uvs_top.extend_from_slice(&[uv0, uv1, uv2]);
    };

    let mut push_side = |p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], uv0: [f32; 2], uv1: [f32; 2], uv2: [f32; 2]| {
        let u = Vec3::new(p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
        let v = Vec3::new(p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]);
        let normal = u.cross(v).normalize_or_zero();

        positions_side.extend_from_slice(&[p0, p1, p2]);
        normals_side.extend_from_slice(&[[normal.x, normal.y, normal.z]; 3]);
        uvs_side.extend_from_slice(&[uv0, uv1, uv2]);
    };

    // UV helpers: top maps X/Z; vertical faces map horizontal axis to U and height to V.
    let uv_top = |p: [f32; 3]| -> [f32; 2] { [(p[0] - min_x) / tile_top, (p[2] - min_z) / tile_top] };
    let uv_vert_x = |p: [f32; 3]| -> [f32; 2] { [(p[2] - min_z) / tile_side, (p[1] - y_lo) / tile_side] };
    let uv_vert_z = |p: [f32; 3]| -> [f32; 2] { [(p[0] - min_x) / tile_side, (p[1] - y_lo) / tile_side] };

    if slope_axis_x {
        // Top slanted surface - winding order depends on X direction
        if x_direction_positive {
            push_top(a, b, d, uv_top(a), uv_top(b), uv_top(d));
            push_top(a, d, c, uv_top(a), uv_top(d), uv_top(c));
        } else {
            // Reversed direction: flip winding order
            push_top(a, c, d, uv_top(a), uv_top(c), uv_top(d));
            push_top(a, d, b, uv_top(a), uv_top(d), uv_top(b));
        }

        // High vertical face - winding order depends on direction
        if x_direction_positive {
            push_side(e, c, d, uv_vert_x(e), uv_vert_x(c), uv_vert_x(d));
            push_side(e, d, f, uv_vert_x(e), uv_vert_x(d), uv_vert_x(f));
        } else {
            push_side(e, d, c, uv_vert_x(e), uv_vert_x(d), uv_vert_x(c));
            push_side(e, f, d, uv_vert_x(e), uv_vert_x(f), uv_vert_x(d));
        }

        // South and North faces
        if x_direction_positive {
            push_side(a, c, e, uv_vert_z(a), uv_vert_z(c), uv_vert_z(e));
            push_side(b, f, d, uv_vert_z(b), uv_vert_z(f), uv_vert_z(d));
        } else {
            push_side(a, e, c, uv_vert_z(a), uv_vert_z(e), uv_vert_z(c));
            push_side(b, d, f, uv_vert_z(b), uv_vert_z(d), uv_vert_z(f));
        }
    } else {
        // Top slanted surface - winding order depends on Z direction
        if z_direction_positive {
            push_top(a, c, d, uv_top(a), uv_top(c), uv_top(d));
            push_top(a, d, b, uv_top(a), uv_top(d), uv_top(b));
        } else {
            // Reversed direction: flip winding order
            push_top(a, b, d, uv_top(a), uv_top(b), uv_top(d));
            push_top(a, d, c, uv_top(a), uv_top(d), uv_top(c));
        }

        // High vertical face - winding order depends on direction
        if z_direction_positive {
            push_side(e, f, d, uv_vert_z(e), uv_vert_z(f), uv_vert_z(d));
            push_side(e, d, c, uv_vert_z(e), uv_vert_z(d), uv_vert_z(c));
        } else {
            push_side(e, c, d, uv_vert_z(e), uv_vert_z(c), uv_vert_z(d));
            push_side(e, d, f, uv_vert_z(e), uv_vert_z(d), uv_vert_z(f));
        }

        // West and East faces
        if z_direction_positive {
            push_side(a, e, c, uv_vert_x(a), uv_vert_x(e), uv_vert_x(c));
            push_side(b, d, f, uv_vert_x(b), uv_vert_x(d), uv_vert_x(f));
        } else {
            push_side(a, c, e, uv_vert_x(a), uv_vert_x(c), uv_vert_x(e));
            push_side(b, f, d, uv_vert_x(b), uv_vert_x(f), uv_vert_x(d));
        }
    }

    let mut mesh_top = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh_top.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions_top);
    mesh_top.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals_top);
    mesh_top.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs_top);
    let _ = mesh_top.generate_tangents();

    let mut mesh_side = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh_side.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions_side);
    mesh_side.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals_side);
    mesh_side.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs_side);
    let _ = mesh_side.generate_tangents();

    (mesh_top, mesh_side)
}

// ============================================================================
// Map Spawning
// ============================================================================

// Load a texture with repeat addressing so UVs beyond 0..1 tile instead of clamping.
pub fn load_repeating_texture(asset_server: &AssetServer, path: impl Into<AssetPath<'static>>) -> Handle<Image> {
    asset_server.load_with_settings(path, |settings: &mut ImageLoaderSettings| {
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            anisotropy_clamp: 16,
            ..default()
        });
    })
}

pub fn load_repeating_texture_linear(asset_server: &AssetServer, path: impl Into<AssetPath<'static>>) -> Handle<Image> {
    asset_server.load_with_settings(path, |settings: &mut ImageLoaderSettings| {
        settings.is_srgb = false;
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            anisotropy_clamp: 16,
            ..default()
        });
    })
}

// Spawn a wall segment entity based on a shared `Wall` config.
pub fn spawn_wall(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    wall: &Wall,
) {
    use rand::Rng;

    // Calculate wall center and dimensions from corners
    let center_x = f32::midpoint(wall.x1, wall.x2);
    let center_z = f32::midpoint(wall.z1, wall.z2);

    let dx = wall.x2 - wall.x1;
    let dz = wall.z2 - wall.z1;
    let length = dx.hypot(dz);

    // Put length on local X (visible faces will be the ±Z quads after rotation), width on Z is thickness.
    let mesh_size_x = length;
    let mesh_size_z = wall.width;
    let rotation = Quat::from_rotation_y(dz.atan2(dx));

    // Create material based on whether random colors are enabled
    let wall_material = if RANDOM_WALL_COLORS {
        let mut rng = rand::rng();
        StandardMaterial {
            base_color: Color::srgb(
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
            ),
            ..default()
        }
    } else {
        StandardMaterial {
            base_color_texture: Some(load_repeating_texture(asset_server, "textures/wall/albedo.png")),
            normal_map_texture: Some(load_repeating_texture_linear(
                asset_server,
                "textures/wall/normal-dx.png",
            )),
            occlusion_texture: Some(load_repeating_texture_linear(asset_server, "textures/wall/ao.png")),
            metallic_roughness_texture: Some(load_repeating_texture_linear(
                asset_server,
                "textures/wall/metallic-roughness.png",
            )),
            perceptual_roughness: TEXTURE_WALL_ROUGHNESS,
            metallic: TEXTURE_WALL_METALLIC,
            ..default()
        }
    };

    let mut mesh = tiled_cuboid(mesh_size_x, WALL_HEIGHT, mesh_size_z, TEXTURE_WALL_TILE_SIZE);
    let _ = mesh.generate_tangents();

    commands.spawn(WallBundle {
        mesh: Mesh3d(meshes.add(mesh)),
        material: MeshMaterial3d(materials.add(wall_material)),
        transform: Transform::from_xyz(
            center_x,
            WALL_HEIGHT / 2.0, // Lift so bottom is at y=0
            center_z,
        )
        .with_rotation(rotation),
        visibility: Visibility::default(),
        marker: WallMarker,
    });
}

// Spawn a roof wall entity based on a shared `Wall` config.
// Roof walls are normally invisible (only used for collision), but when
// RANDOM_ROOF_WALL_COLORS is enabled, they're rendered with random colors for debugging.
pub fn spawn_roof_wall(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    wall: &Wall,
) {
    // Only spawn visible roof walls when debugging is enabled
    if !RANDOM_ROOF_WALL_COLORS {
        return;
    }

    use rand::Rng;

    // Calculate wall center and dimensions from corners
    let center_x = f32::midpoint(wall.x1, wall.x2);
    let center_z = f32::midpoint(wall.z1, wall.z2);

    let dx = wall.x2 - wall.x1;
    let dz = wall.z2 - wall.z1;
    let length = dx.hypot(dz);

    // Put length on local X, width on Z is thickness.
    let mesh_size_x = length;
    let mesh_size_z = wall.width;
    let rotation = Quat::from_rotation_y(dz.atan2(dx));

    // Create material with random colors for debugging
    let mut rng = rand::rng();
    let roof_wall_material = StandardMaterial {
        base_color: Color::srgb(
            rng.random_range(0.2..1.0),
            rng.random_range(0.2..1.0),
            rng.random_range(0.2..1.0),
        ),
        ..default()
    };

    let mut mesh = tiled_cuboid(mesh_size_x, WALL_HEIGHT, mesh_size_z, TEXTURE_WALL_TILE_SIZE);
    let _ = mesh.generate_tangents();

    commands.spawn(RoofWallBundle {
        mesh: Mesh3d(meshes.add(mesh)),
        material: MeshMaterial3d(materials.add(roof_wall_material)),
        transform: Transform::from_xyz(
            center_x,
            ROOF_HEIGHT + WALL_HEIGHT / 2.0, // Position at roof level
            center_z,
        )
        .with_rotation(rotation),
        visibility: Visibility::default(),
        marker: RoofWallMarker,
    });
}

// Spawn a roof entity based on a shared `Roof` config.
pub fn spawn_roof(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    roof: &Roof,
) {
    use rand::Rng;

    // Calculate roof center and dimensions from corners
    let center_x = f32::midpoint(roof.x1, roof.x2);
    let center_z = f32::midpoint(roof.z1, roof.z2);

    let width = (roof.x2 - roof.x1).abs();
    let depth = (roof.z2 - roof.z1).abs();

    // Create material based on whether random colors are enabled
    let roof_material = if RANDOM_ROOF_COLORS {
        let mut rng = rand::rng();
        StandardMaterial {
            base_color: Color::srgb(
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
            ),
            ..default()
        }
    } else {
        StandardMaterial {
            base_color_texture: Some(load_repeating_texture(asset_server, "textures/roof/albedo.png")),
            normal_map_texture: Some(load_repeating_texture_linear(
                asset_server,
                "textures/roof/normal-dx.png",
            )),
            occlusion_texture: Some(load_repeating_texture_linear(asset_server, "textures/roof/ao.png")),
            metallic_roughness_texture: Some(load_repeating_texture_linear(
                asset_server,
                "textures/roof/metallic-roughness.png",
            )),
            perceptual_roughness: TEXTURE_ROOF_ROUGHNESS,
            metallic: TEXTURE_ROOF_METALLIC,
            ..default()
        }
    };

    // Use the actual aspect ratio to compute tile repeats for square texels
    let mut mesh = tiled_cuboid(width, roof.thickness, depth, TEXTURE_ROOF_TILE_SIZE);
    let _ = mesh.generate_tangents();

    commands.spawn(RoofBundle {
        mesh: Mesh3d(meshes.add(mesh)),
        material: MeshMaterial3d(materials.add(roof_material)),
        transform: Transform::from_xyz(
            center_x,
            WALL_HEIGHT + roof.thickness / 2.0, // Position so bottom of roof sits on top of wall
            center_z,
        ),
        visibility: Visibility::Visible,
        marker: RoofMarker,
    });
}

// Spawn a ramp entity based on shared `Ramp` config.
pub fn spawn_ramp(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    ramp: &Ramp,
) {
    // Build meshes split by material usage
    let (mesh_top, mesh_side) = build_ramp_meshes(ramp.x1, ramp.z1, ramp.x2, ramp.z2, ramp.y1, ramp.y2);

    // Floor material for the ramp top
    let mut top_material = StandardMaterial {
        base_color_texture: Some(load_repeating_texture(asset_server, "textures/ground/albedo.png")),
        normal_map_texture: Some(load_repeating_texture_linear(
            asset_server,
            "textures/ground/normal-dx.png",
        )),
        occlusion_texture: Some(load_repeating_texture_linear(asset_server, "textures/ground/ao.png")),
        metallic_roughness_texture: Some(load_repeating_texture_linear(
            asset_server,
            "textures/ground/metallic-roughness.png",
        )),
        perceptual_roughness: TEXTURE_FLOOR_ROUGHNESS,
        metallic: TEXTURE_FLOOR_METALLIC,
        ..default()
    };
    top_material.alpha_mode = AlphaMode::Opaque;
    top_material.base_color.set_alpha(1.0);

    // Wall material for the ramp sides
    let mut side_material = StandardMaterial {
        base_color_texture: Some(load_repeating_texture(asset_server, "textures/wall/albedo.png")),
        normal_map_texture: Some(load_repeating_texture_linear(
            asset_server,
            "textures/wall/normal-dx.png",
        )),
        occlusion_texture: Some(load_repeating_texture_linear(asset_server, "textures/wall/ao.png")),
        metallic_roughness_texture: Some(load_repeating_texture_linear(
            asset_server,
            "textures/wall/metallic-roughness.png",
        )),
        perceptual_roughness: TEXTURE_WALL_ROUGHNESS,
        metallic: TEXTURE_WALL_METALLIC,
        ..default()
    };
    side_material.alpha_mode = AlphaMode::Opaque;
    side_material.base_color.set_alpha(1.0);

    // Top entity (floor texture)
    commands.spawn(RampBundle {
        mesh: Mesh3d(meshes.add(mesh_top)),
        material: MeshMaterial3d(materials.add(top_material)),
        transform: Transform::default(),
        visibility: Visibility::Visible,
        marker: RampMarker,
    });

    // Side entity (wall texture)
    commands.spawn(RampBundle {
        mesh: Mesh3d(meshes.add(mesh_side)),
        material: MeshMaterial3d(materials.add(side_material)),
        transform: Transform::default(),
        visibility: Visibility::Visible,
        marker: RampMarker,
    });
}
