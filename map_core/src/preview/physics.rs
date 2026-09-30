use anyhow::{Result, ensure};
use bevy_math::Vec3;
use common::{
    config::{
        CharacterPhysicsConfig, FallDamageConfig, HitboxConfig, MovementColliderConfig, PlayerMovementConfig,
        PortalFunnelConfig, PortalSize, validate_non_negative_finite, validate_positive_finite,
    },
    physics::{PortalFrame, character_movement_center, player_move_speed, portal_placement_yaw},
    protocol::Position,
};
use serde::Deserialize;

// The movement values a preview needs, taken from the merged map settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewPhysics {
    pub server_hz: u32,
    pub gravity: f32,
    pub low_gravity: f32,
    pub player: PlayerMovementConfig,
    pub player_fall: FallDamageConfig,
    pub max_health: f32,
    pub body: MovementColliderConfig,
    pub portal_size: PortalSize,
    pub funnel: PortalFunnelConfig,
}

// A pickup combination a preview is flown with.
#[derive(Debug, Clone, Copy)]
pub(super) struct Scenario {
    pub has_speed: bool,
    pub has_low_gravity: bool,
}

// Every preview reports these, in this order.
pub(super) const SCENARIOS: [Scenario; 4] = [
    Scenario {
        has_speed: false,
        has_low_gravity: false,
    },
    Scenario {
        has_speed: true,
        has_low_gravity: false,
    },
    Scenario {
        has_speed: false,
        has_low_gravity: true,
    },
    Scenario {
        has_speed: true,
        has_low_gravity: true,
    },
];

impl PreviewPhysics {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.server_hz >= 1, "network.server_hz must be positive");
        validate_positive_finite(self.gravity, "movement.gravity")?;
        validate_non_negative_finite(self.low_gravity, "movement.low_gravity")?;
        self.player.validate("movement.player")?;
        self.player_fall.validate("player_fall")?;
        validate_positive_finite(self.max_health, "combat.health.player.max")?;
        self.body.validate("player.movement_collider")?;
        validate_positive_finite(self.portal_size.width, "weapons.portals.size.width")?;
        validate_positive_finite(self.portal_size.height, "weapons.portals.size.height")?;
        validate_non_negative_finite(self.funnel.capture_margin, "weapons.portals.funnel.capture_margin")
    }

    pub(super) fn tick(&self) -> f32 {
        1.0 / self.server_hz as f32
    }

    pub(super) fn gravity_for(&self, scenario: Scenario) -> f32 {
        if scenario.has_low_gravity {
            self.low_gravity
        } else {
            self.gravity
        }
    }

    pub(super) fn speed_for(&self, scenario: Scenario) -> f32 {
        player_move_speed(&self.player, scenario.has_speed)
    }

    // The hitbox plays no part in a flight; the hop needs a complete body.
    pub(super) fn character(&self) -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            movement_collider: self.body,
            hitbox: HitboxConfig {
                width: self.body.diameter,
                height: self.body.height,
                depth: self.body.diameter,
                bottom_offset: 0.0,
            },
        }
    }

    pub(super) fn centre_offset(&self) -> Vec3 {
        character_movement_center(Position::default(), self.character())
    }

    pub(super) fn frame(&self, surface: &SurfaceSpec) -> PortalFrame {
        let normal = Vec3::from_array(surface.normal);
        PortalFrame::from_surface(
            Vec3::from_array(surface.center),
            normal,
            portal_placement_yaw(normal, surface.yaw),
            self.portal_size,
        )
    }
}

// A portal as it is shot: `yaw` is the shooter's facing, snapped by the
// game's placement rule.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceSpec {
    pub center: [f32; 3],
    pub normal: [f32; 3],
    pub yaw: f32,
}

impl SurfaceSpec {
    pub(super) fn validate(&self, name: &str) -> Result<()> {
        ensure!(
            self.center.iter().chain(&self.normal).all(|value| value.is_finite()) && self.yaw.is_finite(),
            "{name} must be finite"
        );
        ensure!(
            Vec3::from_array(self.normal).length_squared() > 1e-6,
            "{name}.normal must not be zero"
        );
        Ok(())
    }
}

pub fn portal_frame(physics: &PreviewPhysics, surface: &SurfaceSpec) -> PortalFrame {
    physics.frame(surface)
}

#[cfg(test)]
#[path = "tests/physics.rs"]
mod tests;
