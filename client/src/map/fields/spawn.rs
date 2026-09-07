use bevy::{light::NotShadowCaster, prelude::*};
use common::protocol::MapLayout;

use super::{FieldMaterials, FieldMeshes, VisualField};

pub(crate) fn spawn_field_visual(
    parent: &mut ChildSpawnerCommands,
    meshes: &FieldMeshes,
    materials: &FieldMaterials,
    field: &VisualField,
    layout: &MapLayout,
) {
    spawn_framed_surface(
        parent,
        meshes,
        materials,
        field.panel_rects(layout),
        field.frame_rects(layout),
        field.rect.center(),
        field.thickness,
    );
}

pub(crate) fn spawn_framed_surface(
    parent: &mut ChildSpawnerCommands,
    meshes: &FieldMeshes,
    materials: &FieldMaterials,
    panels: Vec<Rect>,
    frames: Vec<Rect>,
    center: Vec2,
    thickness: f32,
) {
    for (rects, mesh, material, depth) in [
        (panels, &meshes.panel, &materials.surface, 1.0),
        (frames, &meshes.frame, &materials.frame, thickness),
    ] {
        for rect in rects {
            let center = rect.center() - center;
            parent.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(center.x, center.y, 0.0).with_scale(Vec3::new(rect.width(), rect.height(), depth)),
                NotShadowCaster,
            ));
        }
    }
}
