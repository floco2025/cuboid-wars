use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages, camera::primitives::Aabb, mesh::Indices, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use common::protocol::{CheckpointKind, MapLayout};

use super::checkpoint_paint::CheckpointPaint;
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::ClientSettings,
    constants::{
        CHECKPOINT_BADGE_SIZE, CHECKPOINT_BASE_HEIGHT, CHECKPOINT_BASE_SIZE, CHECKPOINT_COLOR,
        CHECKPOINT_FLAG_WIND_SPEED, CHECKPOINT_FLAG_WIND_STRENGTH, CHECKPOINT_PENNANT_GAP, CHECKPOINT_PENNANT_HEIGHT,
        CHECKPOINT_PENNANT_HEM, CHECKPOINT_PENNANT_HOIST, CHECKPOINT_PENNANT_LENGTH, CHECKPOINT_PENNANT_SEGMENTS,
        CHECKPOINT_PENNANT_WEAVE, CHECKPOINT_POLE_COLOR, CHECKPOINT_POLE_HEIGHT, CHECKPOINT_POLE_RADIUS,
        CHECKPOINT_UNCLAIMED_COLOR, GRASS_WIND_DIRECTION_DEGREES,
    },
    materials::{FlagMaterial, FlagWindExtension},
    players::{MyPlayerId, PlayerMap},
};

#[derive(Component)]
pub struct CheckpointMarker;

// The group's claimed checkpoint from the snapshot, an index into `MapLayout.checkpoints`.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct SharedCheckpoint(pub Option<u16>);

#[derive(Component)]
pub(crate) struct CheckpointPennant {
    index: u16,
}

#[derive(Component)]
pub(crate) struct CheckpointBadge(u16);

#[derive(Resource)]
pub(crate) struct CheckpointAssets {
    badge_unclaimed: Handle<StandardMaterial>,
    badge_claimed: Handle<StandardMaterial>,
    pole: Handle<StandardMaterial>,
    unclaimed: Handle<FlagMaterial>,
    claimed: Handle<FlagMaterial>,
    pole_mesh: Handle<Mesh>,
    pennant_mesh: Handle<Mesh>,
    badge_mesh: Handle<Mesh>,
    base_mesh: Handle<Mesh>,
    cap_mesh: Handle<Mesh>,
}

impl FromWorld for CheckpointAssets {
    fn from_world(world: &mut World) -> Self {
        let emission = world
            .resource::<ClientSettings>()
            .vfx
            .checkpoints
            .flag_emissive_brightness;
        let wind_direction = Vec2::from_angle(GRASS_WIND_DIRECTION_DEGREES.to_radians());
        let wind = FlagWindExtension {
            wind: Vec4::new(
                wind_direction.x,
                wind_direction.y,
                CHECKPOINT_FLAG_WIND_STRENGTH,
                CHECKPOINT_FLAG_WIND_SPEED,
            ),
            cloth: Vec4::new(
                CHECKPOINT_PENNANT_LENGTH,
                CHECKPOINT_PENNANT_HEM,
                CHECKPOINT_PENNANT_HOIST,
                CHECKPOINT_PENNANT_WEAVE,
            ),
        };
        let cloth = |color: Color, emissive: f32| {
            let linear = color.to_linear();
            FlagMaterial {
                base: StandardMaterial {
                    base_color: color,
                    emissive: LinearRgba::rgb(linear.red * emissive, linear.green * emissive, linear.blue * emissive),
                    perceptual_roughness: 0.9,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                },
                extension: wind.clone(),
            }
        };
        let (badge_unclaimed, badge_claimed, pole) = {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            let badge = |color: Color| StandardMaterial {
                base_color: color,
                emissive: color.to_linear() * emission,
                perceptual_roughness: 0.85,
                double_sided: true,
                cull_mode: None,
                ..default()
            };
            (
                materials.add(badge(CHECKPOINT_UNCLAIMED_COLOR)),
                materials.add(badge(CHECKPOINT_COLOR)),
                materials.add(StandardMaterial {
                    base_color: CHECKPOINT_POLE_COLOR,
                    metallic: 0.8,
                    perceptual_roughness: 0.35,
                    ..default()
                }),
            )
        };
        let (unclaimed, claimed) = {
            let mut materials = world.resource_mut::<Assets<FlagMaterial>>();
            (
                materials.add(cloth(CHECKPOINT_UNCLAIMED_COLOR, emission)),
                materials.add(cloth(CHECKPOINT_COLOR, emission)),
            )
        };
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        Self {
            badge_unclaimed,
            badge_claimed,
            pole,
            unclaimed,
            claimed,
            pole_mesh: meshes.add(Cylinder::new(CHECKPOINT_POLE_RADIUS, CHECKPOINT_POLE_HEIGHT)),
            pennant_mesh: meshes.add(pennant_mesh(
                CHECKPOINT_PENNANT_LENGTH,
                CHECKPOINT_PENNANT_HEIGHT,
                CHECKPOINT_PENNANT_SEGMENTS,
            )),
            badge_mesh: meshes.add(group_badge_mesh()),
            base_mesh: meshes.add(Cuboid::new(
                CHECKPOINT_BASE_SIZE,
                CHECKPOINT_BASE_HEIGHT,
                CHECKPOINT_BASE_SIZE,
            )),
            cap_mesh: meshes.add(Sphere::new(CHECKPOINT_POLE_RADIUS * 1.6)),
        }
    }
}

