use bevy::prelude::*;

use crate::{
    actors::{ActorCharacter, ActorLanding, ActorMap},
    characters::{FALL_DAMAGE_EMIT_THRESHOLD, fall_damage_for_distance, fall_distance_for_speed},
    combat::{PendingExplosions, apply_damage, kill_actor},
    config::{FallDamageConfigs, ServerGameplayConfig},
    players::PlayerMap,
};
use common::protocol::{ActorId, ActorMarker, Health, MapSettings, Position};

// Ground actors take the map's `actor_fall` damage on landing; a lethal
// landing credits nobody, like a crush.
pub fn actors_fall_damage_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    players: Res<PlayerMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_settings: Res<MapSettings>,
    fall: Res<FallDamageConfigs>,
    mut query: Query<(Entity, &ActorId, &Position, &ActorLanding, &ActorCharacter, &mut Health), With<ActorMarker>>,
) {
    for (entity, id, pos, landing, character, mut health) in &mut query {
        // A body already at zero awaits removal with its shooter's credit.
        if character.0.flies() || landing.0 <= 0.0 || health.0 <= 0.0 {
            continue;
        }
        let Some(info) = actors.get(id) else {
            continue;
        };
        let max_health = server_gameplay_config.combat.health.expect_actor(&info.spawn_kind).max;
        let fall_distance = fall_distance_for_speed(landing.0, map_settings.movement.gravity);
        let damage = fall_damage_for_distance(fall_distance, &fall.actor, max_health);
        if damage < FALL_DAMAGE_EMIT_THRESHOLD {
            continue;
        }
        apply_damage(&mut health, damage);
        if health.0 > 0.0 {
            continue;
        }
        info!("{} died from fall (distance {fall_distance:.1}m)", actors.describe(id));
        kill_actor(
            &mut commands,
            &mut actors,
            &players,
            &mut pending_explosions,
            &server_gameplay_config.feed,
            *id,
            entity,
            *pos,
            None,
        );
    }
}

#[cfg(test)]
#[path = "tests/falling.rs"]
mod tests;
