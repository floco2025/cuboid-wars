use crate::constants::PROJECTILE_BODY_EMISSIVE;
use bevy::prelude::*;

use crate::characters::PreviousTickPosition;
use common::{
    config::{GameplayConfig, ProjectilesConfig},
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

impl FromWorld for ProjectileAssets {
    fn from_world(world: &mut World) -> Self {
        let brightness = PROJECTILE_BODY_EMISSIVE;
        let radius = world.resource::<GameplayConfig>().projectiles.radius;
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Sphere::new(radius));
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(projectile_material(brightness));

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
        config: &ProjectilesConfig,
        position: Vec3,
        direction_yaw: f32,
        direction_pitch: f32,
        shooter_id: PlayerId,
    ) -> Self {
        Self {
            mesh: Mesh3d(projectile_assets.mesh.clone()),
            material: MeshMaterial3d(projectile_assets.material.clone()),
            transform: Transform::from_translation(position),
            position: position.into(),
            previous_tick_position: PreviousTickPosition(position.into()),
            proj_motion: ProjectileMotion::new(direction_yaw, direction_pitch, config),
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
    config: &ProjectilesConfig,
    pos: Vec3,
    velocity: Vec3,
    shooter: Option<PlayerId>,
) {
    let mut bundle = ProjectileBundle::new(
        projectile_assets,
        config,
        pos,
        0.0,
        0.0,
        shooter.unwrap_or(PlayerId(u32::MAX)),
    );
    bundle.proj_motion.velocity = velocity;
    bundle.proj_motion.lifetime = Timer::from_seconds(EMBER_LIFETIME_SECS, TimerMode::Once);
    commands.spawn(bundle);
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
    has_multi_shot: bool,
    shooter_eye_height: f32,
    gameplay: &GameplayConfig,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    shooter_id: PlayerId,
) -> usize {
    let spawns = calculate_projectile_spawns(
        pos,
        face_yaw,
        face_pitch,
        has_multi_shot,
        shooter_eye_height,
        gameplay,
        collision_world,
        open_kinds,
    );

    for spawn_info in &spawns {
        spawn_single_projectile(
            commands,
            projectile_assets,
            &gameplay.projectiles,
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
    config: &ProjectilesConfig,
    spawn_info: &ProjectileSpawnInfo,
    shooter_id: PlayerId,
) {
    let spawn_pos = Vec3::new(spawn_info.position.x, spawn_info.position.y, spawn_info.position.z);

    commands.spawn(ProjectileBundle::new(
        projectile_assets,
        config,
        spawn_pos,
        spawn_info.direction_yaw,
        spawn_info.direction_pitch,
        shooter_id,
    ));
}
