use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use crate::{
    config::ServerGameplayConfig,
    constants::ACTOR_MOVE_INTENT_SEND_COOLDOWN,
    resources::{ActorInfo, ActorMap, ActorSpawnThrottles, ActorSpawner, MapConfig},
    systems::characters::generate_actor_spawn_position_in_zone,
};
use common::{
    config::GameplayConfig,
    markers::{ActorMarker, PlayerMarker},
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMoveIntent, FaceDirection, Health, Position},
};

// What to do for a single (zone, kind) slot on a given tick.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SpawnDecision {
    // Slot is at quota — do not tick the throttle, do not spawn.
    Skip,
    // Slot is short but the throttle is still running — tick it down.
    Tick,
    // Slot is short and throttle has expired — spawn one actor and reset.
    Spawn,
}

fn decide_spawn(live: u32, count: u32, throttle: f32) -> SpawnDecision {
    if live >= count {
        SpawnDecision::Skip
    } else if throttle > 0.0 {
        SpawnDecision::Tick
    } else {
        SpawnDecision::Spawn
    }
}

// Walk every actor spawn zone and apply the throttle algorithm per slot:
//   - Skip if at quota (do not touch the throttle).
//   - Tick the throttle down by `dt` if the slot is short and the throttle
//     hasn't expired yet.
//   - Otherwise spawn one actor and reset the throttle to the kind's
//     `spawn_throttle_seconds`.
//
// The first time a slot is seen the throttle entry doesn't exist yet; it's
// inserted as 0.0, which means "go now." That's how boot starts spawning
// immediately: the very first tick picks Spawn for every slot, then sets
// the throttle so subsequent spawns are rate-limited.
pub fn actor_spawn_quota_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    mut spawner: ResMut<ActorSpawner>,
    mut throttles: ResMut<ActorSpawnThrottles>,
    time: Res<Time>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    players: Query<&Position, With<PlayerMarker>>,
) {
    let dt = time.delta_secs();
    // Avoid spawning on top of players. Existing actors aren't in this list —
    // we don't have a Position query for them here, and physics will resolve
    // any overlap on the next tick.
    let mut occupied_positions: Vec<Position> = players.iter().copied().collect();
    let mut rng = rng();

    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        // Configs are cross-validated against the map at startup, so any
        // zone kind here is guaranteed to resolve in both configs.
        let actor_config = gameplay_config
            .actor(&zone.kind)
            .expect("zone kind validated at startup");
        let kind_server_config = server_gameplay_config
            .actor(&zone.kind)
            .expect("zone kind validated at startup");
        let actor_physics = actor_config.physics();
        let throttle_seconds = kind_server_config.spawn_throttle_seconds;

        let live = live_actor_count(&actors, zone_idx);
        let throttle = throttles.0.entry(zone_idx).or_insert(0.0);

        match decide_spawn(live, zone.count, *throttle) {
            // Frozen — leave the throttle as-is so the next death pays a
            // full throttle wait.
            SpawnDecision::Skip => {}
            SpawnDecision::Tick => *throttle -= dt,
            SpawnDecision::Spawn => {
                spawn_actor_in_zone(
                    &mut commands,
                    &mut actors,
                    &mut spawner,
                    &mut occupied_positions,
                    &mut rng,
                    &map_config,
                    &collision_world,
                    actor_config,
                    kind_server_config,
                    actor_physics,
                    zone_idx,
                    &zone.kind,
                );
                *throttle = throttle_seconds;
            }
        }
    }
}

fn live_actor_count(actors: &ActorMap, zone_idx: usize) -> u32 {
    actors
        .0
        .values()
        .filter(|info| info.spawn_zone_index == zone_idx)
        .count() as u32
}

#[allow(clippy::too_many_arguments)]
fn spawn_actor_in_zone(
    commands: &mut Commands,
    actors: &mut ActorMap,
    spawner: &mut ActorSpawner,
    occupied_positions: &mut Vec<Position>,
    rng: &mut ThreadRng,
    map_config: &MapConfig,
    collision_world: &CollisionWorld,
    actor_config: &common::config::ActorGameplayConfig,
    kind_server_config: &crate::config::ActorKindServerConfig,
    actor_physics: common::config::CharacterPhysicsConfig,
    zone_idx: usize,
    spawn_kind: &str,
) {
    let pos =
        generate_actor_spawn_position_in_zone(map_config, zone_idx, collision_world, occupied_positions, actor_physics);
    occupied_positions.push(pos);

    let direction = rng.random_range(0.0..std::f32::consts::TAU);
    let move_intent = ActorMoveIntent::Moving {
        direction,
        speed: actor_config.patrol_speed,
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
            Health(actor_config.health().max),
        ))
        .id();

    actors.0.insert(
        actor_id,
        ActorInfo {
            entity,
            spawn_zone_index: zone_idx,
            spawn_kind: spawn_kind.to_string(),
            direction_timer: random_direction_time(rng, kind_server_config),
            patrol_intent: move_intent,
            go_to_position: None,
            wall_avoidance_direction: None,
            last_broadcast_move_intent: move_intent,
            move_intent_send_timer: ACTOR_MOVE_INTENT_SEND_COOLDOWN,
        },
    );
}

