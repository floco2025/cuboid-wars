use bevy::prelude::*;

use common::{
    collision::Projectile,
    constants::*,
    markers::ProjectileMarker,
    protocol::*,
    spawning::{ProjectileSpawnInfo, calculate_projectile_spawns},
};

use common::markers::PlayerMarker;

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct ProjectileBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    projectile: Projectile,
    player_id: PlayerId,
    projectile_marker: ProjectileMarker,
}

impl ProjectileBundle {
    fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        position: Vec3,
        direction_yaw: f32,
        direction_pitch: f32,
        shooter_id: PlayerId,
    ) -> Self {
        Self {
            mesh: Mesh3d(meshes.add(Sphere::new(PROJECTILE_RADIUS))),
            material: MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(10.0, 10.0, 0.0),
                emissive: LinearRgba::rgb(10.0, 10.0, 0.0),
                ..default()
            })),
            transform: Transform::from_translation(position),
            projectile: Projectile::new(direction_yaw, direction_pitch),
            player_id: shooter_id,
            projectile_marker: ProjectileMarker,
        }
    }
}

// ============================================================================
// Projectile Spawning
// ============================================================================

// Spawn projectile(s) on whether player has multi-shot power-up
pub fn spawn_projectiles(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    pos: &Position,
    face_dir: f32,
    face_pitch: f32,
    has_multi_shot: bool,
    walls: &[Wall],
    ramps: &[Ramp],
    roofs: &[Roof],
    shooter_id: PlayerId,
) -> usize {
    let spawns = calculate_projectile_spawns(pos, face_dir, face_pitch, has_multi_shot, walls, ramps, roofs);

    for spawn_info in &spawns {
        spawn_single_projectile(commands, meshes, materials, spawn_info, shooter_id);
    }

    spawns.len()
}

// Internal helper to spawn a single projectile
fn spawn_single_projectile(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    spawn_info: &ProjectileSpawnInfo,
    shooter_id: PlayerId,
) {
    let spawn_pos = Vec3::new(spawn_info.position.x, spawn_info.position.y, spawn_info.position.z);

    commands.spawn(ProjectileBundle::new(
        meshes,
        materials,
        spawn_pos,
        spawn_info.direction_yaw,
        spawn_info.direction_pitch,
        shooter_id,
    ));
}

// Spawn a projectile for a player (when receiving shot from server).
pub fn spawn_projectile_for_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_query: &Query<(&PlayerId, &Position, &FaceDirection), With<PlayerMarker>>,
    entity: Entity,
    has_multi_shot: bool,
    walls: &[Wall],
    ramps: &[Ramp],
    roofs: &[Roof],
) {
    // Get player ID, position and face direction for this player entity
    if let Ok((player_id, pos, face_dir)) = player_query.get(entity) {
        spawn_projectiles(
            commands,
            meshes,
            materials,
            pos,
            face_dir.0,
            0.0,
            has_multi_shot,
            walls,
            ramps,
            roofs,
            *player_id,
        );
    }
}
