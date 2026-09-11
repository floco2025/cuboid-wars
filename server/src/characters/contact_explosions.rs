use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    physics::{CollisionWorld, character_hitbox_center},
    protocol::{ActorId, ActorMarker, BarrierId, Health, PlateState, PlayerMarker, Position},
};

use crate::{actors::ActorMap, characters::character_surface_distance, config::ServerGameplayConfig};

// A character where this tick left it.
#[derive(Clone, Copy)]
pub(super) struct CharacterBody {
    pub entity: Entity,
    pub pos: Position,
    pub physics: CharacterPhysicsConfig,
}

pub(super) fn contact_explosions_system(
    mut health: Query<&mut Health, With<ActorMarker>>,
    actors: Res<ActorMap>,
    gameplay: Res<GameplayConfig>,
    config: Res<ServerGameplayConfig>,
    collision: Res<CollisionWorld>,
    plates: Res<PlateState>,
    players: Query<(Entity, &Position), With<PlayerMarker>>,
    actor_positions: Query<(Entity, &ActorId, &Position), With<ActorMarker>>,
) {
    let players: Vec<_> = players
        .iter()
        .map(|(entity, pos)| CharacterBody {
            entity,
            pos: *pos,
            physics: gameplay.player.physics(),
        })
        .collect();
    // Actors whose kind detonates on contact, with the kind's trigger gap.
    let contact_actors: Vec<_> = actor_positions
        .iter()
        .filter_map(|(entity, id, pos)| {
            let kind = &actors.get(id)?.spawn_kind;
            let trigger_gap = config.expect_actor(kind).attack.contact_trigger_gap()?;
            let body = CharacterBody {
                entity,
                pos: *pos,
                physics: gameplay.expect_actor(kind).physics(),
            };
            Some((body, trigger_gap))
        })
        .collect();
    detonate_actors_touching_players(
        &mut health,
        actors.peaceful,
        &players,
        &contact_actors,
        &collision,
        &plates.open_barriers,
    );
}

pub(super) fn detonate_actors_touching_players(
    actor_health: &mut Query<&mut Health, With<ActorMarker>>,
    peaceful: bool,
    players: &[CharacterBody],
    contact_actors: &[(CharacterBody, f32)],
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierId],
) {
    if peaceful {
        return;
    }
    for player in players {
        for (actor, trigger_gap) in contact_actors {
            if character_bodies_touch(player, actor, *trigger_gap, collision_world, open_barriers)
                && let Ok(mut health) = actor_health.get_mut(actor.entity)
            {
                health.0 = 0.0;
            }
        }
    }
}

fn character_bodies_touch(
    a: &CharacterBody,
    b: &CharacterBody,
    trigger_gap: f32,
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierId],
) -> bool {
    // Character movement blocks before colliders overlap, so contact uses a
    // configurable surface tolerance instead of requiring actual intersection.
    if character_surface_distance(a.pos, a.physics, b.pos, b.physics) > trigger_gap {
        return false;
    }
    collision_world.attack_path_clear(
        character_hitbox_center(a.pos, a.physics),
        character_hitbox_center(b.pos, b.physics),
        open_barriers,
    )
}

#[cfg(test)]
#[path = "tests/contact_explosions.rs"]
mod tests;
