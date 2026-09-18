use std::collections::HashMap;

use bevy::{ecs::system::SystemParam, light::NotShadowCaster, prelude::*};
use common::protocol::{Checkpoint, MapLayout, MapSettings};

use super::{clip_surface_rects, surface_frame_rects};
use crate::{
    config::{AssetSet, ClientSettings},
    constants::{CHECKPOINT_COLOR, CHECKPOINT_OUTLINE_WIDTH, CHECKPOINT_PAINT_OFFSET},
    map::spawn::tiled_floor_top_mesh,
    materials::MaterialTextures,
};

#[derive(SystemParam)]
pub(crate) struct CheckpointPaint<'w> {
    settings: Res<'w, MapSettings>,
    client: Res<'w, ClientSettings>,
    assets: Res<'w, AssetSet>,
    textures: ResMut<'w, MaterialTextures>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

impl CheckpointPaint<'_> {
    pub fn spawn(
        &mut self,
        parent: &mut ChildSpawnerCommands,
        checkpoint: &Checkpoint,
        layout: &MapLayout,
        cache: &mut HashMap<String, Handle<StandardMaterial>>,
    ) {
        let footprint = Rect::new(checkpoint.min_x, checkpoint.min_z, checkpoint.max_x, checkpoint.max_z);
        let center = footprint.center();
        let outline = surface_frame_rects(&[footprint], CHECKPOINT_OUTLINE_WIDTH);
        let outline = clip_surface_rects(
            outline,
            layout,
            checkpoint.carrier,
            [0, 2],
            checkpoint.y + CHECKPOINT_PAINT_OFFSET,
            0.0,
        );
        let map_materials = self.assets.map_materials(&self.settings.textures);
        for (floor, faces) in layout.floors.iter().zip(&layout.floor_materials) {
            if floor.carrier != checkpoint.carrier || (floor.y - checkpoint.y).abs() > 0.0001 {
                continue;
            }
            let (x1, x2, z1, z2) = floor.bounds_xz();
            let support = Rect::new(x1, z1, x2, z2);
            let rectangles: Vec<_> = outline
                .iter()
                .filter_map(|rect| {
                    let clipped = rect.intersect(support);
                    (clipped.width() > 0.0 && clipped.height() > 0.0).then_some([
                        clipped.min.x,
                        clipped.min.y,
                        clipped.max.x,
                        clipped.max.y,
                    ])
                })
                .collect();
            if rectangles.is_empty() {
                continue;
            }
            let definition = map_materials.get(&faces.top);
            let material = cache.entry(faces.top.clone()).or_insert_with(|| {
                let mut paint = definition.standard_material(
                    &mut self.textures,
                    self.client.rendering.texture_anisotropy,
                    self.client.rendering.mipmaps,
                );
                paint.base_color = CHECKPOINT_COLOR;
                paint.base_color_texture = None;
                paint.metallic = 0.0;
                paint.perceptual_roughness = 0.85;
                paint.metallic_roughness_texture = None;
                paint.emissive = CHECKPOINT_COLOR.to_linear() * self.client.vfx.checkpoints.paint_emissive_brightness;
                self.materials.add(paint)
            });
            let mut mesh = tiled_floor_top_mesh(
                &rectangles,
                checkpoint.y + CHECKPOINT_PAINT_OFFSET,
                Vec3::new(center.x, checkpoint.y, center.y),
                definition.tile_size(),
            );
            mesh.generate_tangents()
                .expect("checkpoint paint mesh has invalid tangent geometry");
            parent.spawn((
                Mesh3d(self.meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                NotShadowCaster,
            ));
        }
    }
}
