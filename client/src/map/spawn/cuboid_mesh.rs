use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};

// UVs are computed from each vertex's WORLD position so textures tile
// continuously across adjacent segments — no visible seam at the boundary
// between two cells, two walls, or a floor and the wall above it.
//
// The mesh itself is still emitted in local coordinates (the caller positions
// it via `Transform`). For each face we just project the world position onto
// the face's UV axes:
//   world_pos = world_center + rotation * local_pos
//   uv = (world_pos · u_axis_world, world_pos · v_axis_world) / tile_size
//
// `u_axis_local` / `v_axis_local` are unit directions in the *unrotated* mesh
// that match the existing texture orientation per face.

fn face_uv(
    local: [f32; 3],
    world_center: Vec3,
    rotation: Quat,
    u_axis_local: Vec3,
    v_axis_local: Vec3,
    tile_size: f32,
) -> [f32; 2] {
    let world = world_center + rotation * Vec3::from_array(local);
    let u_axis_world = rotation * u_axis_local;
    let v_axis_world = rotation * v_axis_local;
    [world.dot(u_axis_world) / tile_size, world.dot(v_axis_world) / tile_size]
}

// Face spec: which local axes the UV runs along, for one of the six cuboid
// faces. The directions are chosen to preserve the original texture
// orientation from before world-space UVs were introduced.
struct FaceUvAxes {
    u: Vec3,
    v: Vec3,
}

const POS_X_FACE: FaceUvAxes = FaceUvAxes { u: Vec3::new(0.0, 0.0, -1.0), v: Vec3::new(0.0, 1.0, 0.0) };
const NEG_X_FACE: FaceUvAxes = FaceUvAxes { u: Vec3::new(0.0, 0.0, 1.0), v: Vec3::new(0.0, 1.0, 0.0) };
const POS_Y_FACE: FaceUvAxes = FaceUvAxes { u: Vec3::new(0.0, 0.0, 1.0), v: Vec3::new(1.0, 0.0, 0.0) };
const NEG_Y_FACE: FaceUvAxes = FaceUvAxes { u: Vec3::new(0.0, 0.0, -1.0), v: Vec3::new(1.0, 0.0, 0.0) };
const POS_Z_FACE: FaceUvAxes = FaceUvAxes { u: Vec3::new(1.0, 0.0, 0.0), v: Vec3::new(0.0, 1.0, 0.0) };
const NEG_Z_FACE: FaceUvAxes = FaceUvAxes { u: Vec3::new(-1.0, 0.0, 0.0), v: Vec3::new(0.0, 1.0, 0.0) };

