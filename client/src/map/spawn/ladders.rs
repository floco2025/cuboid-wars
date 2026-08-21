use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};

use crate::map::MapLevel;
use common::{
    constants::{LADDER_OVERSHOOT, LADDER_WIDTH, LEVEL_HEIGHT},
    protocol::Ladder,
};

// How far the rails sit off the edge plane, into the climb volume. Must
// clear the floor slabs' `WALL_HALF_THICKNESS` overhang past the grid line
// (incl. the rail's own half thickness) or the rails pierce the landing's
// lip; also puts the rungs near the character's face at the climb stand-off.
const RAIL_INSET: f32 = 0.22;
const RAIL_HALF_THICKNESS: f32 = 0.035;
const RUNG_HALF_THICKNESS: f32 = 0.025;
const RUNG_SPACING: f32 = 0.35;

// Marks the ladder entity; `levels` is the spanned storey count so level
// focus can show the ladder from every level it passes through.
#[derive(Component)]
pub struct LadderMarker {
    pub levels: u8,
}

// Spawn one ladder entity from precomputed layout data. The mesh is built
// directly in world space (identity transform), like ramp meshes — every
// ladder has a distinct height anyway, so there is nothing to share.
pub fn spawn_ladder_from_layout(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    ladder: &Ladder,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.44, 0.48),
        metallic: 0.8,
        perceptual_roughness: 0.45,
        ..default()
    });

    commands.spawn((
        LadderMarker { levels: ladder.levels },
        MapLevel(ladder.level),
        Mesh3d(meshes.add(build_ladder_mesh(ladder))),
        MeshMaterial3d(material),
        Transform::IDENTITY,
        Visibility::Visible,
    ));
}

// Two vertical rails at the segment ends plus evenly spaced rungs between
// them, all axis-aligned boxes (ladder segments lie on grid edges). The rails
// poke `LADDER_OVERSHOOT` above the top landing as the step-off affordance,
// matching the climb volume's reach.
fn build_ladder_mesh(ladder: &Ladder) -> Mesh {
    let base_y = f32::from(ladder.level) * LEVEL_HEIGHT;
    let top_y = (f32::from(ladder.level) + f32::from(ladder.levels)) * LEVEL_HEIGHT + LADDER_OVERSHOOT;

    let a = Vec3::new(ladder.x1, base_y, ladder.z1);
    let b = Vec3::new(ladder.x2, base_y, ladder.z2);
    let along = (b - a).normalize_or_zero();
    let normal = Vec3::new(ladder.nx, 0.0, ladder.nz);
    let inset = normal * RAIL_INSET;

    let mut boxes = BoxMeshData::default();

    let rail_half =
        along.abs() * RAIL_HALF_THICKNESS + normal.abs() * RAIL_HALF_THICKNESS + Vec3::Y * ((top_y - base_y) / 2.0);
    let rail_lift = Vec3::Y * ((top_y - base_y) / 2.0);
    boxes.push_box(a + along * RAIL_HALF_THICKNESS + inset + rail_lift, rail_half);
    boxes.push_box(b - along * RAIL_HALF_THICKNESS + inset + rail_lift, rail_half);

    let rung_half =
        along.abs() * (LADDER_WIDTH / 2.0) + normal.abs() * RUNG_HALF_THICKNESS + Vec3::Y * RUNG_HALF_THICKNESS;
    let mid = a.midpoint(b) + inset;
    let mut rung_y = base_y + RUNG_SPACING;
    while rung_y < top_y - RUNG_HALF_THICKNESS {
        boxes.push_box(Vec3::new(mid.x, rung_y, mid.z), rung_half);
        rung_y += RUNG_SPACING;
    }

    boxes.into_mesh()
}

#[derive(Default)]
struct BoxMeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

impl BoxMeshData {
    fn push_box(&mut self, center: Vec3, half: Vec3) {
        let min = center - half;
        let max = center + half;
        let corner = |x: f32, y: f32, z: f32| [x, y, z];
        // Same winding as `tiled_cuboid`; UVs are per-face extents (the
        // material is a flat color, so tiling continuity doesn't matter).
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
        let size = max - min;
        for (normal, [p0, p1, p2, p3]) in faces {
            self.positions.extend_from_slice(&[p0, p1, p2, p0, p2, p3]);
            self.normals.extend_from_slice(&[normal; 6]);
            let (u, v) = if normal[0] != 0.0 {
                (size.z, size.y)
            } else if normal[1] != 0.0 {
                (size.x, size.z)
            } else {
                (size.x, size.y)
            };
            self.uvs
                .extend_from_slice(&[[0.0, 0.0], [u, 0.0], [u, v], [0.0, 0.0], [u, v], [0.0, v]]);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh
    }
}
