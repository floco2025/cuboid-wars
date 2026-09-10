use bevy::prelude::*;

use crate::vfx::{translucent_kind_material, with_white_vertex_colors};

// The unit quad and cube every translucent field (barriers, light bridges,
// erasers) scales into its panes and frames.
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

// One kind's shared look: its pane and frame materials, the configured
// colour, and the key pickup material of a barrier kind.
pub(crate) struct KindVisual {
    pub surface: Handle<StandardMaterial>,
    pub frame: Handle<StandardMaterial>,
    pub base_color: Color,
    pub key_material: Option<Handle<StandardMaterial>>,
}

impl KindVisual {
    pub fn new(materials: &mut Assets<StandardMaterial>, color: Color, alpha: f32, emissive: f32) -> Self {
        Self {
            surface: materials.add(translucent_kind_material(color, alpha, emissive)),
            frame: materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                ..default()
            }),
            base_color: color,
            key_material: None,
        }
    }
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
