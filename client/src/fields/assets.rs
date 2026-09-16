use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};

use super::surface::surface_edges;

use crate::{
    constants::{FIELD_FRAME_BODY_TINT, FIELD_FRAME_METALLIC, FIELD_FRAME_ROUGHNESS},
    vfx::{translucent_kind_material, with_white_vertex_colors},
};

// Erasers scale the unit quad; barriers and bridges use `field_pane_mesh`.
#[derive(Resource)]
pub struct FieldMeshes {
    pub panel: Handle<Mesh>,
    pub frame: Handle<Mesh>,
}

impl FieldMeshes {
    pub fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            panel: meshes.add(with_white_vertex_colors(Rectangle::new(1.0, 1.0).into())),
            frame: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        }
    }
}

impl FromWorld for FieldMeshes {
    fn from_world(world: &mut World) -> Self {
        Self::new(&mut world.resource_mut::<Assets<Mesh>>())
    }
}

// The eraser look: a translucent pane in one colour with glowing rails.
pub(crate) struct PaneVisual {
    pub surface: Handle<StandardMaterial>,
    pub frame: Handle<StandardMaterial>,
}

impl PaneVisual {
    pub fn new(
        materials: &mut Assets<StandardMaterial>,
        color: Color,
        alpha: f32,
        emissive: f32,
        rail_emissive: f32,
    ) -> Self {
        Self {
            surface: materials.add(translucent_kind_material(color, alpha, emissive)),
            frame: materials.add(rail_material(color, rail_emissive)),
        }
    }
}

// Lit rails in a darkened tint of `color` that glow in it at `emissive`.
fn rail_material(color: Color, emissive: f32) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(
            linear.red * FIELD_FRAME_BODY_TINT,
            linear.green * FIELD_FRAME_BODY_TINT,
            linear.blue * FIELD_FRAME_BODY_TINT,
        )),
        emissive: LinearRgba::rgb(linear.red * emissive, linear.green * emissive, linear.blue * emissive),
        metallic: FIELD_FRAME_METALLIC,
        perceptual_roughness: FIELD_FRAME_ROUGHNESS,
        ..default()
    }
}

// One barrier or bridge kind's shared look: its frame material, the
// configured colour, and the key pickup material of a barrier kind.
pub(crate) struct KindVisual {
    pub frame: Handle<StandardMaterial>,
    pub base_color: Color,
    pub key_material: Option<Handle<StandardMaterial>>,
}

impl KindVisual {
    pub fn new(materials: &mut Assets<StandardMaterial>, color: Color, rail_emissive: f32) -> Self {
        Self {
            frame: materials.add(rail_material(color, rail_emissive)),
            base_color: color,
            key_material: None,
        }
    }
}

// UV_1 measures distance to the union's outline; mesh partitions do not create rims.
pub(crate) fn field_pane_mesh(surfaces: &[Rect], root: &Transform, fade_width: f32) -> Mesh {
    let edges = surface_edges(surfaces);
    let mut positions = Vec::new();
    let mut pattern = Vec::new();
    let mut band = Vec::new();
    let mut indices = Vec::new();
    for rect in surfaces {
        let nearby: Vec<_> = edges
            .iter()
            .filter(|edge| {
                edge.max.x >= rect.min.x - fade_width
                    && edge.min.x <= rect.max.x + fade_width
                    && edge.max.y >= rect.min.y - fade_width
                    && edge.min.y <= rect.max.y + fade_width
            })
            .collect();
        let coordinates = |axis: usize| {
            let mut values = vec![rect.min[axis], rect.max[axis]];
            if rect.max[axis] - rect.min[axis] < 2.0 * fade_width {
                values.push(rect.center()[axis]);
            }
            for edge in &nearby {
                for end in [edge.min[axis], edge.max[axis]] {
                    for offset in [-fade_width, 0.0, fade_width] {
                        let value = end + offset;
                        if value > rect.min[axis] && value < rect.max[axis] {
                            values.push(value);
                        }
                    }
                }
            }
            values.sort_by(f32::total_cmp);
            values.dedup();
            values
        };
        let xs = coordinates(0);
        let ys = coordinates(1);
        let start = positions.len() as u32;
        for &y in &ys {
            for &x in &xs {
                let point = Vec2::new(x, y);
                let distance = edges
                    .iter()
                    .map(|edge| (point - point.clamp(edge.min, edge.max)).abs().max_element())
                    .fold(f32::INFINITY, f32::min);
                let carrier_pos = root.translation + root.rotation * Vec3::new(x, y, 0.0);
                positions.push([x, y, 0.0]);
                pattern.push([
                    carrier_pos.dot(root.rotation * Vec3::X),
                    carrier_pos.dot(root.rotation * Vec3::Y),
                ]);
                band.push([(distance / fade_width).min(1.0), 0.0]);
            }
        }
        let columns = xs.len() as u32;
        for row in 0..ys.len() as u32 - 1 {
            for column in 0..columns - 1 {
                let i = start + row * columns + column;
                indices.extend([i, i + 1, i + columns + 1, i, i + columns + 1, i + columns]);
            }
        }
    }
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, pattern);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, band);
    mesh.insert_indices(Indices::U32(indices));
    with_white_vertex_colors(mesh)
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
