use bevy::{asset::RenderAssetUsages, light::NotShadowCaster, prelude::*, render::render_resource::PrimitiveTopology};

use crate::constants::{LADDER_RAIL_HALF_THICKNESS, LADDER_RUNG_HALF_THICKNESS, LADDER_RUNG_SPACING};
use crate::map::MapLevel;
use common::{
    constants::{LADDER_OVERSHOOT, LADDER_RAIL_INSET, LADDER_WIDTH, LEVEL_HEIGHT},
    protocol::Ladder,
};

// Marks the ladder entity; `levels` is the spanned storey count so level
// focus can show the ladder from every level it passes through.
#[derive(Component)]
pub struct LadderMarker {
    pub levels: u8,
}

// Spawn one ladder entity from precomputed layout data. The mesh is built
// directly in world space (identity transform), like ramp meshes — every
// ladder has a distinct height anyway, so there is nothing to share. The
// material is shared: one handle (`assets.json::ladder_material`) for all
// ladders.
pub fn spawn_ladder_from_layout(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    tile_size: f32,
    ladder: &Ladder,
) {
    commands.spawn((
        LadderMarker { levels: ladder.levels },
        MapLevel(ladder.level),
        Mesh3d(meshes.add(build_ladder_mesh(ladder, tile_size))),
        MeshMaterial3d(material),
        // Rails and rungs are thinner than a directional-shadow texel: as
        // casters they self-shadow completely and render black even in
        // direct sun.
        NotShadowCaster,
        Transform::IDENTITY,
        Visibility::Visible,
    ));
}

// Two vertical rails at the segment ends plus evenly spaced rungs between
// them, all axis-aligned boxes (ladder segments lie on grid edges). One
// ladder serves both climb sides; the rails sit `LADDER_RAIL_INSET` off the
// edge on the authored side — the same plane the physics holds climbers
// against. The rails poke `LADDER_OVERSHOOT` above the top landing as the
// step-off affordance, matching the climb volume's reach.
fn build_ladder_mesh(ladder: &Ladder, tile_size: f32) -> Mesh {
    let base_y = f32::from(ladder.level) * LEVEL_HEIGHT;
    let top_y = (f32::from(ladder.level) + f32::from(ladder.levels)) * LEVEL_HEIGHT + LADDER_OVERSHOOT;

    let a = Vec3::new(ladder.x1, base_y, ladder.z1);
    let b = Vec3::new(ladder.x2, base_y, ladder.z2);
    let along = (b - a).normalize_or_zero();
    let normal = Vec3::new(ladder.nx, 0.0, ladder.nz);
    let inset = normal * LADDER_RAIL_INSET;

    let mut boxes = BoxMeshData::new(tile_size);

    let rail_half = along.abs() * LADDER_RAIL_HALF_THICKNESS
        + normal.abs() * LADDER_RAIL_HALF_THICKNESS
        + Vec3::Y * ((top_y - base_y) / 2.0);
    let rail_lift = Vec3::Y * ((top_y - base_y) / 2.0);
    boxes.push_box(a + along * LADDER_RAIL_HALF_THICKNESS + inset + rail_lift, rail_half);
    boxes.push_box(b - along * LADDER_RAIL_HALF_THICKNESS + inset + rail_lift, rail_half);

    let rung_half = along.abs() * (LADDER_WIDTH / 2.0)
        + normal.abs() * LADDER_RUNG_HALF_THICKNESS
        + Vec3::Y * LADDER_RUNG_HALF_THICKNESS;
    let mid = a.midpoint(b) + inset;
    let mut rung_y = base_y + LADDER_RUNG_SPACING;
    while rung_y < top_y - LADDER_RUNG_HALF_THICKNESS {
        boxes.push_box(Vec3::new(mid.x, rung_y, mid.z), rung_half);
        rung_y += LADDER_RUNG_SPACING;
    }

    boxes.into_mesh()
}

struct BoxMeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tile_size: f32,
}

