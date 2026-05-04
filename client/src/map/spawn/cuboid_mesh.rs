use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};

// Build a cuboid mesh with UVs that tile based on a single tile size.
// Maps U to X extent on ±X faces, and to Z extent on ±Z faces; V maps to Y on side faces.
#[must_use]
pub fn tiled_cuboid(size_x: f32, size_y: f32, size_z: f32, tile_size: f32) -> Mesh {
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

pub struct TiledFloorSurfaceMeshes {
    pub north: Mesh,
    pub south: Mesh,
    pub east: Mesh,
    pub west: Mesh,
    pub up: Mesh,
    pub down: Mesh,
}

pub struct TiledWallSurfaceMeshes {
    pub local_positive_x: Mesh,
    pub local_negative_x: Mesh,
    pub local_positive_z: Mesh,
    pub local_negative_z: Mesh,
    pub up: Mesh,
    pub down: Mesh,
}

#[must_use]
pub fn tiled_floor_surface_meshes(
    size_x: f32,
    size_y: f32,
    size_z: f32,
    north_tile_size: f32,
    south_tile_size: f32,
    east_tile_size: f32,
    west_tile_size: f32,
    up_tile_size: f32,
    down_tile_size: f32,
) -> TiledFloorSurfaceMeshes {
    let hx = size_x / 2.0;
    let hy = size_y / 2.0;
    let hz = size_z / 2.0;

    let north_repeat_x = size_x / north_tile_size;
    let north_repeat_y = size_y / north_tile_size;
    let south_repeat_x = size_x / south_tile_size;
    let south_repeat_y = size_y / south_tile_size;
    let east_repeat_z = size_z / east_tile_size;
    let east_repeat_y = size_y / east_tile_size;
    let west_repeat_z = size_z / west_tile_size;
    let west_repeat_y = size_y / west_tile_size;
    let up_repeat_x = size_x / up_tile_size;
    let up_repeat_z = size_z / up_tile_size;
    let down_repeat_x = size_x / down_tile_size;
    let down_repeat_z = size_z / down_tile_size;

    let mut north = SurfaceMeshData::default();
    let mut south = SurfaceMeshData::default();
    let mut east = SurfaceMeshData::default();
    let mut west = SurfaceMeshData::default();
    let mut up = SurfaceMeshData::default();
    let mut down = SurfaceMeshData::default();

    east.push_face(
        [hx, -hy, -hz],
        [hx, hy, -hz],
        [hx, hy, hz],
        [hx, -hy, hz],
        [1.0, 0.0, 0.0],
        [east_repeat_z, 0.0],
        [east_repeat_z, east_repeat_y],
        [0.0, east_repeat_y],
        [0.0, 0.0],
    );

    west.push_face(
        [-hx, -hy, hz],
        [-hx, hy, hz],
        [-hx, hy, -hz],
        [-hx, -hy, -hz],
        [-1.0, 0.0, 0.0],
        [west_repeat_z, 0.0],
        [west_repeat_z, west_repeat_y],
        [0.0, west_repeat_y],
        [0.0, 0.0],
    );

    up.push_face(
        [-hx, hy, -hz],
        [-hx, hy, hz],
        [hx, hy, hz],
        [hx, hy, -hz],
        [0.0, 1.0, 0.0],
        [0.0, 0.0],
        [up_repeat_z, 0.0],
        [up_repeat_z, up_repeat_x],
        [0.0, up_repeat_x],
    );

    down.push_face(
        [-hx, -hy, hz],
        [-hx, -hy, -hz],
        [hx, -hy, -hz],
        [hx, -hy, hz],
        [0.0, -1.0, 0.0],
        [0.0, 0.0],
        [down_repeat_z, 0.0],
        [down_repeat_z, down_repeat_x],
        [0.0, down_repeat_x],
    );

    south.push_face(
        [-hx, -hy, hz],
        [hx, -hy, hz],
        [hx, hy, hz],
        [-hx, hy, hz],
        [0.0, 0.0, 1.0],
        [0.0, 0.0],
        [south_repeat_x, 0.0],
        [south_repeat_x, south_repeat_y],
        [0.0, south_repeat_y],
    );

    north.push_face(
        [hx, -hy, -hz],
        [-hx, -hy, -hz],
        [-hx, hy, -hz],
        [hx, hy, -hz],
        [0.0, 0.0, -1.0],
        [0.0, 0.0],
        [north_repeat_x, 0.0],
        [north_repeat_x, north_repeat_y],
        [0.0, north_repeat_y],
    );

    TiledFloorSurfaceMeshes {
        north: north.into_mesh(),
        south: south.into_mesh(),
        east: east.into_mesh(),
        west: west.into_mesh(),
        up: up.into_mesh(),
        down: down.into_mesh(),
    }
}

#[must_use]
pub fn tiled_wall_surface_meshes(
    size_x: f32,
    size_y: f32,
    size_z: f32,
    positive_x_tile_size: f32,
    negative_x_tile_size: f32,
    positive_z_tile_size: f32,
    negative_z_tile_size: f32,
    up_tile_size: f32,
    down_tile_size: f32,
) -> TiledWallSurfaceMeshes {
    let hx = size_x / 2.0;
    let hy = size_y / 2.0;
    let hz = size_z / 2.0;

    let positive_x_repeat_z = size_z / positive_x_tile_size;
    let positive_x_repeat_y = size_y / positive_x_tile_size;
    let negative_x_repeat_z = size_z / negative_x_tile_size;
    let negative_x_repeat_y = size_y / negative_x_tile_size;
    let positive_z_repeat_x = size_x / positive_z_tile_size;
    let positive_z_repeat_y = size_y / positive_z_tile_size;
    let negative_z_repeat_x = size_x / negative_z_tile_size;
    let negative_z_repeat_y = size_y / negative_z_tile_size;
    let up_repeat_x = size_x / up_tile_size;
    let up_repeat_z = size_z / up_tile_size;
    let down_repeat_x = size_x / down_tile_size;
    let down_repeat_z = size_z / down_tile_size;

    let mut local_positive_x = SurfaceMeshData::default();
    let mut local_negative_x = SurfaceMeshData::default();
    let mut local_positive_z = SurfaceMeshData::default();
    let mut local_negative_z = SurfaceMeshData::default();
    let mut up = SurfaceMeshData::default();
    let mut down = SurfaceMeshData::default();

    local_positive_x.push_face(
        [hx, -hy, -hz],
        [hx, hy, -hz],
        [hx, hy, hz],
        [hx, -hy, hz],
        [1.0, 0.0, 0.0],
        [positive_x_repeat_z, 0.0],
        [positive_x_repeat_z, positive_x_repeat_y],
        [0.0, positive_x_repeat_y],
        [0.0, 0.0],
    );

    local_negative_x.push_face(
        [-hx, -hy, hz],
        [-hx, hy, hz],
        [-hx, hy, -hz],
        [-hx, -hy, -hz],
        [-1.0, 0.0, 0.0],
        [negative_x_repeat_z, 0.0],
        [negative_x_repeat_z, negative_x_repeat_y],
        [0.0, negative_x_repeat_y],
        [0.0, 0.0],
    );

    local_positive_z.push_face(
        [-hx, -hy, hz],
        [hx, -hy, hz],
        [hx, hy, hz],
        [-hx, hy, hz],
        [0.0, 0.0, 1.0],
        [0.0, 0.0],
        [positive_z_repeat_x, 0.0],
        [positive_z_repeat_x, positive_z_repeat_y],
        [0.0, positive_z_repeat_y],
    );
    local_negative_z.push_face(
        [hx, -hy, -hz],
        [-hx, -hy, -hz],
        [-hx, hy, -hz],
        [hx, hy, -hz],
        [0.0, 0.0, -1.0],
        [0.0, 0.0],
        [negative_z_repeat_x, 0.0],
        [negative_z_repeat_x, negative_z_repeat_y],
        [0.0, negative_z_repeat_y],
    );

    up.push_face(
        [-hx, hy, -hz],
        [-hx, hy, hz],
        [hx, hy, hz],
        [hx, hy, -hz],
        [0.0, 1.0, 0.0],
        [0.0, 0.0],
        [up_repeat_z, 0.0],
        [up_repeat_z, up_repeat_x],
        [0.0, up_repeat_x],
    );

    down.push_face(
        [-hx, -hy, hz],
        [-hx, -hy, -hz],
        [hx, -hy, -hz],
        [hx, -hy, hz],
        [0.0, -1.0, 0.0],
        [0.0, 0.0],
        [down_repeat_z, 0.0],
        [down_repeat_z, down_repeat_x],
        [0.0, down_repeat_x],
    );

    TiledWallSurfaceMeshes {
        local_positive_x: local_positive_x.into_mesh(),
        local_negative_x: local_negative_x.into_mesh(),
        local_positive_z: local_positive_z.into_mesh(),
        local_negative_z: local_negative_z.into_mesh(),
        up: up.into_mesh(),
        down: down.into_mesh(),
    }
}

#[derive(Default)]
struct SurfaceMeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

impl SurfaceMeshData {
    fn push_face(
        &mut self,
        p0: [f32; 3],
        p1: [f32; 3],
        p2: [f32; 3],
        p3: [f32; 3],
        normal: [f32; 3],
        uv0: [f32; 2],
        uv1: [f32; 2],
        uv2: [f32; 2],
        uv3: [f32; 2],
    ) {
        self.positions.extend_from_slice(&[p0, p1, p2, p0, p2, p3]);
        self.normals.extend_from_slice(&[normal; 6]);
        self.uvs.extend_from_slice(&[uv0, uv1, uv2, uv0, uv2, uv3]);
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh
    }
}
