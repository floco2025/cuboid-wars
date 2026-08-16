use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};
use std::f32::consts::FRAC_PI_2;

use crate::{
    characters::PreviousTickPosition,
    constants::{
        ITEM_MISSILE_COLOR, MISSILE_BODY_LENGTH, MISSILE_BODY_RADIUS, MISSILE_FIN_LENGTH, MISSILE_FIN_SPAN,
        MISSILE_NOSE_LENGTH,
    },
    missiles::MissileVelocity,
};
use common::protocol::{MissileId, MissileMarker, MissileMovementState};

// ============================================================================
// Resources
// ============================================================================

// Shared handles so every missile instance batches: cylinder body, cone
// nose, one fin mesh holding all four tail fins.
#[derive(Resource)]
pub struct MissileAssets {
    body_mesh: Handle<Mesh>,
    nose_mesh: Handle<Mesh>,
    fins_mesh: Handle<Mesh>,
    body_material: Handle<StandardMaterial>,
    accent_material: Handle<StandardMaterial>,
}

impl FromWorld for MissileAssets {
    fn from_world(world: &mut World) -> Self {
        let brightness = world
            .resource::<crate::config::ClientSettings>()
            .vfx
            .projectiles
            .body_emissive_brightness;
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let body_mesh = meshes.add(Cylinder::new(MISSILE_BODY_RADIUS, MISSILE_BODY_LENGTH));
        let nose_mesh = meshes.add(Cone {
            radius: MISSILE_BODY_RADIUS,
            height: MISSILE_NOSE_LENGTH,
        });
        let fins_mesh = meshes.add(build_fins_mesh());

        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let body_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.78, 0.82),
            metallic: 0.8,
            perceptual_roughness: 0.35,
            ..default()
        });
        let accent_material = materials.add(StandardMaterial {
            base_color: ITEM_MISSILE_COLOR,
            emissive: LinearRgba::rgb(brightness * 0.95, brightness * 0.4, brightness * 0.1),
            ..default()
        });

        Self {
            body_mesh,
            nose_mesh,
            fins_mesh,
            body_material,
            accent_material,
        }
    }
}

// Four triangular trim tabs at the tail, 90° apart, each emitted with both
// windings so it reads from every side. Body-local coordinates (Y-up, body
// centered at the origin) so the child transform stays identity.
fn build_fins_mesh() -> Mesh {
    let tail_y = -MISSILE_BODY_LENGTH / 2.0;
    let template = [
        Vec3::new(MISSILE_BODY_RADIUS, tail_y, 0.0),
        Vec3::new(MISSILE_BODY_RADIUS, tail_y + MISSILE_FIN_LENGTH, 0.0),
        Vec3::new(MISSILE_BODY_RADIUS + MISSILE_FIN_SPAN, tail_y, 0.0),
    ];

    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    for fin in 0..4 {
        let rotation = Quat::from_rotation_y(fin as f32 * FRAC_PI_2);
        let [a, b, c] = template.map(|vertex| rotation * vertex);
        let normal = (rotation * Vec3::Z).to_array();
        let back_normal = (rotation * Vec3::NEG_Z).to_array();
        for vertex in [a, b, c] {
            positions.push(vertex.to_array());
            normals.push(normal);
        }
        for vertex in [a, c, b] {
            positions.push(vertex.to_array());
            normals.push(back_normal);
        }
        uvs.extend([[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

// ============================================================================
// Spawning
// ============================================================================

pub fn spawn_missile(
    commands: &mut Commands,
    assets: &MissileAssets,
    id: MissileId,
    movement: &MissileMovementState,
) -> Entity {
    let pos = movement.pos;
    let velocity = movement.velocity();
    let rotation = missile_rotation(velocity);
    commands
        .spawn((
            MissileMarker,
            id,
            pos,
            PreviousTickPosition(pos),
            MissileVelocity(velocity),
            Transform::from_translation(Vec3::from(pos)).with_rotation(rotation),
            Visibility::default(),
        ))
        .with_children(|parent| {
            spawn_missile_meshes(parent, assets);
        })
        .id()
}

// The same body/nose/fins hierarchy at world scale, for the `missile_pack`
// pickup item — spawned as children of the item root.
pub fn spawn_missile_pickup_visual(parent: &mut ChildSpawnerCommands, assets: &MissileAssets) {
    spawn_missile_meshes(parent, assets);
}

fn spawn_missile_meshes(parent: &mut ChildSpawnerCommands, assets: &MissileAssets) {
    parent.spawn((
        Mesh3d(assets.body_mesh.clone()),
        MeshMaterial3d(assets.body_material.clone()),
        Transform::IDENTITY,
    ));
    parent.spawn((
        Mesh3d(assets.nose_mesh.clone()),
        MeshMaterial3d(assets.accent_material.clone()),
        Transform::from_xyz(0.0, (MISSILE_BODY_LENGTH + MISSILE_NOSE_LENGTH) / 2.0, 0.0),
    ));
    parent.spawn((
        Mesh3d(assets.fins_mesh.clone()),
        MeshMaterial3d(assets.accent_material.clone()),
        Transform::IDENTITY,
    ));
}

// The meshes are authored Y-up; point the nose along the velocity.
#[must_use]
pub fn missile_rotation(velocity: Vec3) -> Quat {
    let dir = velocity.normalize_or_zero();
    if dir == Vec3::ZERO {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_arc(Vec3::Y, dir)
    }
}
