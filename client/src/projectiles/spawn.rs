use bevy::prelude::*;

use crate::{
    characters::PreviousTickPosition,
    constants::{PROJECTILE_BODY_EMISSIVE, PROJECTILE_COLOR},
};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, ProjectileMotion, calculate_projectile_spawns},
    protocol::*,
};

// ============================================================================
// Resources
// ============================================================================

#[derive(Resource)]
pub struct ProjectileAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

impl ProjectileAssets {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, radius: f32) -> Self {
        let brightness = PROJECTILE_BODY_EMISSIVE;
        let mesh = meshes.add(Sphere::new(radius));
        let material = materials.add(projectile_material(brightness));

        Self { mesh, material }
    }
}

fn projectile_material(brightness: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: PROJECTILE_COLOR,
        emissive: (LinearRgba::from(PROJECTILE_COLOR) * brightness).with_alpha(1.0),
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
    // Fixed steps keep the trajectory independent of rendering frame rate.
    position: Position,
    previous_tick_position: PreviousTickPosition,
    proj_motion: ProjectileMotion,
    proj_marker: ProjectileMarker,
    player_id: PlayerId,
}

impl ProjectileBundle {
    fn with_motion(
        projectile_assets: &ProjectileAssets,
        position: Vec3,
        proj_motion: ProjectileMotion,
        shooter_id: PlayerId,
    ) -> Self {
        Self {
            mesh: Mesh3d(projectile_assets.mesh.clone()),
            material: MeshMaterial3d(projectile_assets.material.clone()),
            transform: Transform::from_translation(position),
            position: position.into(),
            previous_tick_position: PreviousTickPosition(position.into()),
            proj_motion,
            player_id: shooter_id,
            proj_marker: ProjectileMarker,
        }
    }
}

#[derive(Component)]
pub struct EmberMarker;

const EMBER_LIFETIME_SECS: f32 = 6.0;

pub fn spawn_ember_projectile(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    gameplay: &GameplayConfig,
    pos: Vec3,
    velocity: Vec3,
    shooter: Option<PlayerId>,
) {
    let mut motion = ProjectileMotion::from_velocity(velocity, &gameplay.projectiles);
    motion.lifetime = Timer::from_seconds(EMBER_LIFETIME_SECS, TimerMode::Once);
    commands.spawn((
        ProjectileBundle::with_motion(projectile_assets, pos, motion, shooter.unwrap_or(PlayerId(u32::MAX))),
        EmberMarker,
    ));
}

// ============================================================================
// Projectile Spawning
// ============================================================================

pub fn spawn_projectiles(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    shot: &CProjectileShot,
    gameplay: &GameplayConfig,
    projectile_speed: f32,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    shooter_id: PlayerId,
) -> usize {
    let spawns = calculate_projectile_spawns(
        &shot.origin,
        shot.face_yaw,
        shot.face_pitch,
        shot.pattern,
        gameplay,
        collision_world,
        open_kinds,
    );

    for spawn in &spawns {
        let motion = ProjectileMotion::new(
            spawn.direction_yaw,
            spawn.direction_pitch,
            projectile_speed,
            &gameplay.projectiles,
        );
        commands.spawn(ProjectileBundle::with_motion(
            projectile_assets,
            Vec3::from(spawn.position),
            motion,
            shooter_id,
        ));
    }
    spawns.len()
}
