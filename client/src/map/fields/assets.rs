use bevy::prelude::*;

use crate::vfx::{translucent_kind_material, with_white_vertex_colors};

pub(crate) struct FieldMeshes {
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

pub(crate) struct FieldMaterials {
    pub surface: Handle<StandardMaterial>,
    pub frame: Handle<StandardMaterial>,
}

impl FieldMaterials {
    pub fn new(materials: &mut Assets<StandardMaterial>, color: Color, alpha: f32, emissive: f32) -> Self {
        Self {
            surface: materials.add(translucent_kind_material(color, alpha, emissive)),
            frame: materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                ..default()
            }),
        }
    }
}
