use crate::{
    config::{MapMovementConfig, PlayerMovementConfig},
    constants::{PLAYER_AIR_APEX_ACCELERATION_FACTOR, PLAYER_AIR_APEX_RISE_SPEED},
    protocol::PlayerMoveIntent,
};
use bevy_math::Vec3;

// Horizontal target speed; the speed pickup scales it.
#[must_use]
pub fn player_move_speed(cfg: &PlayerMovementConfig, has_speed: bool) -> f32 {
    cfg.move_speed * if has_speed { cfg.move_speed_power_up } else { 1.0 }
}

// Desired ground velocity, also used for animation and ladder intent. It is
// not the player's velocity: accelerate_player owns changes to that state.
#[must_use]
pub fn player_control_velocity(
    intent: PlayerMoveIntent,
    movement: &MapMovementConfig,
    has_speed: bool,
    disabled: bool,
) -> Vec3 {
    if disabled {
        return Vec3::ZERO;
    }
    intent.wish_velocity(player_move_speed(&movement.player, has_speed), false)
}

// Ground and air use independent acceleration, braking, and sideways grip in
// m/s². Braking acts on release, reversal, or excess speed, never against forward
// acceleration. Zero air braking preserves launch momentum; all-zero air rates
// also disable countersteering. Portals do not change these rules. Blast shove
// participates in the air acceleration limit but retains its separate decay:
// passive braking must not store opposing locomotion when the blast wears off.
// Rising slower than PLAYER_AIR_APEX_RISE_SPEED, air acceleration drops to
// PLAYER_AIR_APEX_ACCELERATION_FACTOR of its rate; the descent keeps the full rate.
#[must_use]
pub fn accelerate_player(
    mut velocity: Vec3,
    knockback: Vec3,
    wish: Vec3,
    vertical: f32,
    grounded: bool,
    cfg: &PlayerMovementConfig,
    dt: f32,
) -> Vec3 {
    velocity.y = 0.0;
    let (acceleration, deceleration, lateral_deceleration) = if grounded {
        (
            cfg.ground_acceleration,
            cfg.ground_deceleration,
            cfg.ground_lateral_deceleration,
        )
    } else {
        let apex_factor = if vertical > 0.0 && vertical <= PLAYER_AIR_APEX_RISE_SPEED {
            PLAYER_AIR_APEX_ACCELERATION_FACTOR
        } else {
            1.0
        };
        (
            cfg.air_acceleration * apex_factor,
            cfg.air_deceleration,
            cfg.air_lateral_deceleration,
        )
    };
    if acceleration == 0.0 && deceleration == 0.0 && lateral_deceleration == 0.0 {
        return velocity;
    }
    let Some(direction) = wish.try_normalize() else {
        return slow_velocity(velocity, deceleration * dt);
    };
    let wishspeed = wish.length();
    let mut along = velocity.dot(direction);
    let sideways = slow_velocity(velocity - direction * along, lateral_deceleration * dt);
    if along > wishspeed {
        along = (along - deceleration * dt).max(wishspeed);
    } else {
        // Reversing brakes to zero first, then uses only the remaining time
        // to accelerate. Fast braking must not also give an instant start.
        // With passive braking disabled, opposite input still brakes using its
        // acceleration. All-zero air rates leave velocity unchanged for any input.
        let braking = if deceleration > 0.0 { deceleration } else { acceleration };
        let braking_time = if braking > 0.0 {
            (-along / braking).clamp(0.0, dt)
        } else {
            0.0
        };
        along += braking * braking_time;
        let external = if grounded { 0.0 } else { knockback.dot(direction) };
        let add = (wishspeed - along - external).max(0.0);
        along += (acceleration * (dt - braking_time)).min(add);
    }
    direction * along + sideways
}

fn slow_velocity(velocity: Vec3, drop: f32) -> Vec3 {
    if drop == 0.0 {
        return velocity;
    }
    let speed = velocity.length();
    if speed <= drop {
        Vec3::ZERO
    } else {
        velocity * ((speed - drop) / speed)
    }
}

#[cfg(test)]
#[path = "tests/player_control.rs"]
mod tests;
