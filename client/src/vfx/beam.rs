use std::collections::HashMap;

use bevy::{asset::AssetId, light::NotShadowCaster, prelude::*, world_serialization::WorldInstanceReady};
use rand::{RngExt, rng};

use super::particles::{ParticlePriority, ParticleSpawn, TransientParticles};
use crate::{
    config::{ActorBeamInVfxConfig, ClientSettings},
    constants::*,
};

#[derive(Component, Clone, Copy)]
pub struct BeamInGhost {
    pub remaining_secs: f32,
    pub warning_secs: f32,
    pub half_extents: Vec3,
}

impl BeamInGhost {
    pub fn resync(&mut self, update: Self) {
        self.remaining_secs = self.remaining_secs.min(update.remaining_secs);
        self.warning_secs = update.warning_secs;
        self.half_extents = update.half_extents;
    }

    fn fade_progress(&self) -> f32 {
        if self.warning_secs <= 0.0 {
            1.0
        } else {
            (1.0 - self.remaining_secs / self.warning_secs).clamp(0.0, 1.0)
        }
    }

    fn volume(&self) -> f32 {
        8.0 * self.half_extents.x * self.half_extents.y * self.half_extents.z
    }
}

#[derive(Component)]
pub struct BeamEmitter {
    sparkle_credit: f32,
    materialization_emitted: bool,
}

impl Default for BeamEmitter {
    fn default() -> Self {
        Self {
            sparkle_credit: 1.0,
            materialization_emitted: false,
        }
    }
}

struct GhostFadeMaterial {
    handle: Handle<StandardMaterial>,
    source_alpha: f32,
}

#[derive(Component)]
pub struct GhostFadeMaterials(Vec<GhostFadeMaterial>);

pub fn ghost_fade_setup_system(
    scene_ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    mesh_materials: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut clones_by_source = HashMap::<AssetId<StandardMaterial>, usize>::new();
    let mut faded_materials = Vec::<GhostFadeMaterial>::new();

    for child in children.iter_descendants(scene_ready.entity) {
        let Ok(mesh_material) = mesh_materials.get(child) else {
            continue;
        };
        let source_id = mesh_material.0.id();
        let handle = if let Some(index) = clones_by_source.get(&source_id) {
            faded_materials[*index].handle.clone()
        } else {
            let Some(source) = materials.get(&mesh_material.0) else {
                continue;
            };
            let source_alpha = source.base_color.alpha();
            let mut faded = source.clone();
            faded.alpha_mode = AlphaMode::Blend;
            faded.base_color.set_alpha(0.0);
            let handle = materials.add(faded);
            clones_by_source.insert(source_id, faded_materials.len());
            faded_materials.push(GhostFadeMaterial {
                handle: handle.clone(),
                source_alpha,
            });
            handle
        };
        commands.entity(child).insert((MeshMaterial3d(handle), NotShadowCaster));
    }
    commands
        .entity(scene_ready.entity)
        .insert(GhostFadeMaterials(faded_materials));
}

pub fn beam_ghost_fade_system(
    time: Res<Time>,
    settings: Res<ClientSettings>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ghosts: Query<(&mut BeamInGhost, &mut PointLight, &Children)>,
    faders: Query<&GhostFadeMaterials>,
) {
    let delta = time.delta_secs();
    let config = &settings.vfx.actor_beam_in;
    for (mut ghost, mut light, children) in &mut ghosts {
        ghost.remaining_secs = (ghost.remaining_secs - delta).max(0.0);
        let progress = smoothstep(ghost.fade_progress());
        let full_intensity = (config.light_intensity_lumens_per_m3 * ghost.volume()).max(BEAM_IN_LIGHT_MIN_INTENSITY);
        light.intensity = full_intensity * progress;
        for child in children {
            let Ok(fade) = faders.get(*child) else {
                continue;
            };
            for faded in &fade.0 {
                if let Some(mut material) = materials.get_mut(&faded.handle) {
                    material.base_color.set_alpha(faded.source_alpha * progress);
                }
            }
        }
    }
}

pub fn beam_ghost_sparkle_system(
    time: Res<Time>,
    settings: Res<ClientSettings>,
    mut particles: ResMut<TransientParticles>,
    mut ghosts: Query<(&GlobalTransform, &BeamInGhost, &mut BeamEmitter)>,
) {
    let delta = time.delta_secs();
    let config = &settings.vfx.actor_beam_in;
    let base_color = beam_color(config.sparkle_emissive_brightness);
    let mut rng = rng();

    for (transform, ghost, mut emitter) in &mut ghosts {
        let rate = sparkle_rate(ghost.volume(), config.sparkles_per_m3_per_second);
        let count = take_emissions(&mut emitter.sparkle_credit, rate, delta, BEAM_IN_MAX_SPARKLES_PER_FRAME);
        for _ in 0..count {
            let local_offset = Vec3::new(
                rng.random_range(-ghost.half_extents.x..ghost.half_extents.x),
                rng.random_range(-ghost.half_extents.y..ghost.half_extents.y),
                rng.random_range(-ghost.half_extents.z..ghost.half_extents.z),
            );
            let local_drift =
                Vec3::new(rng.random_range(-1.0..1.0), 0.0, rng.random_range(-1.0..1.0)) * BEAM_IN_SPARKLE_DRIFT_SPEED;
            let size = config.sparkle_size * rng.random_range(0.65..1.4);
            particles.spawn(ParticleSpawn {
                position: sparkle_world_position(transform, local_offset),
                velocity: transform.rotation()
                    * (local_drift + Vec3::Y * BEAM_IN_SPARKLE_RISE_SPEED * rng.random_range(0.75..1.25)),
                acceleration: Vec3::ZERO,
                start_size: size,
                end_size: size * 0.1,
                stretch: Vec3::ONE,
                fades: true,
                lifetime: config.sparkle_lifetime_secs * rng.random_range(0.75..1.25),
                color: base_color * rng.random_range(0.7..1.2),
                priority: ParticlePriority::Ambient,
            });
        }

        if ghost.remaining_secs <= f32::EPSILON && !emitter.materialization_emitted {
            if config.materialization_ring_enabled {
                spawn_materialization_ring(&mut particles, transform, ghost, config);
            }
            emitter.materialization_emitted = true;
        }
    }
}

