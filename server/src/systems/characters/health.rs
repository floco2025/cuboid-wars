use bevy::{ecs::query::QueryFilter, prelude::*};

use common::{
    config::GameplayConfig,
    markers::{ActorMarker, PlayerMarker},
    protocol::Health,
};

pub fn characters_health_regeneration_system(
    time: Res<Time>,
    gameplay_config: Res<GameplayConfig>,
    mut players: Query<&mut Health, (With<PlayerMarker>, Without<ActorMarker>)>,
    mut actors: Query<&mut Health, (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    regenerate_health(
        &mut players,
        gameplay_config.characters.player.health().max,
        gameplay_config.characters.player.health().regeneration_per_second,
        time.delta_secs(),
    );
    regenerate_health(
        &mut actors,
        gameplay_config.characters.actor.health().max,
        gameplay_config.characters.actor.health().regeneration_per_second,
        time.delta_secs(),
    );
}

fn regenerate_health<F: QueryFilter>(
    query: &mut Query<&mut Health, F>,
    max_health: f32,
    regeneration_per_second: f32,
    delta: f32,
) {
    if regeneration_per_second <= 0.0 {
        return;
    }

    let gain = regeneration_per_second * delta;
    for mut health in query {
        health.0 = (health.0 + gain).min(max_health);
    }
}
