use bevy::{light::NotShadowCaster, prelude::*};
use common::protocol::MapLayout;

use crate::carriers::{CarrierEntities, CarrierStoreys};

#[derive(Component)]
pub struct EraserMarker;

pub fn erasers_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    carriers: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, With<EraserMarker>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if layout.erasers.is_empty() {
        return;
    }
    let panel = meshes.add(Rectangle::new(1.0, 1.0));
    let edge = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let surface = materials.add(StandardMaterial {
        base_color: Color::srgba(0.7, 0.4, 1.0, 0.22),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    let rim = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.6, 1.0),
        unlit: true,
        ..default()
    });
    for eraser in &layout.erasers {
        let dx = eraser.x2 - eraser.x1;
        let dz = eraser.z2 - eraser.z1;
        let width = dx.hypot(dz);
        let height = eraser.height;
        let thickness = eraser.width;
        commands
            .spawn((
                EraserMarker,
                storeys.tag(eraser.carrier, eraser.level, 0),
                ChildOf(carriers.get(eraser.carrier)),
                Transform::from_xyz(
                    f32::midpoint(eraser.x1, eraser.x2),
                    eraser.y + height / 2.0,
                    f32::midpoint(eraser.z1, eraser.z2),
                )
                .with_rotation(Quat::from_rotation_y(-dz.atan2(dx))),
                Visibility::Inherited,
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(panel.clone()),
                    MeshMaterial3d(surface.clone()),
                    Transform::from_scale(Vec3::new(width, height, 1.0)),
                    NotShadowCaster,
                ));
                for (pos, scale) in [
                    (
                        Vec3::new(-width / 2.0, 0.0, 0.0),
                        Vec3::new(thickness, height, thickness),
                    ),
                    (
                        Vec3::new(width / 2.0, 0.0, 0.0),
                        Vec3::new(thickness, height, thickness),
                    ),
                    (
                        Vec3::new(0.0, -height / 2.0, 0.0),
                        Vec3::new(width, thickness, thickness),
                    ),
                    (
                        Vec3::new(0.0, height / 2.0, 0.0),
                        Vec3::new(width, thickness, thickness),
                    ),
                ] {
                    parent.spawn((
                        Mesh3d(edge.clone()),
                        MeshMaterial3d(rim.clone()),
                        Transform::from_translation(pos).with_scale(scale),
                        NotShadowCaster,
                    ));
                }
            });
    }
}
