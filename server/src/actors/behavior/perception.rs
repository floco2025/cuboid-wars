use bevy::prelude::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    physics::{CharacterSupport, CollisionWorld},
    protocol::{PlayerId, Position},
};

use crate::actors::resources::{ActorInfo, AwarePlayer};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PlayerState {
    pub(super) id: PlayerId,
    pub(super) pos: Position,
    pub(super) support: CharacterSupport,
}

pub(super) fn update_awareness(
    info: &mut ActorInfo,
    actor_pos: Position,
    actor_eye_height: f32,
    vision_range: f32,
    forget_secs: f32,
    player_physics: CharacterPhysicsConfig,
    players: &[PlayerState],
    collision_world: &CollisionWorld,
) {
    let actor_eye = Vec3::new(actor_pos.x, actor_pos.y + actor_eye_height, actor_pos.z);
    let range_sq = vision_range * vision_range;

    info.awareness.retain_mut(|aware| {
        let Some(player) = players.iter().find(|player| player.id == aware.id) else {
            return false;
        };
        aware.visible = player_visible(actor_eye, range_sq, player, player_physics, collision_world);
        if aware.visible {
            aware.pos = player.pos;
            aware.support = player.support;
            aware.forget_remaining_secs = forget_secs;
        }
        aware.forget_remaining_secs > 0.0
    });

    for player in players {
        if info.awareness.iter().any(|aware| aware.id == player.id) {
            continue;
        }
        if !player_visible(actor_eye, range_sq, player, player_physics, collision_world) {
            continue;
        }
        info.awareness.push(AwarePlayer {
            id: player.id,
            pos: player.pos,
            support: player.support,
            visible: true,
            forget_remaining_secs: forget_secs,
            attack_anchor: None,
        });
    }

    info.awareness.sort_by(|a, b| {
        actor_pos
            .distance_sq(&a.pos)
            .total_cmp(&actor_pos.distance_sq(&b.pos))
            .then_with(|| a.id.0.cmp(&b.id.0))
    });
}

fn player_visible(
    actor_eye: Vec3,
    range_sq: f32,
    player: &PlayerState,
    player_physics: CharacterPhysicsConfig,
    collision_world: &CollisionWorld,
) -> bool {
    let center = Vec3::new(
        player.pos.x,
        player_physics.collider_center_y(player.pos.y),
        player.pos.z,
    );
    actor_eye.distance_squared(center) <= range_sq && collision_world.line_of_sight_clear(actor_eye, center)
}