fn random_direction_time(rng: &mut ThreadRng, kind_server_config: &crate::config::ActorKindServerConfig) -> f32 {
    rng.random_range(kind_server_config.min_direction_time..=kind_server_config.max_direction_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_skip_when_full() {
        // Throttle value is irrelevant when the slot is at quota.
        assert_eq!(decide_spawn(3, 3, 0.0), SpawnDecision::Skip);
        assert_eq!(decide_spawn(3, 3, 5.0), SpawnDecision::Skip);
        assert_eq!(decide_spawn(5, 3, 5.0), SpawnDecision::Skip);
    }

    #[test]
    fn decide_tick_when_throttle_positive() {
        assert_eq!(decide_spawn(0, 3, 0.5), SpawnDecision::Tick);
        assert_eq!(decide_spawn(2, 3, 1.0), SpawnDecision::Tick);
    }

    #[test]
    fn decide_spawn_when_throttle_zero_or_negative() {
        assert_eq!(decide_spawn(0, 3, 0.0), SpawnDecision::Spawn);
        assert_eq!(decide_spawn(2, 3, -0.5), SpawnDecision::Spawn);
    }

    // Simulate the per-slot loop without any Bevy world: walk a sequence of
    // ticks and apply decide_spawn's verdict to (live, throttle).
    fn simulate(initial_live: u32, count: u32, delay: f32, dt: f32, ticks: usize) -> Vec<u32> {
        let mut live = initial_live;
        let mut throttle = 0.0_f32;
        let mut history = Vec::with_capacity(ticks);
        for _ in 0..ticks {
            match decide_spawn(live, count, throttle) {
                SpawnDecision::Skip => {}
                SpawnDecision::Tick => throttle -= dt,
                SpawnDecision::Spawn => {
                    live += 1;
                    throttle = delay;
                }
            }
            history.push(live);
        }
        history
    }

    #[test]
    fn boot_sequence_spawns_at_throttle_intervals() {
        // count=3, delay=2.0s, dt=1.0s. Tick 1 spawns (throttle 0 → spawn,
        // throttle = 2). Tick 2 ticks (throttle 2 → 1). Tick 3 ticks
        // (throttle 1 → 0). Tick 4 spawns again (throttle 0 → spawn,
        // throttle = 2). Etc. Slot fills at tick 7.
        let history = simulate(0, 3, 2.0, 1.0, 8);
        assert_eq!(history, vec![1, 1, 1, 2, 2, 2, 3, 3]);
    }

    #[test]
    fn kill_after_full_pays_full_delay() {
        // Drive the slot to full first, then drop it and verify the throttle
        // takes a full `delay` to reach zero before another spawn fires.
        // Throttle starts at 2.0s — what the spawn that filled the slot left
        // behind. dt=0.5 means it takes 4 ticks to reach zero.
        let mut live = 3;
        let mut throttle = 2.0_f32;
        let count = 3;
        let dt = 0.5;

        // While full, decide is Skip and the throttle stays frozen at 2.0.
        for _ in 0..10 {
            assert_eq!(decide_spawn(live, count, throttle), SpawnDecision::Skip);
        }

        // A death drops live; throttle is unchanged.
        live -= 1;

        // Now the throttle ticks 4 times (at dt=0.5) before reaching zero.
        for _ in 0..4 {
            assert_eq!(decide_spawn(live, count, throttle), SpawnDecision::Tick);
            throttle -= dt;
        }

        // Throttle is now 0.0; next decision is Spawn.
        assert_eq!(decide_spawn(live, count, throttle), SpawnDecision::Spawn);
    }

    #[test]
    fn continuous_kills_do_not_starve_the_slot() {
        // count=1: kill the actor every tick, verify the spawner replaces it
        // every `delay` regardless of how often deaths happen. (With the old
        // cooldown-on-death model, this would starve forever because every
        // death reset the timer.)
        let count = 1;
        // Per-spawn throttle reset, applied below when a Spawn fires.
        let delay = 2.0_f32;
        let dt = 1.0;

        let mut live = 0;
        let mut throttle = 0.0;
        let mut spawns = 0;
        for _ in 0..10 {
            match decide_spawn(live, count, throttle) {
                SpawnDecision::Skip => {
                    // simulate kill: drop live to 0 immediately
                    live = 0;
                }
                SpawnDecision::Tick => throttle -= dt,
                SpawnDecision::Spawn => {
                    live += 1;
                    throttle = delay;
                    spawns += 1;
                }
            }
        }
        // Without the throttle every kill would respawn immediately (10
        // spawns). With the throttle, one spawn at t=0, then one every
        // `delay`/dt = 2 ticks. Over 10 ticks: spawns at ticks 0, 3, 6, 9
        // (spawn → kill → 2 ticks → spawn ...). Either way the count is
        // bounded and not zero.
        assert!(
            spawns >= 3,
            "expected spawns to keep arriving under continuous kills, got {spawns}"
        );
    }
}
