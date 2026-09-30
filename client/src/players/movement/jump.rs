use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, MapMovementConfig},
    physics::{CharacterSupport, CollisionWorld, player_move_speed, position_has_floor_support},
    protocol::{FieldId, PlayerMoveIntent, Position},
};

#[must_use]
// What a jump request does, from the support the last step left the body with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerJump {
    // Leaves the ground with this upward speed.
    Rise(f32),
    // Lets go of the ladder: a horizontal shove away from its face, no rise.
    Release(Vec3),
}

// A held body lets go instead of rising, shoved the way it is pushing (away
// from the face by default, forward through the rungs), and a body airborne
// in the volume gets no jump at all, so hopping cannot outclimb the ladder.
#[must_use]
pub fn player_jump(
    support: CharacterSupport,
    intent: PlayerMoveIntent,
    vertical_velocity: f32,
    collision_world: &CollisionWorld,
    physics: CharacterPhysicsConfig,
    movement: &MapMovementConfig,
    has_speed: bool,
    pos: &Position,
    passable_fields: &[FieldId],
) -> Option<PlayerJump> {
    if support == CharacterSupport::Ladder {
        let ladder = collision_world.ladder_volume_at(pos)?;
        let away = Vec3::new(ladder.normal_x, 0.0, ladder.normal_z);
        let direction = intent.wish_velocity(1.0, false).try_normalize().unwrap_or(away);
        // The shove decays like a blast, so it is at least what carries the body out of the volume.
        let exit_speed = (2.0 * movement.knockback.deceleration * ladder.exit_distance(pos, direction)).sqrt();
        let ladder_speed = player_move_speed(&movement.player, has_speed) * movement.player.move_speed_ladder;
        return Some(PlayerJump::Release(direction * ladder_speed.max(exit_speed)));
    }
    if vertical_velocity > 0.0 || !position_has_floor_support(collision_world, pos, physics, passable_fields) {
        return None;
    }
    Some(PlayerJump::Rise(movement.player.jump_speed))
}

#[cfg(test)]
#[path = "tests/jump.rs"]
mod tests;
