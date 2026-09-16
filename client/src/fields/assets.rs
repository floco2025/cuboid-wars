use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};

use crate::{
    constants::{FIELD_FRAME_BODY_TINT, FIELD_FRAME_METALLIC, FIELD_FRAME_ROUGHNESS},
    vfx::{translucent_kind_material, with_white_vertex_colors},
};

// The unit quad the plain fields (erasers, checkpoints) scale into their panes
// and the unit cube every field scales into its frame rails; barrier and
// bridge panes are built per rect by `field_pane_mesh`.
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

// A translucent pane in one colour with a plain unlit frame (checkpoints) or
// glowing rails (erasers).
pub(crate) struct PaneVisual {
    pub surface: Handle<StandardMaterial>,
    pub frame: Handle<StandardMaterial>,
}

impl PaneVisual {
    pub fn plain(materials: &mut Assets<StandardMaterial>, color: Color, alpha: f32, emissive: f32) -> Self {
        Self {
            surface: materials.add(translucent_kind_material(color, alpha, emissive)),
            frame: materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                ..default()
            }),
        }
    }

    pub fn with_rails(
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

// A pane at `rect` in its root's frame (the +Z face of the root): an outer
// ring at the rails and an inner quad inset by `fade_width`, so `UV_1.x` runs
// 0 → 1 from the frame inward, and `UV_0` is the position on the pane plane in
// the carrier's frame in metres, like every map mesh, so the pattern is
// continuous across neighbouring panes and rides a carrier.
pub(crate) fn field_pane_mesh(rect: Rect, root: &Transform, fade_width: f32) -> Mesh {
    let inset = fade_width.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let inner = Rect {
        min: rect.min + inset,
        max: rect.max - inset,
    };
    let corners = |r: Rect| {
        [
            [r.min.x, r.min.y],
            [r.max.x, r.min.y],
            [r.max.x, r.max.y],
            [r.min.x, r.max.y],
        ]
    };
    let mut positions = Vec::with_capacity(8);
    let mut pattern = Vec::with_capacity(8);
    let mut band = Vec::with_capacity(8);
    for (ring, edge) in [(rect, 0.0), (inner, 1.0)] {
        for [x, y] in corners(ring) {
            let carrier_pos = root.translation + root.rotation * Vec3::new(x, y, 0.0);
            positions.push([x, y, 0.0]);
            pattern.push([
                carrier_pos.dot(root.rotation * Vec3::X),
                carrier_pos.dot(root.rotation * Vec3::Y),
            ]);
            band.push([edge, 0.0]);
        }
    }
    let mut indices = Vec::with_capacity(30);
    for i in 0..4u32 {
        let j = (i + 1) % 4;
        indices.extend([i, j, 4 + j, i, 4 + j, 4 + i]);
    }
    indices.extend([4, 5, 6, 4, 6, 7]);
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 8]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, pattern);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, band);
    mesh.insert_indices(Indices::U32(indices));
    with_white_vertex_colors(mesh)
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
