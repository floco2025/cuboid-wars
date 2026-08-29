use bevy::prelude::*;

use crate::{actors::ActorMap, config::ServerGameplayConfig};
use common::{
    health::regenerate_health,
    protocol::{ActorId, ActorMarker, Health, PlayerMarker},
};

pub fn characters_health_regeneration_system(
    time: Res<Time>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    actors_map: Res<ActorMap>,
    mut players: Query<&mut Health, (With<PlayerMarker>, Without<ActorMarker>)>,
    mut actors: Query<(&ActorId, &mut Health), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let dt = time.delta_secs();
    let health_config = &server_gameplay_config.combat.health;

    // Players use the single shared player config.
    let player_health = health_config.player;
    let player_gain = player_health.max * player_health.regen_rate * dt;
    if player_gain > 0.0 {
        for mut health in &mut players {
            regenerate_health(&mut health, player_health.max, player_gain);
        }
    }

    // Actors regenerate per kind.
    for (id, mut health) in &mut actors {
        let Some(info) = actors_map.get(id) else {
            continue;
        };
        if health.0 <= 0.0 {
            continue;
        }
        let actor_health = health_config.expect_actor(&info.spawn_kind);
        let gain = actor_health.max * actor_health.regen_rate * dt;
        if gain <= 0.0 {
            continue;
        }
        regenerate_health(&mut health, actor_health.max, gain);
    }
}
