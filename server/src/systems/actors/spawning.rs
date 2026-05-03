use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use crate::{
    config::ServerGameplayConfig,
    constants::ACTOR_MOVE_INTENT_SEND_COOLDOWN,
    resources::{ActorInfo, ActorMap, ActorSpawner, MapConfig},
    systems::characters::generate_actor_spawn_position_in_zone,
};
use common::{
    config::GameplayConfig,
    markers::{ActorMarker, PlayerMarker},
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorKind, ActorMoveIntent, FaceDirection, Health, Position},
};

const ACTOR_SPAWN_KIND: &str = "actor";

// Top each spawn zone up to its `actor` quota every tick. Idempotent: if the
// zone is already at quota, nothing happens. Replacements happen here too,
// since `actor_death_system` despawns actors and removes them from `ActorMap`.
pub fn actor_spawn_quota_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    mut spawner: ResMut<ActorSpawner>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    players: Query<&Position, With<PlayerMarker>>,
) {
    let actor_physics = gameplay_config.characters.actor.physics();
    let actor_max_health = gameplay_config.characters.actor.health().max;
    // Avoid spawning on top of players. Existing actors aren't in this list —
    // we don't have a Position query for them here, and physics will resolve
    // any overlap on the next tick.
    let mut occupied_positions: Vec<Position> = players.iter().copied().collect();
    let mut rng = rng();

    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        let Some(entry) = zone.entry_for(ACTOR_SPAWN_KIND) else {
            continue;
        };
        let live = actors
            .0
            .values()
            .filter(|info| info.spawn_zone_index == zone_idx && info.spawn_kind == ACTOR_SPAWN_KIND)
            .count() as u32;
        if live >= entry.count {
            continue;
        }
        let to_spawn = entry.count - live;
        for _ in 0..to_spawn {
            let pos = generate_actor_spawn_position_in_zone(
                &map_config,
                zone_idx,
                &collision_world,
                &occupied_positions,
                actor_physics,
            );
            occupied_positions.push(pos);

            let direction = rng.random_range(0.0..std::f32::consts::TAU);
            let move_intent = ActorMoveIntent::Moving {
                direction,
                speed: gameplay_config.characters.actor.patrol_speed,
            };
            let actor_id = spawner.allocate();
            let entity = commands
                .spawn((
                    ActorMarker,
                    actor_id,
                    pos,
                    move_intent,
                    FaceDirection(direction),
                    CharacterVerticalVelocity::default(),
                    Health(actor_max_health),
                ))
                .id();

            actors.0.insert(
                actor_id,
                ActorInfo {
                    entity,
                    kind: ActorKind::Automaton,
                    spawn_zone_index: zone_idx,
                    spawn_kind: ACTOR_SPAWN_KIND.to_string(),
                    direction_timer: random_direction_time(&mut rng, &server_gameplay_config),
                    patrol_intent: move_intent,
                    go_to_position: None,
                    wall_avoidance_direction: None,
                    last_broadcast_move_intent: move_intent,
                    move_intent_send_timer: ACTOR_MOVE_INTENT_SEND_COOLDOWN,
                },
            );
        }
    }
}

fn random_direction_time(rng: &mut ThreadRng, server_gameplay_config: &ServerGameplayConfig) -> f32 {
    rng.random_range(
        server_gameplay_config.actors.min_direction_time..=server_gameplay_config.actors.max_direction_time,
    )
}