// Build a cuboid mesh with world-aligned UV tiling. `world_center` and
// `rotation` describe where the mesh will be placed (must match the caller's
// `Transform`); UVs use that to compute world positions for each vertex.
#[must_use]
pub fn tiled_cuboid(
    size_x: f32,
    size_y: f32,
    size_z: f32,
    tile_size: f32,
    world_center: Vec3,
    rotation: Quat,
) -> Mesh {
    let hx = size_x / 2.0;
    let hy = size_y / 2.0;
    let hz = size_z / 2.0;

    let mut positions = Vec::with_capacity(36);
    let mut normals = Vec::with_capacity(36);
    let mut uvs = Vec::with_capacity(36);

    let mut push_face = |p0: [f32; 3],
                         p1: [f32; 3],
                         p2: [f32; 3],
                         p3: [f32; 3],
                         normal: [f32; 3],
                         axes: &FaceUvAxes| {
        let uv = |p: [f32; 3]| face_uv(p, world_center, rotation, axes.u, axes.v, tile_size);
        positions.extend_from_slice(&[p0, p1, p2, p0, p2, p3]);
        normals.extend_from_slice(&[normal; 6]);
        uvs.extend_from_slice(&[uv(p0), uv(p1), uv(p2), uv(p0), uv(p2), uv(p3)]);
    };

    push_face(
        [hx, -hy, -hz], [hx, hy, -hz], [hx, hy, hz], [hx, -hy, hz],
        [1.0, 0.0, 0.0],
        &POS_X_FACE,
    );
    push_face(
        [-hx, -hy, hz], [-hx, hy, hz], [-hx, hy, -hz], [-hx, -hy, -hz],
        [-1.0, 0.0, 0.0],
        &NEG_X_FACE,
    );
    push_face(
        [-hx, hy, -hz], [-hx, hy, hz], [hx, hy, hz], [hx, hy, -hz],
        [0.0, 1.0, 0.0],
        &POS_Y_FACE,
    );
    push_face(
        [-hx, -hy, hz], [-hx, -hy, -hz], [hx, -hy, -hz], [hx, -hy, hz],
        [0.0, -1.0, 0.0],
        &NEG_Y_FACE,
    );
    push_face(
        [-hx, -hy, hz], [hx, -hy, hz], [hx, hy, hz], [-hx, hy, hz],
        [0.0, 0.0, 1.0],
        &POS_Z_FACE,
    );
    push_face(
        [hx, -hy, -hz], [-hx, -hy, -hz], [-hx, hy, -hz], [hx, hy, -hz],
        [0.0, 0.0, -1.0],
        &NEG_Z_FACE,
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
    world_center: Vec3,
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
    let rotation = Quat::IDENTITY;

    let mut north = SurfaceMeshData::default();
    let mut south = SurfaceMeshData::default();
    let mut east = SurfaceMeshData::default();
    let mut west = SurfaceMeshData::default();
    let mut up = SurfaceMeshData::default();
    let mut down = SurfaceMeshData::default();

    east.push_face_world(
        [hx, -hy, -hz], [hx, hy, -hz], [hx, hy, hz], [hx, -hy, hz],
        [1.0, 0.0, 0.0],
        &POS_X_FACE, world_center, rotation, east_tile_size,
    );
    west.push_face_world(
        [-hx, -hy, hz], [-hx, hy, hz], [-hx, hy, -hz], [-hx, -hy, -hz],
        [-1.0, 0.0, 0.0],
        &NEG_X_FACE, world_center, rotation, west_tile_size,
    );
    up.push_face_world(
        [-hx, hy, -hz], [-hx, hy, hz], [hx, hy, hz], [hx, hy, -hz],
        [0.0, 1.0, 0.0],
        &POS_Y_FACE, world_center, rotation, up_tile_size,
    );
    down.push_face_world(
        [-hx, -hy, hz], [-hx, -hy, -hz], [hx, -hy, -hz], [hx, -hy, hz],
        [0.0, -1.0, 0.0],
        &NEG_Y_FACE, world_center, rotation, down_tile_size,
    );
    south.push_face_world(
        [-hx, -hy, hz], [hx, -hy, hz], [hx, hy, hz], [-hx, hy, hz],
        [0.0, 0.0, 1.0],
        &POS_Z_FACE, world_center, rotation, south_tile_size,
    );
    north.push_face_world(
        [hx, -hy, -hz], [-hx, -hy, -hz], [-hx, hy, -hz], [hx, hy, -hz],
        [0.0, 0.0, -1.0],
        &NEG_Z_FACE, world_center, rotation, north_tile_size,
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
    world_center: Vec3,
    rotation: Quat,
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

    let mut local_positive_x = SurfaceMeshData::default();
    let mut local_negative_x = SurfaceMeshData::default();
    let mut local_positive_z = SurfaceMeshData::default();
    let mut local_negative_z = SurfaceMeshData::default();
    let mut up = SurfaceMeshData::default();
    let mut down = SurfaceMeshData::default();

    local_positive_x.push_face_world(
        [hx, -hy, -hz], [hx, hy, -hz], [hx, hy, hz], [hx, -hy, hz],
        [1.0, 0.0, 0.0],
        &POS_X_FACE, world_center, rotation, positive_x_tile_size,
    );
    local_negative_x.push_face_world(
        [-hx, -hy, hz], [-hx, hy, hz], [-hx, hy, -hz], [-hx, -hy, -hz],
        [-1.0, 0.0, 0.0],
        &NEG_X_FACE, world_center, rotation, negative_x_tile_size,
    );
    local_positive_z.push_face_world(
        [-hx, -hy, hz], [hx, -hy, hz], [hx, hy, hz], [-hx, hy, hz],
        [0.0, 0.0, 1.0],
        &POS_Z_FACE, world_center, rotation, positive_z_tile_size,
    );
    local_negative_z.push_face_world(
        [hx, -hy, -hz], [-hx, -hy, -hz], [-hx, hy, -hz], [hx, hy, -hz],
        [0.0, 0.0, -1.0],
        &NEG_Z_FACE, world_center, rotation, negative_z_tile_size,
    );
    up.push_face_world(
        [-hx, hy, -hz], [-hx, hy, hz], [hx, hy, hz], [hx, hy, -hz],
        [0.0, 1.0, 0.0],
        &POS_Y_FACE, world_center, rotation, up_tile_size,
    );
    down.push_face_world(
        [-hx, -hy, hz], [-hx, -hy, -hz], [hx, -hy, -hz], [hx, -hy, hz],
        [0.0, -1.0, 0.0],
        &NEG_Y_FACE, world_center, rotation, down_tile_size,
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
    #[allow(clippy::too_many_arguments)]
    fn push_face_world(
        &mut self,
        p0: [f32; 3],
        p1: [f32; 3],
        p2: [f32; 3],
        p3: [f32; 3],
        normal: [f32; 3],
        axes: &FaceUvAxes,
        world_center: Vec3,
        rotation: Quat,
        tile_size: f32,
    ) {
        let uv = |p: [f32; 3]| face_uv(p, world_center, rotation, axes.u, axes.v, tile_size);
        self.positions.extend_from_slice(&[p0, p1, p2, p0, p2, p3]);
        self.normals.extend_from_slice(&[normal; 6]);
        self.uvs.extend_from_slice(&[uv(p0), uv(p1), uv(p2), uv(p0), uv(p2), uv(p3)]);
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh
    }
}
