use super::shards::{ExplosionShardCloud, bounce_on_surface, update_particle_mesh};
use super::smoke::{ExplosionSmokeCloud, update_smoke_mesh};
use crate::{cameras::MainCameraMarker, config::ClientSettings, constants::*};
use bevy::prelude::*;
use common::physics::WorldSurfaceHit;
use rand::{Rng, RngExt};
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub struct ExplosionVfxBudget {
    active_shards: usize,
    active_smoke: usize,
    active_lights: usize,
    scorches: VecDeque<Entity>,
}

impl ExplosionVfxBudget {
    pub(super) fn reserve_shards(&mut self, requested: usize, max_active: usize) -> usize {
        let granted = requested.min(max_active.saturating_sub(self.active_shards));
        self.active_shards += granted;
        granted
    }

    pub(super) fn release_shards(&mut self, count: usize) {
        self.active_shards = self.active_shards.saturating_sub(count);
    }

    pub(super) fn reserve_smoke(&mut self, requested: usize, max_active: usize) -> usize {
        let granted = requested.min(max_active.saturating_sub(self.active_smoke));
        self.active_smoke += granted;
        granted
    }

    pub(super) fn release_smoke(&mut self, count: usize) {
        self.active_smoke = self.active_smoke.saturating_sub(count);
    }

    pub(super) fn reserve_light(&mut self, max_active: usize) -> bool {
        if self.active_lights >= max_active {
            return false;
        }
        self.active_lights += 1;
        true
    }

    pub(super) fn release_light(&mut self) {
        self.active_lights = self.active_lights.saturating_sub(1);
    }

    pub(super) fn register_scorch(&mut self, commands: &mut Commands, entity: Entity, max_active: usize) {
        if self.scorches.len() >= max_active
            && let Some(oldest) = self.scorches.pop_front()
        {
            commands.entity(oldest).despawn();
        }
        self.scorches.push_back(entity);
    }

    pub(super) fn remove_scorch(&mut self, entity: Entity) {
        self.scorches.retain(|candidate| *candidate != entity);
    }
}

#[derive(Clone, Copy)]
pub(super) struct SurfacePlane {
    pub(super) point: Vec3,
    pub(super) normal: Vec3,
    pub(super) radius: f32,
}

impl SurfacePlane {
    pub(super) fn from_hit(hit: WorldSurfaceHit, center: Vec3, radius: f32) -> Self {
        Self {
            point: hit.point - center,
            normal: hit.normal,
            radius,
        }
    }
}

pub fn explosion_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    _settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut budget: ResMut<ExplosionVfxBudget>,
    mut shards: Query<(Entity, &mut ExplosionShardCloud)>,
    mut smoke: Query<(Entity, &mut ExplosionSmokeCloud)>,
    main_camera: Query<&GlobalTransform, (With<Camera3d>, With<MainCameraMarker>)>,
) {
    let delta = time.delta_secs();
    for (entity, mut cloud) in &mut shards {
        cloud.elapsed += delta;
        let elapsed = cloud.elapsed;
        let ground = cloud.ground;
        let mut alive = false;
        for particle in &mut cloud.particles {
            if elapsed >= particle.lifetime || particle.max_distance.is_some_and(|limit| particle.travelled >= limit) {
                continue;
            }
            alive = true;
            particle.velocity.y -= EXPLOSION_SHARD_GRAVITY * delta;
            let step = particle.velocity * delta;
            particle.position += step;
            particle.travelled += step.length();
            particle.rotation = Quat::from_scaled_axis(particle.angular_velocity * delta) * particle.rotation;
            if let Some(plane) = ground {
                bounce_on_surface(particle, plane);
            }
        }
        if !alive {
            budget.release_shards(cloud.reserved_count);
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(mut mesh) = meshes.get_mut(&cloud.mesh) {
            update_particle_mesh(&mut mesh, &cloud.particles, elapsed);
        }
    }

    let (smoke_right, smoke_up, smoke_normal) = main_camera.single().map_or((Vec3::X, Vec3::Y, Vec3::Z), |camera| {
        let rotation = camera.to_scale_rotation_translation().1;
        (rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z)
    });
    for (entity, mut cloud) in &mut smoke {
        cloud.elapsed += delta;
        let elapsed = cloud.elapsed;
        let mut alive = false;
        for particle in &mut cloud.particles {
            if elapsed >= particle.lifetime {
                continue;
            }
            alive = true;
            particle.position += particle.velocity * delta;
            particle.velocity *= (1.0 - delta * 0.35).max(0.0);
            particle.rotation += particle.angular_velocity * delta;
        }
        if !alive {
            budget.release_smoke(cloud.reserved_count);
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(mut mesh) = meshes.get_mut(&cloud.mesh) {
            update_smoke_mesh(
                &mut mesh,
                &cloud.particles,
                elapsed,
                smoke_right,
                smoke_up,
                smoke_normal,
                EXPLOSION_SMOKE_MAX_OPACITY,
            );
        }
    }
}

pub(super) fn random_direction(rng: &mut impl Rng) -> Vec3 {
    let direction = Vec3::new(
        rng.random_range(-1.0..1.0),
        rng.random_range(-1.0..1.0),
        rng.random_range(-1.0..1.0),
    ) + Vec3::Y * EXPLOSION_SHARD_UP_BIAS;
    if direction.length_squared() <= f32::EPSILON {
        Vec3::Y
    } else {
        direction.normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_budget_clamps_and_releases_particles() {
        let mut budget = ExplosionVfxBudget::default();
        assert_eq!(
            budget.reserve_shards(EXPLOSION_SHARD_GLOBAL_MAX_COUNT + 10, EXPLOSION_SHARD_GLOBAL_MAX_COUNT,),
            EXPLOSION_SHARD_GLOBAL_MAX_COUNT
        );
        assert_eq!(budget.reserve_shards(1, EXPLOSION_SHARD_GLOBAL_MAX_COUNT), 0);
        budget.release_shards(20);
        assert_eq!(budget.reserve_shards(30, EXPLOSION_SHARD_GLOBAL_MAX_COUNT), 20);
    }
}
