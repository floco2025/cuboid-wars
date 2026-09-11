use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    physics::{CollisionWorld, blast_hit, character_hitbox_center},
    protocol::{BarrierId, HitTarget, MissileBlastHit, Position},
};

pub fn missile_blast_hits(
    center: Vec3,
    radius: f32,
    world: &CollisionWorld,
    open_kinds: &[BarrierId],
    candidates: impl Iterator<Item = (HitTarget, Position, CharacterPhysicsConfig)>,
) -> Vec<MissileBlastHit> {
    candidates
        .filter_map(|(target, pos, physics)| {
            let victim = character_hitbox_center(pos, physics);
            let (falloff, direction) = blast_hit(center, victim, radius, world, open_kinds)?;
            Some(MissileBlastHit {
                target,
                falloff,
                direction: [direction.x, direction.z],
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/blast.rs"]
mod tests;
