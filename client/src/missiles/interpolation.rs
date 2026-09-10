use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    map::Carriers,
    math::angle_delta_radians,
    physics::CollisionWorld,
    protocol::{MapLayout, MissileId, MissileMovementState, Position},
};

use super::{MissileImpact, MissileMap, MissileVelocity, RemoteMissileMotion};
use crate::{
    audio::play_explosion_sound,
    carriers::CarrierEntities,
    config::{AssetSet, ClientSettings},
    vfx::{BlastRadii, ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, spawn_missile_explosion},
};

impl RemoteMissileMotion {
    pub(crate) fn advance(&mut self, delta_ticks: f64) -> MissileMovementState {
        let playback = self.buffer.advance(delta_ticks);
        let mut movement = *playback.left;
        let Some(right) = playback.right else {
            return movement;
        };
        let start = Vec3::from(movement.pos);
        movement.pos = start.lerp(Vec3::from(right.pos), playback.alpha).into();
        movement.yaw += angle_delta_radians(right.yaw, movement.yaw) * playback.alpha;
        movement.pitch += (right.pitch - movement.pitch) * playback.alpha;
        movement.speed += (right.speed - movement.speed) * playback.alpha;
        movement
    }
}

pub(crate) fn interpolate_remote_missiles_system(
    mut commands: Commands,
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(Entity, &mut RemoteMissileMotion, &mut Position, &mut MissileVelocity), Without<MissileImpact>>,
) {
    let delta_ticks = time.delta_secs_f64() / fixed_time.timestep().as_secs_f64();
    for (entity, mut motion, mut pos, mut velocity) in &mut query {
        let movement = motion.advance(delta_ticks);
        *pos = movement.pos;
        velocity.0 = movement.velocity();
        if let Some(impact) = motion.detonation_reached() {
            commands.entity(entity).insert(MissileImpact(impact));
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct MissileExplosionParams<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    budget: ResMut<'w, ExplosionVfxBudget>,
    explosion_assets: Res<'w, ExplosionAssets>,
    gameplay_config: Res<'w, GameplayConfig>,
    collision_world: Res<'w, CollisionWorld>,
    map_layout: Res<'w, MapLayout>,
    carriers: Res<'w, Carriers>,
    carrier_entities: Res<'w, CarrierEntities>,
    blast_radii: Res<'w, BlastRadii>,
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    client_settings: Res<'w, ClientSettings>,
}

impl MissileExplosionParams<'_> {
    fn explode(&mut self, commands: &mut Commands, pos: Position) {
        let mut ctx = ExplosionSpawnCtx {
            meshes: &mut self.meshes,
            materials: &mut self.materials,
            budget: &mut self.budget,
            explosion_assets: &self.explosion_assets,
            gameplay_config: &self.gameplay_config,
            collision_world: Some(&self.collision_world),
            map_layout: Some(&self.map_layout),
            carriers: &self.carriers,
            carrier_entities: &self.carrier_entities,
            blast_radii: &self.blast_radii,
        };
        spawn_missile_explosion(commands, &mut ctx, pos);
        play_explosion_sound(
            commands,
            &self.asset_server,
            self.asset_set.player_sound("explodes"),
            &self.client_settings.audio,
            Vec3::from(pos),
            Some(self.blast_radii.missile),
        );
    }
}

pub(crate) fn remote_missile_impacts_system(
    mut commands: Commands,
    mut missiles: ResMut<MissileMap>,
    mut explosion: MissileExplosionParams,
    query: Query<(Entity, &MissileId, &MissileImpact)>,
) {
    for (entity, id, impact) in &query {
        missiles.take(id);
        commands.entity(entity).despawn();
        explosion.explode(&mut commands, impact.0);
    }
}

#[cfg(test)]
#[path = "interpolation_tests.rs"]
mod tests;
