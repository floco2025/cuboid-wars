use bevy::{light::NotShadowCaster, prelude::*};
use common::{config::GameplayConfig, protocol::Position};

use crate::config::{AssetSet, SpriteSheetFirstFrame};

#[derive(Component)]
pub struct ExplosionEffect {
    pub elapsed: f32,
    pub lifetime: f32,
    pub frames: usize,
    pub columns: u32,
    pub rows: u32,
    pub first_frame: SpriteSheetFirstFrame,
    pub current_frame: usize,
    pub mesh: Handle<Mesh>,
}

pub fn spawn_actor_explosion(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_set: &AssetSet,
    gameplay_config: &GameplayConfig,
    pos: Position,
) -> Entity {
    let effect = asset_set.actor_explosion_effect();
    let actor_physics = gameplay_config.characters.actor.physics();
    let frames = (effect.columns * effect.rows) as usize;
    let current_frame = initial_frame(frames, effect.first_frame);
    let mesh = meshes.add(explosion_mesh(effect.scale, effect.columns, effect.rows, current_frame));
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(asset_server.load(effect.image.clone())),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            NotShadowCaster,
            Transform::from_translation(Vec3::new(pos.x, actor_physics.collider_center_y(pos.y), pos.z)),
            PointLight {
                color: Color::srgb(1.0, 0.55, 0.2),
                intensity: 500_000.0,
                range: 7.0,
                radius: 1.0,
                shadows_enabled: false,
                ..default()
            },
            ExplosionEffect {
                elapsed: 0.0,
                lifetime: effect.lifetime,
                frames,
                columns: effect.columns,
                rows: effect.rows,
                first_frame: effect.first_frame,
                current_frame,
                mesh,
            },
        ))
        .id()
}

fn explosion_mesh(scale: f32, columns: u32, rows: u32, frame: usize) -> Mesh {
    let mut mesh = Mesh::from(Rectangle::new(scale, scale));
    set_mesh_uvs(&mut mesh, columns, rows, frame);
    mesh
}

pub fn set_mesh_uvs(mesh: &mut Mesh, columns: u32, rows: u32, frame: usize) {
    let col = frame as u32 % columns;
    let row = frame as u32 / columns;
    let u0 = col as f32 / columns as f32;
    let u1 = (col + 1) as f32 / columns as f32;
    let v0 = row as f32 / rows as f32;
    let v1 = (row + 1) as f32 / rows as f32;
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[u1, v0], [u0, v0], [u0, v1], [u1, v1]]);
}

pub const fn initial_frame(frames: usize, first_frame: SpriteSheetFirstFrame) -> usize {
    match first_frame {
        SpriteSheetFirstFrame::UpperLeft => 0,
        SpriteSheetFirstFrame::LowerRight => frames - 1,
    }
}

pub const fn animation_frame(frame: usize, frames: usize, first_frame: SpriteSheetFirstFrame) -> usize {
    match first_frame {
        SpriteSheetFirstFrame::UpperLeft => frame,
        SpriteSheetFirstFrame::LowerRight => frames - 1 - frame,
    }
}