impl BoxMeshData {
    fn new(tile_size: f32) -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            tile_size,
        }
    }

    fn push_box(&mut self, center: Vec3, half: Vec3) {
        let min = center - half;
        let max = center + half;
        let corner = |x: f32, y: f32, z: f32| [x, y, z];
        // Same winding as `tiled_cuboid`. UVs are projected from WORLD
        // position (the mesh is world-space), like every map mesh: a
        // centimeters-thin face anchored at uv 0 would sample the same
        // sliver of the texture's edge on every rail and read as one flat
        // color; world anchoring spreads the members across the texture.
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            (
                [1.0, 0.0, 0.0],
                [
                    corner(max.x, min.y, min.z),
                    corner(max.x, max.y, min.z),
                    corner(max.x, max.y, max.z),
                    corner(max.x, min.y, max.z),
                ],
            ),
            (
                [-1.0, 0.0, 0.0],
                [
                    corner(min.x, min.y, max.z),
                    corner(min.x, max.y, max.z),
                    corner(min.x, max.y, min.z),
                    corner(min.x, min.y, min.z),
                ],
            ),
            (
                [0.0, 1.0, 0.0],
                [
                    corner(min.x, max.y, min.z),
                    corner(min.x, max.y, max.z),
                    corner(max.x, max.y, max.z),
                    corner(max.x, max.y, min.z),
                ],
            ),
            (
                [0.0, -1.0, 0.0],
                [
                    corner(min.x, min.y, max.z),
                    corner(min.x, min.y, min.z),
                    corner(max.x, min.y, min.z),
                    corner(max.x, min.y, max.z),
                ],
            ),
            (
                [0.0, 0.0, 1.0],
                [
                    corner(min.x, min.y, max.z),
                    corner(max.x, min.y, max.z),
                    corner(max.x, max.y, max.z),
                    corner(min.x, max.y, max.z),
                ],
            ),
            (
                [0.0, 0.0, -1.0],
                [
                    corner(max.x, min.y, min.z),
                    corner(min.x, min.y, min.z),
                    corner(min.x, max.y, min.z),
                    corner(max.x, max.y, min.z),
                ],
            ),
        ];
        for (normal, [p0, p1, p2, p3]) in faces {
            self.positions.extend_from_slice(&[p0, p1, p2, p0, p2, p3]);
            self.normals.extend_from_slice(&[normal; 6]);
            let (u_axis, v_axis) = if normal[0] != 0.0 {
                (Vec3::new(0.0, 0.0, -normal[0]), Vec3::Y)
            } else if normal[1] != 0.0 {
                (Vec3::new(0.0, 0.0, normal[1]), Vec3::X)
            } else {
                (Vec3::new(normal[2], 0.0, 0.0), Vec3::Y)
            };
            let uv = |p: [f32; 3]| {
                let world = Vec3::from_array(p);
                [world.dot(u_axis) / self.tile_size, world.dot(v_axis) / self.tile_size]
            };
            self.uvs
                .extend_from_slice(&[uv(p0), uv(p1), uv(p2), uv(p0), uv(p2), uv(p3)]);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        // The ladder material carries a normal map, which only renders on
        // meshes with tangents.
        let _ = mesh.generate_tangents();
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::Ladder;

    #[test]
    fn ladder_mesh_has_varied_uvs_and_tangents() {
        let ladder = Ladder {
            x1: -0.5,
            z1: 0.0,
            x2: 0.5,
            z2: 0.0,
            nx: 0.0,
            nz: -1.0,
            level: 0,
            levels: 1,
        };
        let mesh = build_ladder_mesh(&ladder, 0.6);
        let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) => uvs,
            other => panic!("unexpected UV attribute: {other:?}"),
        };
        let min_u = uvs.iter().map(|uv| uv[0]).fold(f32::MAX, f32::min);
        let max_u = uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
        let max_v = uvs.iter().map(|uv| uv[1]).fold(f32::MIN, f32::max);
        // World-anchored: members must spread across the texture, not all
        // sample the same edge sliver.
        assert!(max_u - min_u > 1.0, "UVs bunched: {min_u}..{max_u}");
        assert!(max_v > 1.0, "rail faces should tile vertically: max_v={max_v}");
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(),
            "tangent generation failed"
        );
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .map(bevy::mesh::VertexAttributeValues::len);
        let uv_len = uvs.len();
        assert_eq!(positions, Some(uv_len), "attribute counts diverge");
    }

    #[test]
    fn ladder_mesh_tangents_are_finite_and_nonzero() {
        let ladder = Ladder {
            x1: -0.5,
            z1: 0.0,
            x2: 0.5,
            z2: 0.0,
            nx: 0.0,
            nz: -1.0,
            level: 0,
            levels: 1,
        };
        let mesh = build_ladder_mesh(&ladder, 0.6);
        let tangents = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
            Some(bevy::mesh::VertexAttributeValues::Float32x4(t)) => t,
            other => panic!("unexpected tangent attribute: {other:?}"),
        };
        let mut bad = 0;
        for t in tangents {
            let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
            if !len.is_finite() || len < 0.5 || !t[3].is_finite() || t[3].abs() < 0.5 {
                bad += 1;
            }
        }
        assert_eq!(
            bad,
            0,
            "{bad}/{} degenerate tangents, first: {:?}",
            tangents.len(),
            &tangents[..4]
        );
    }
}
