use crate::{constants::EXPLOSION_BLAST_CORE_FRACTION, physics::CollisionWorld, protocol::BarrierKindId};
use bevy::prelude::*;

pub fn visible_blast_falloff(
    center: Vec3,
    target: Vec3,
    radius: f32,
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierKindId],
) -> Option<f32> {
    let distance_squared = center.distance_squared(target);
    if distance_squared >= radius * radius {
        return None;
    }
    if !collision_world.attack_path_clear(center, target, open_barriers) {
        return None;
    }
    Some(blast_falloff_at_distance(distance_squared.sqrt(), radius))
}

pub fn planar_shove(center: Vec3, target: Vec3, falloff: f32, max_speed: f32) -> Vec3 {
    Vec3::new(target.x - center.x, 0.0, target.z - center.z).normalize_or_zero() * max_speed * falloff
}

pub fn blast_falloff_at_distance(distance: f32, radius: f32) -> f32 {
    if distance >= radius {
        return 0.0;
    }
    let core = radius * EXPLOSION_BLAST_CORE_FRACTION;
    if distance <= core {
        return 1.0;
    }
    let progress = (distance - core) / (radius - core);
    (1.0 - progress).powi(2)
}
