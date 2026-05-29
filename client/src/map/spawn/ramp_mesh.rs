use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};

// Build ramp meshes split into top (uses floor texture) and sides (use wall texture).
#[must_use]
pub fn build_ramp_meshes(
    x1: f32,
    z1: f32,
    x2: f32,
    z2: f32,
    y_low: f32,
    y_high: f32,
    tile_top: f32,
    tile_side: f32,
) -> (Mesh, Mesh) {
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
    let x_direction_positive = x2 > x1;
    let z_direction_positive = z2 > z1;

    let (a, b, c, d, e, f) = if slope_axis_x {
        (
            [x1, y_lo, min_z],
            [x1, y_lo, max_z],
            [x2, y_hi, min_z],
            [x2, y_hi, max_z],
            [x2, y_lo, min_z],
            [x2, y_lo, max_z],
        )
    } else {
        (
            [min_x, y_lo, z1],
            [max_x, y_lo, z1],
            [min_x, y_hi, z2],
            [max_x, y_hi, z2],
            [min_x, y_lo, z2],
            [max_x, y_lo, z2],
        )
    };

    let mut positions_top = Vec::with_capacity(6);
    let mut normals_top = Vec::with_capacity(6);
    let mut uvs_top = Vec::with_capacity(6);

    let mut positions_side = Vec::with_capacity(12);
    let mut normals_side = Vec::with_capacity(12);
    let mut uvs_side = Vec::with_capacity(12);

    let mut push_top = |p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], uv0: [f32; 2], uv1: [f32; 2], uv2: [f32; 2]| {
        let normal = triangle_normal(p0, p1, p2);
        positions_top.extend_from_slice(&[p0, p1, p2]);
        normals_top.extend_from_slice(&[normal; 3]);
        uvs_top.extend_from_slice(&[uv0, uv1, uv2]);
    };

    let mut push_side = |p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], uv0: [f32; 2], uv1: [f32; 2], uv2: [f32; 2]| {
        let normal = triangle_normal(p0, p1, p2);
        positions_side.extend_from_slice(&[p0, p1, p2]);
        normals_side.extend_from_slice(&[normal; 3]);
        uvs_side.extend_from_slice(&[uv0, uv1, uv2]);
    };

    // World-space UVs: each component of the vertex world position divided by
    // the tile size. This keeps the texture continuous with adjacent floors
    // and walls — no seam at the ramp's footprint border. The top face matches
    // the floor's POS_Y axis assignment (U along world Z, V along world X) so a
    // directional texture stays oriented across the ramp-floor seam.
    let uv_top = |p: [f32; 3]| -> [f32; 2] { [p[2] / tile_top, p[0] / tile_top] };
    let uv_vert_x = |p: [f32; 3]| -> [f32; 2] { [p[2] / tile_side, p[1] / tile_side] };
    let uv_vert_z = |p: [f32; 3]| -> [f32; 2] { [p[0] / tile_side, p[1] / tile_side] };

    if slope_axis_x {
        if x_direction_positive {
            push_top(a, b, d, uv_top(a), uv_top(b), uv_top(d));
            push_top(a, d, c, uv_top(a), uv_top(d), uv_top(c));
            push_side(e, c, d, uv_vert_x(e), uv_vert_x(c), uv_vert_x(d));
            push_side(e, d, f, uv_vert_x(e), uv_vert_x(d), uv_vert_x(f));
            push_side(a, c, e, uv_vert_z(a), uv_vert_z(c), uv_vert_z(e));
            push_side(b, f, d, uv_vert_z(b), uv_vert_z(f), uv_vert_z(d));
        } else {
            push_top(a, c, d, uv_top(a), uv_top(c), uv_top(d));
            push_top(a, d, b, uv_top(a), uv_top(d), uv_top(b));
            push_side(e, d, c, uv_vert_x(e), uv_vert_x(d), uv_vert_x(c));
            push_side(e, f, d, uv_vert_x(e), uv_vert_x(f), uv_vert_x(d));
            push_side(a, e, c, uv_vert_z(a), uv_vert_z(e), uv_vert_z(c));
            push_side(b, d, f, uv_vert_z(b), uv_vert_z(d), uv_vert_z(f));
        }
    } else if z_direction_positive {
        push_top(a, c, d, uv_top(a), uv_top(c), uv_top(d));
        push_top(a, d, b, uv_top(a), uv_top(d), uv_top(b));
        push_side(e, f, d, uv_vert_z(e), uv_vert_z(f), uv_vert_z(d));
        push_side(e, d, c, uv_vert_z(e), uv_vert_z(d), uv_vert_z(c));
        push_side(a, e, c, uv_vert_x(a), uv_vert_x(e), uv_vert_x(c));
        push_side(b, d, f, uv_vert_x(b), uv_vert_x(d), uv_vert_x(f));
    } else {
        push_top(a, b, d, uv_top(a), uv_top(b), uv_top(d));
        push_top(a, d, c, uv_top(a), uv_top(d), uv_top(c));
        push_side(e, c, d, uv_vert_z(e), uv_vert_z(c), uv_vert_z(d));
        push_side(e, d, f, uv_vert_z(e), uv_vert_z(d), uv_vert_z(f));
        push_side(a, c, e, uv_vert_x(a), uv_vert_x(c), uv_vert_x(e));
        push_side(b, f, d, uv_vert_x(b), uv_vert_x(f), uv_vert_x(d));
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

fn triangle_normal(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) -> [f32; 3] {
    let u = Vec3::new(p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
    let v = Vec3::new(p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]);
    let normal = u.cross(v).normalize_or_zero();
    [normal.x, normal.y, normal.z]
}
