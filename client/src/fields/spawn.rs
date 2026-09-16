use bevy::{light::NotShadowCaster, prelude::*};
use common::protocol::MapLayout;

use super::{FieldMeshes, KindVisual, PaneVisual, VisualField, field_pane_mesh};
use crate::{constants::FIELD_EDGE_FADE_WIDTH, materials::FieldMaterial};

// A plain field (erasers, checkpoints): scaled unit panes and rails in one look.
pub(crate) fn spawn_field_visual(
    parent: &mut ChildSpawnerCommands,
    meshes: &FieldMeshes,
    visual: &PaneVisual,
    field: &VisualField,
    layout: &MapLayout,
) {
    let center = field.rect.center();
    for rect in field.panel_rects(layout) {
        spawn_scaled(parent, &meshes.panel, &visual.surface, rect, center, 1.0);
    }
    spawn_frames(
        parent,
        meshes,
        &visual.frame,
        field.frame_rects(layout),
        center,
        field.thickness,
    );
}

// A barrier or bridge surface: one patterned pane mesh per rect on the shared
// field material, and the kind's rails. `root` is the surface entity's
// transform under its carrier, which the pane pattern coordinates follow.
#[expect(
    clippy::too_many_arguments,
    reason = "one surface threads its meshes, materials, rects, and root"
)]
pub(crate) fn spawn_patterned_surface(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    field_meshes: &FieldMeshes,
    surface: &Handle<FieldMaterial>,
    visual: &KindVisual,
    panels: Vec<Rect>,
    frames: Vec<Rect>,
    center: Vec2,
    thickness: f32,
    root: &Transform,
) {
    for rect in panels {
        let local = Rect {
            min: rect.min - center,
            max: rect.max - center,
        };
        parent.spawn((
            Mesh3d(meshes.add(field_pane_mesh(local, root, FIELD_EDGE_FADE_WIDTH))),
            MeshMaterial3d(surface.clone()),
            NotShadowCaster,
        ));
    }
    spawn_frames(parent, field_meshes, &visual.frame, frames, center, thickness);
}

fn spawn_frames(
    parent: &mut ChildSpawnerCommands,
    meshes: &FieldMeshes,
    material: &Handle<StandardMaterial>,
    frames: Vec<Rect>,
    center: Vec2,
    thickness: f32,
) {
    for rect in frames {
        spawn_scaled(parent, &meshes.frame, material, rect, center, thickness);
    }
}

fn spawn_scaled<M: Material>(
    parent: &mut ChildSpawnerCommands,
    mesh: &Handle<Mesh>,
    material: &Handle<M>,
    rect: Rect,
    center: Vec2,
    depth: f32,
) {
    let center = rect.center() - center;
    parent.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_xyz(center.x, center.y, 0.0).with_scale(Vec3::new(rect.width(), rect.height(), depth)),
        NotShadowCaster,
    ));
}
