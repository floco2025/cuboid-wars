use bevy::prelude::*;

use common::{
    constants::*,
    markers::ProjectileMarker,
    physics::{CollisionWorld, ProjectileMotion},
    protocol::*,
    spawning::{ProjectileSpawnInfo, calculate_projectile_spawns},
};

// ============================================================================
// Resources
// ============================================================================

#[derive(Resource)]
pub struct ProjectileAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

impl FromWorld for ProjectileAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Sphere::new(PROJECTILE_RADIUS));
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(projectile_material());

        Self { mesh, material }
    }
}

fn projectile_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(10.0, 10.0, 0.0),
        emissive: LinearRgba::rgb(10.0, 10.0, 0.0),
        ..default()
    }
}

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct ProjectileBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    proj_motion: ProjectileMotion,
    proj_marker: ProjectileMarker,
    player_id: PlayerId,
}

impl ProjectileBundle {
    fn new(
        projectile_assets: &ProjectileAssets,
        position: Vec3,
        direction_yaw: f32,
        direction_pitch: f32,
        shooter_id: PlayerId,
    ) -> Self {
        Self {
            mesh: Mesh3d(projectile_assets.mesh.clone()),
            material: MeshMaterial3d(projectile_assets.material.clone()),
            transform: Transform::from_translation(position),
            proj_motion: ProjectileMotion::new(direction_yaw, direction_pitch),
            player_id: shooter_id,
            proj_marker: ProjectileMarker,
        }
    }
}

// ============================================================================
// Projectile Spawning
// ============================================================================

// Spawn projectile(s) on whether player has multi-shot power-up
pub fn spawn_projectiles(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    pos: &Position,
    face_dir: f32,
    face_pitch: f32,
    has_multi_shot: bool,
    collision_world: &CollisionWorld,
    shooter_id: PlayerId,
) -> usize {
    let spawns = calculate_projectile_spawns(pos, face_dir, face_pitch, has_multi_shot, collision_world);

    for spawn_info in &spawns {
        spawn_single_projectile(commands, projectile_assets, spawn_info, shooter_id);
    }

    spawns.len()
}

// Internal helper to spawn a single projectile
fn spawn_single_projectile(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    spawn_info: &ProjectileSpawnInfo,
    shooter_id: PlayerId,
) {
    let spawn_pos = Vec3::new(spawn_info.position.x, spawn_info.position.y, spawn_info.position.z);

    commands.spawn(ProjectileBundle::new(
        projectile_assets,
        spawn_pos,
        spawn_info.direction_yaw,
        spawn_info.direction_pitch,
        shooter_id,
    ));
}
