use bevy::prelude::*;

use crate::vfx::{ExplosionEffect, animation_frame, set_mesh_uvs};

pub fn explosion_effects_system(
    mut commands: Commands,
    time: Res<Time>,
    camera_query: Query<&GlobalTransform, (With<Camera3d>, Without<crate::cameras::RearviewCameraMarker>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(
        Entity,
        &mut ExplosionEffect,
        &GlobalTransform,
        &mut Transform,
        Option<&mut PointLight>,
    )>,
) {
    let camera_pos = camera_query.single().ok().map(GlobalTransform::translation);

    for (entity, mut effect, global_transform, mut transform, maybe_light) in &mut query {
        effect.elapsed += time.delta_secs();
        if effect.elapsed >= effect.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        let progress = (effect.elapsed / effect.lifetime).clamp(0.0, 1.0);
        let frame = (progress * effect.frames as f32).floor() as usize;
        let next_frame = animation_frame(frame.min(effect.frames - 1), effect.frames, effect.first_frame);
        if next_frame != effect.current_frame {
            effect.current_frame = next_frame;
            if let Some(mut mesh) = meshes.get_mut(&effect.mesh) {
                set_mesh_uvs(&mut mesh, effect.columns, effect.rows, next_frame);
            }
        }

        if let Some(mut light) = maybe_light {
            let fade = (1.0 - progress).powi(2);
            light.intensity = effect.light_intensity * fade;
            light.range = 7.0 * fade.max(0.25);
        }

        if let Some(camera_pos) = camera_pos {
            let pos = global_transform.translation();
            let to_camera = camera_pos - pos;
            if to_camera.length_squared() > 0.0001 {
                // Mesh is `cull_mode: None`, so either face renders — just
                // orient the plane perpendicular to the view direction.
                // Swap `up` to avoid the degenerate `look_at` when the
                // camera is nearly straight above/below.
                let up = if to_camera.normalize().y.abs() > 0.999 {
                    Vec3::Z
                } else {
                    Vec3::Y
                };
                transform.look_at(camera_pos, up);
            }
        }
    }
}
