use bevy::prelude::*;
use common::{
    config::{GameplayConfig, MapMovementConfig},
    constants::{PLAYER_CROUCH_PORTAL_TILT, PORTAL_KNOCKBACK_CARRY_FACTOR},
    physics::{
        CharacterHopBody, CharacterPortalHop, CharacterVerticalVelocity, KnockbackVelocity, PortalSet,
        traverse_move_intent, traverse_vector,
    },
    protocol::{FaceYaw, PlayerMoveIntent, PlayerStance, Position},
};

use crate::players::HorizontalVelocity;

pub struct PlayerHopBody<'a> {
    pub stance: PlayerStance,
    pub knockback: &'a KnockbackVelocity,
    pub horizontal_velocity: &'a HorizontalVelocity,
    pub vertical_velocity: f32,
    // The velocity the ride gives a grounded body on top of its own.
    pub carried: Vec3,
    pub yaw: f32,
}

// A player's crossing: the shared hop, plus the shorter hull an angled exit forces.
#[derive(Debug, Clone, Copy)]
pub struct PlayerHop {
    pub crossing: CharacterPortalHop,
    pub force_crouch: bool,
}

impl PlayerHop {
    pub fn apply(
        &self,
        position: &mut Position,
        face_yaw: &mut FaceYaw,
        vertical_velocity: &mut CharacterVerticalVelocity,
        move_intent: &mut PlayerMoveIntent,
        stance: &mut PlayerStance,
        knockback: &mut KnockbackVelocity,
        horizontal_velocity: &mut HorizontalVelocity,
    ) {
        let hop = &self.crossing;
        if self.force_crouch {
            stance.crouched = true;
        }
        *position = hop.origin.into();
        face_yaw.0 = hop.yaw;
        vertical_velocity.0 = hop.vertical_velocity;
        *move_intent = traverse_move_intent(&hop.entry, &hop.exit, *move_intent);
        knockback.0 = hop.knockback;
        horizontal_velocity.0 = hop.horizontal_velocity;
    }
}

// The player's crossing over the shared `character_hop`: knockback keeps its
// cap, and rotating an upright body more than 30 degrees needs the shorter
// hull at the exit, whose centre is preserved as with an airborne manual duck.
#[must_use]
pub fn player_hop(
    portals: &PortalSet,
    from: Vec3,
    to: Vec3,
    gameplay_config: &GameplayConfig,
    movement: &MapMovementConfig,
    body: PlayerHopBody<'_>,
    delta: f32,
) -> Option<PlayerHop> {
    let mut crossing = portals.character_hop(
        from,
        to,
        body.stance.physics(&gameplay_config.player),
        CharacterHopBody {
            knockback: body.knockback.0,
            horizontal_velocity: body.horizontal_velocity.0,
            vertical_velocity: body.vertical_velocity,
            carried: body.carried,
            yaw: body.yaw,
        },
        PORTAL_KNOCKBACK_CARRY_FACTOR * movement.knockback.max_speed,
        delta,
    )?;
    let force_crouch = !body.stance.crouched
        && traverse_vector(&crossing.entry, &crossing.exit, Vec3::Y).y.abs() < PLAYER_CROUCH_PORTAL_TILT.cos();
    if force_crouch {
        let old = body.stance.physics(&gameplay_config.player);
        let crouched = PlayerStance { crouched: true }.physics(&gameplay_config.player);
        crossing.origin.y += (old.movement_collider.height - crouched.movement_collider.height) * 0.5;
    }
    Some(PlayerHop { crossing, force_crouch })
}

#[cfg(test)]
#[path = "tests/hop.rs"]
mod tests;
