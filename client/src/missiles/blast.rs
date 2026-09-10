use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    physics::{CollisionWorld, character_hitbox_center, planar_shove, visible_blast_falloff},
    protocol::{BarrierKindId, HitTarget, MissileBlastHit, Position},
};

pub fn missile_blast_hits(
    center: Vec3,
    radius: f32,
    world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    candidates: impl Iterator<Item = (HitTarget, Position, CharacterPhysicsConfig)>,
) -> Vec<MissileBlastHit> {
    candidates
        .filter_map(|(target, pos, physics)| {
            let victim = character_hitbox_center(pos, physics);
            let falloff = visible_blast_falloff(center, victim, radius, world, open_kinds)?;
            let direction = planar_shove(center, victim, 1.0, 1.0);
            Some(MissileBlastHit {
                target,
                falloff,
                direction: [direction.x, direction.z],
            })
        })
        .collect()
}