pub(crate) fn pennant_mesh(length: f32, height: f32, segments: usize) -> Mesh {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut metres = Vec::new();
    for column in 0..=segments {
        let reach = column as f32 / segments as f32;
        for row in 0..3 {
            let x = length * reach * if row == 1 { 0.78 } else { 1.0 };
            let y = -height * row as f32 / 2.0;
            positions.push([x, y, 0.0]);
            uvs.push([x / length, row as f32 / 2.0]);
            metres.push([x, y]);
        }
    }
    let mut indices = Vec::new();
    for column in 0..segments as u32 {
        for row in 0..2 {
            let a = column * 3 + row;
            indices.extend([a, a + 1, a + 3, a + 3, a + 1, a + 4]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; positions.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, metres);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn pennant_bounds() -> Aabb {
    // Includes the shader's downwind rotation, maximum gust ripple, and calm droop.
    let ripple = CHECKPOINT_FLAG_WIND_STRENGTH * 2.5;
    let radius = CHECKPOINT_PENNANT_LENGTH + ripple;
    Aabb::from_min_max(
        Vec3::new(
            -radius,
            -CHECKPOINT_PENNANT_HEIGHT - CHECKPOINT_PENNANT_LENGTH * 0.18 - ripple,
            -radius,
        ),
        Vec3::new(radius, ripple, radius),
    )
}

fn group_badge_mesh() -> Mesh {
    let mut mesh: Mesh = Rectangle::new(CHECKPOINT_BADGE_SIZE, CHECKPOINT_BADGE_SIZE).into();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]; 4]);
    for x in [-0.07, 0.07] {
        for (mut icon, y) in [
            (Mesh::from(Circle::new(0.035)), 0.065),
            (Mesh::from(Rectangle::new(0.065, 0.09)), -0.035),
        ] {
            icon.insert_attribute(
                Mesh::ATTRIBUTE_COLOR,
                vec![[0.08, 0.08, 0.08, 1.0]; icon.count_vertices()],
            );
            let front = icon.translated_by(Vec3::new(x, y, 0.001));
            let back = front.clone().translated_by(Vec3::new(0.0, 0.0, -0.002));
            mesh.merge(&front).expect("group badge icon attributes differ");
            mesh.merge(&back).expect("group badge icon attributes differ");
        }
    }
    mesh
}

