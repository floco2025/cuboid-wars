use crate::constants::PROJECTILE_BODY_EMISSIVE;
use bevy::prelude::*;

use crate::characters::PreviousTickPosition;
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, ProjectileMotion, ProjectileSpawnInfo, calculate_projectile_spawns},
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
        base_color: Color::srgb(brightness, brightness, 0.0),
        emissive: LinearRgba::rgb(brightness, brightness, 0.0),
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
    // Authoritative simulation state lives in `Position`, stepped at the
    // fixed `TICK_HZ` rate to match the server's integration; `Transform` is
    // presentation only, interpolated between the last two tick positions.
    position: Position,
    previous_tick_position: PreviousTickPosition,
    proj_motion: ProjectileMotion,
    proj_marker: ProjectileMarker,
    player_id: PlayerId,
}

impl ProjectileBundle {
    fn new(
        projectile_assets: &ProjectileAssets,
        gameplay: &GameplayConfig,
        projectile_speed: f32,
        position: Vec3,
        direction_yaw: f32,
        direction_pitch: f32,
        shooter_id: PlayerId,
    ) -> Self {
        Self::with_motion(
            projectile_assets,
            position,
            ProjectileMotion::new(direction_yaw, direction_pitch, projectile_speed, &gameplay.projectiles),
            shooter_id,
        )
    }

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

// Presentation-only "ember" for the firework show: a glowing projectile with
// an explicit velocity and a short life. No server twin exists, so it can
// never deal damage — it just arcs, bounces, and sparks like the real ones.
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
    commands.spawn(ProjectileBundle::with_motion(
        projectile_assets,
        pos,
        motion,
        shooter.unwrap_or(PlayerId(u32::MAX)),
    ));
}

// ============================================================================
// Projectile Spawning
// ============================================================================

// Spawn projectile(s) on whether player has multi-shot power-up
#[expect(
    clippy::too_many_arguments,
    reason = "presentation spawn mirrors the server's spawn inputs"
)]
pub fn spawn_projectiles(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    pos: &Position,
    face_yaw: f32,
    face_pitch: f32,
    multi_shot_pattern: Option<&str>,
    shooter_eye_height: f32,
    gameplay: &GameplayConfig,
    projectile_speed: f32,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    shooter_id: PlayerId,
) -> usize {
    let spawns = calculate_projectile_spawns(
        pos,
        face_yaw,
        face_pitch,
        multi_shot_pattern,
        shooter_eye_height,
        gameplay,
        collision_world,
        open_kinds,
    );

    for spawn_info in &spawns {
        spawn_single_projectile(
            commands,
            projectile_assets,
            gameplay,
            projectile_speed,
            spawn_info,
            shooter_id,
        );
    }

    spawns.len()
}

// Internal helper to spawn a single projectile
fn spawn_single_projectile(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    gameplay: &GameplayConfig,
    projectile_speed: f32,
    spawn_info: &ProjectileSpawnInfo,
    shooter_id: PlayerId,
) {
    let spawn_pos = Vec3::new(spawn_info.position.x, spawn_info.position.y, spawn_info.position.z);

    commands.spawn(ProjectileBundle::new(
        projectile_assets,
        gameplay,
        projectile_speed,
        spawn_pos,
        spawn_info.direction_yaw,
        spawn_info.direction_pitch,
        shooter_id,
    ));
}