pub fn beam_ghost_removed_system(
    removed: On<Remove, BeamInGhost>,
    settings: Res<ClientSettings>,
    mut particles: ResMut<TransientParticles>,
    ghosts: Query<(&GlobalTransform, &BeamInGhost, &BeamEmitter)>,
) {
    let Ok((transform, ghost, emitter)) = ghosts.get(removed.entity) else {
        return;
    };
    if settings.vfx.actor_beam_in.materialization_ring_enabled && !emitter.materialization_emitted {
        spawn_materialization_ring(&mut particles, transform, ghost, &settings.vfx.actor_beam_in);
    }
}

fn spawn_materialization_ring(
    particles: &mut TransientParticles,
    transform: &GlobalTransform,
    ghost: &BeamInGhost,
    config: &ActorBeamInVfxConfig,
) {
    let count = BEAM_IN_MATERIALIZATION_PARTICLE_COUNT;
    let radius = ghost.half_extents.x.max(ghost.half_extents.z) * 0.8;
    let base_y = -ghost.half_extents.y * 0.9;
    let phase = rand::random::<f32>() * std::f32::consts::TAU;
    let color = beam_color(config.sparkle_emissive_brightness * 1.35);

    for index in 0..count {
        let angle = index as f32 / count as f32 * std::f32::consts::TAU + phase;
        let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
        let local_position = radial * radius + Vec3::Y * base_y;
        let size = config.sparkle_size * 1.5;
        particles.spawn(ParticleSpawn {
            position: sparkle_world_position(transform, local_position),
            velocity: transform.rotation()
                * (radial * BEAM_IN_MATERIALIZATION_SPEED + Vec3::Y * BEAM_IN_MATERIALIZATION_SPEED * 0.35),
            acceleration: Vec3::NEG_Y * 2.0,
            start_size: size,
            end_size: 0.0,
            stretch: Vec3::ONE,
            fades: true,
            lifetime: BEAM_IN_MATERIALIZATION_LIFETIME_SECS,
            color,
            priority: ParticlePriority::Cue,
        });
    }
}

fn beam_color(brightness: f32) -> Vec3 {
    let color = BEAM_IN_COLOR.to_linear();
    Vec3::new(color.red, color.green, color.blue) * brightness
}

fn sparkle_world_position(transform: &GlobalTransform, local_offset: Vec3) -> Vec3 {
    transform.transform_point(local_offset)
}

fn sparkle_rate(volume: f32, density: f32) -> f32 {
    let scaled_minimum = BEAM_IN_MIN_SPARKLES_PER_SECOND * density / BEAM_IN_REFERENCE_SPARKLES_PER_M3_PER_SECOND;
    (volume * density).max(scaled_minimum)
}

pub(super) fn take_emissions(credit: &mut f32, rate: f32, delta: f32, max_per_frame: usize) -> usize {
    *credit += rate * delta;
    let due = credit.floor() as usize;
    *credit -= due as f32;
    due.min(max_per_frame)
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_progress_clamps_to_the_warning_window() {
        let mut ghost = BeamInGhost {
            remaining_secs: 3.0,
            warning_secs: 3.0,
            half_extents: Vec3::ONE,
        };
        assert_eq!(ghost.fade_progress(), 0.0);
        ghost.remaining_secs = 1.5;
        assert_eq!(ghost.fade_progress(), 0.5);
        ghost.remaining_secs = 0.0;
        assert_eq!(ghost.fade_progress(), 1.0);
    }

    #[test]
    fn missed_emissions_are_capped_without_building_debt() {
        let mut credit = 0.0;
        assert_eq!(take_emissions(&mut credit, 1_000.0, 1.0, 32), 32);
        assert_eq!(take_emissions(&mut credit, 0.0, 0.0, 32), 0);
    }

    #[test]
    fn sparkle_offsets_follow_ghost_rotation() {
        let transform = GlobalTransform::from(
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(3.0, 2.0, 1.0)),
        );
        let world = sparkle_world_position(&transform, Vec3::X);
        assert!(world.abs_diff_eq(Vec3::new(3.0, 2.0, 0.0), 0.0001));
    }

    #[test]
    fn sparkle_density_scales_the_small_actor_floor() {
        assert_eq!(sparkle_rate(0.01, 200.0), 20.0);
        assert_eq!(sparkle_rate(0.01, 100.0), 10.0);
        assert_eq!(sparkle_rate(0.01, 0.0), 0.0);
    }

    #[test]
    fn snapshot_resync_never_rewinds_the_fade() {
        let mut ghost = BeamInGhost {
            remaining_secs: 1.0,
            warning_secs: 3.0,
            half_extents: Vec3::ONE,
        };
        ghost.resync(BeamInGhost {
            remaining_secs: 1.2,
            warning_secs: 4.0,
            half_extents: Vec3::splat(2.0),
        });

        assert_eq!(ghost.remaining_secs, 1.0);
        assert_eq!(ghost.warning_secs, 4.0);
        assert_eq!(ghost.half_extents, Vec3::splat(2.0));
    }
}