pub(crate) fn checkpoints_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    carriers: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    mut paint: CheckpointPaint,
    assets: Res<CheckpointAssets>,
    existing: Query<Entity, With<CheckpointMarker>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let mut paint_materials = HashMap::new();
    for (index, checkpoint) in layout.checkpoints.iter().enumerate() {
        let index = u16::try_from(index).expect("checkpoint index exceeds u16");
        let footprint = Rect::new(checkpoint.min_x, checkpoint.min_z, checkpoint.max_x, checkpoint.max_z);
        let center = footprint.center();
        commands
            .spawn((
                CheckpointMarker,
                storeys.tag(checkpoint.carrier, checkpoint.level, 0),
                ChildOf(carriers.get(checkpoint.carrier)),
                Transform::from_xyz(center.x, checkpoint.y, center.y),
                Visibility::Inherited,
            ))
            .with_children(|parent| {
                paint.spawn(parent, checkpoint, &layout, &mut paint_materials);
                parent.spawn((
                    Mesh3d(assets.base_mesh.clone()),
                    MeshMaterial3d(assets.pole.clone()),
                    Transform::from_xyz(0.0, CHECKPOINT_BASE_HEIGHT / 2.0, 0.0),
                ));
                parent.spawn((
                    Mesh3d(assets.cap_mesh.clone()),
                    MeshMaterial3d(assets.pole.clone()),
                    Transform::from_xyz(0.0, CHECKPOINT_POLE_HEIGHT + CHECKPOINT_POLE_RADIUS, 0.0),
                ));
                parent.spawn((
                    Mesh3d(assets.pole_mesh.clone()),
                    MeshMaterial3d(assets.pole.clone()),
                    Transform::from_xyz(0.0, CHECKPOINT_POLE_HEIGHT / 2.0, 0.0),
                ));
                parent.spawn((
                    CheckpointPennant { index },
                    Mesh3d(assets.pennant_mesh.clone()),
                    pennant_bounds(),
                    MeshMaterial3d(assets.unclaimed.clone()),
                    Transform::from_xyz(0.0, CHECKPOINT_POLE_HEIGHT, 0.0),
                ));
                if checkpoint.kind != CheckpointKind::Individual {
                    parent.spawn((
                        CheckpointBadge(index),
                        Mesh3d(assets.badge_mesh.clone()),
                        MeshMaterial3d(assets.badge_unclaimed.clone()),
                        Transform::from_xyz(
                            0.0,
                            CHECKPOINT_POLE_HEIGHT
                                - CHECKPOINT_PENNANT_HEIGHT
                                - CHECKPOINT_PENNANT_GAP
                                - CHECKPOINT_BADGE_SIZE / 2.0,
                            0.0,
                        )
                        .with_rotation(Quat::from_rotation_y(-GRASS_WIND_DIRECTION_DEGREES.to_radians())),
                    ));
                }
            });
    }
}

// Recolours each pennant as the player's saved checkpoint and the group's claim change.
pub(crate) fn checkpoint_pennants_system(
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    shared: Res<SharedCheckpoint>,
    assets: Res<CheckpointAssets>,
    mut pennants: Query<(&CheckpointPennant, &mut MeshMaterial3d<FlagMaterial>)>,
    mut badges: Query<(&CheckpointBadge, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let mine = players.get(&my_player_id.0).and_then(|info| info.checkpoint);
    for (pennant, mut material) in &mut pennants {
        let wanted = if mine == Some(pennant.index) {
            &assets.claimed
        } else {
            &assets.unclaimed
        };
        // A material write re-extracts the pennant; an equal one would do that every frame.
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
    for (badge, mut material) in &mut badges {
        let wanted = if shared.0 == Some(badge.0) {
            &assets.badge_claimed
        } else {
            &assets.badge_unclaimed
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

#[cfg(test)]
#[path = "tests/checkpoints.rs"]
mod tests;
