use bevy::prelude::*;

use crate::resources::ActorMap;
use common::{
    config::GameplayConfig,
    health::regenerate_health,
    protocol::{ActorId, ActorMarker, Health, PlayerMarker},
};

pub fn characters_health_regeneration_system(
    time: Res<Time>,
    gameplay_config: Res<GameplayConfig>,
    actors_map: Res<ActorMap>,
    mut players: Query<&mut Health, (With<PlayerMarker>, Without<ActorMarker>)>,
    mut actors: Query<(&ActorId, &mut Health), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let dt = time.delta_secs();

    // Players use the single shared player config.
    let player_health = gameplay_config.player.health();
    if player_health.regeneration_per_second > 0.0 {
        let gain = player_health.regeneration_per_second * dt;
        for mut health in &mut players {
            regenerate_health(&mut health, player_health.max, gain);
        }
    }

    // Actors regenerate per kind.
    for (id, mut health) in &mut actors {
        let Some(info) = actors_map.0.get(id) else {
            continue;
        };
        let actor_health = gameplay_config
            .actor(&info.spawn_kind)
            .expect("actor kind validated at startup")
            .health();
        if actor_health.regeneration_per_second <= 0.0 {
            continue;
        }
        let gain = actor_health.regeneration_per_second * dt;
        regenerate_health(&mut health, actor_health.max, gain);
    }
}
