use std::{collections::HashMap, f32::consts::TAU};

use bevy::{asset::AssetId, light::NotShadowCaster, prelude::*, world_serialization::WorldInstanceReady};
use rand::{RngExt, rng};

use super::particles::{ParticleCloud, ParticleClouds, ParticleSpawn};
use crate::constants::*;
use common::protocol::{ServerTick, sequence_is_newer};

// The warning window in server ticks; the fade is a pure function of the
// shared tick, so nothing counts down or resyncs.
#[derive(Component, Clone, Copy)]
pub struct BeamInGhost {
    pub reserved_tick: u32,
    pub due_tick: u32,
    pub half_extents: Vec3,
    pub center_height: f32,
}

impl BeamInGhost {
    // `overstep` is the render frame's fraction of the current tick. The
    // age is signed so a clock shifted a tick back clamps to zero rather
    // than wrapping.
    fn fade_progress(&self, tick: u32, overstep: f32) -> f32 {
        let window = self.due_tick.wrapping_sub(self.reserved_tick) as i32 as f32;
        if window <= 0.0 {
            return 1.0;
        }
        let age = tick.wrapping_sub(self.reserved_tick) as i32 as f32 + overstep;
        (age / window).clamp(0.0, 1.0)
    }

    fn is_due(&self, tick: u32) -> bool {
        !sequence_is_newer(self.due_tick, tick)
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
    tick: Res<ServerTick>,
    fixed_time: Res<Time<Fixed>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ghosts: Query<(&BeamInGhost, &Children)>,
    mut lights: Query<&mut PointLight>,
    faders: Query<&GhostFadeMaterials>,
) {
    let overstep = fixed_time.overstep_fraction();
    for (ghost, children) in &ghosts {
        let progress = SmoothStepCurve.sample_clamped(ghost.fade_progress(tick.0, overstep));
        let full_intensity = (BEAM_IN_LIGHT_INTENSITY_LUMENS_PER_M3 * ghost.volume()).max(BEAM_IN_LIGHT_MIN_INTENSITY);
        for child in children {
            if let Ok(mut light) = lights.get_mut(*child) {
                light.intensity = full_intensity * progress;
            }
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
    tick: Res<ServerTick>,
    mut clouds: ResMut<ParticleClouds>,
    mut ghosts: Query<(&GlobalTransform, &BeamInGhost, &mut BeamEmitter)>,
) {
    let delta = time.delta_secs();
    let base_color = beam_color(BEAM_IN_SPARKLE_EMISSIVE);
    let mut rng = rng();

    for (transform, ghost, mut emitter) in &mut ghosts {
        let rate = sparkle_rate(ghost.volume(), BEAM_IN_SPARKLES_PER_M3_PER_SECOND);
        let count = take_emissions(&mut emitter.sparkle_credit, rate, delta, BEAM_IN_MAX_SPARKLES_PER_FRAME);
        for _ in 0..count {
            let local_offset = Vec3::new(
                rng.random_range(-ghost.half_extents.x..ghost.half_extents.x),
                ghost.center_height + rng.random_range(-ghost.half_extents.y..ghost.half_extents.y),
                rng.random_range(-ghost.half_extents.z..ghost.half_extents.z),
            );
            let local_drift =
                Vec3::new(rng.random_range(-1.0..1.0), 0.0, rng.random_range(-1.0..1.0)) * BEAM_IN_SPARKLE_DRIFT_SPEED;
            let size = BEAM_IN_SPARKLE_SIZE * rng.random_range(0.65..1.4);
            clouds.sparkles.spawn(ParticleSpawn {
                position: sparkle_world_position(transform, local_offset),
                velocity: transform.rotation()
                    * (local_drift + Vec3::Y * BEAM_IN_SPARKLE_RISE_SPEED * rng.random_range(0.75..1.25)),
                acceleration: Vec3::ZERO,
                start_size: size,
                end_size: size * 0.1,
                stretch: Vec3::ONE,
                fades: true,
                lifetime: BEAM_IN_SPARKLE_LIFETIME_SECS * rng.random_range(0.75..1.25),
                color: base_color * rng.random_range(0.7..1.2),
            });
        }

        if ghost.is_due(tick.0) && !emitter.materialization_emitted {
            if BEAM_IN_MATERIALIZATION_RING_ENABLED {
                spawn_materialization_ring(&mut clouds.sparkles, transform, ghost);
            }
            emitter.materialization_emitted = true;
        }
    }
}

pub fn beam_ghost_removed_system(
    removed: On<Remove, BeamInGhost>,
    mut clouds: ResMut<ParticleClouds>,
    ghosts: Query<(&GlobalTransform, &BeamInGhost, &BeamEmitter)>,
) {
    let Ok((transform, ghost, emitter)) = ghosts.get(removed.entity) else {
        return;
    };
    if BEAM_IN_MATERIALIZATION_RING_ENABLED && !emitter.materialization_emitted {
        spawn_materialization_ring(&mut clouds.sparkles, transform, ghost);
    }
}

fn spawn_materialization_ring(sparkles: &mut ParticleCloud, transform: &GlobalTransform, ghost: &BeamInGhost) {
    let count = BEAM_IN_MATERIALIZATION_PARTICLE_COUNT;
    let radius = ghost.half_extents.x.max(ghost.half_extents.z) * 0.8;
    let base_y = ghost.center_height - ghost.half_extents.y * 0.9;
    let phase = rand::random::<f32>() * TAU;
    let color = beam_color(BEAM_IN_SPARKLE_EMISSIVE * 1.35);

    for index in 0..count {
        let angle = index as f32 / count as f32 * TAU + phase;
        let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
        let local_position = radial * radius + Vec3::Y * base_y;
        let size = BEAM_IN_SPARKLE_SIZE * 1.5;
        sparkles.spawn(ParticleSpawn {
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

#[cfg(test)]
#[path = "tests/beam.rs"]
mod tests;
