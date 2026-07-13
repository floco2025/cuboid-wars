use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use crate::{
    actors::behavior::random_direction_time,
    actors::{ActorInfo, ActorMap, ActorSpawnThrottles, ActorSpawner, PendingActorSpawn, PendingActorSpawns},
    characters::generate_actor_spawn_position_in_zone,
    config::ServerGameplayConfig,
    map::MapConfig,
};
use common::{
    config::GameplayConfig,
    map::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMarker, ActorMoveIntent, FaceDirection, Health, PlayerMarker, Position},
};

// Per-tick decision for one zone: spawn now, tick the cooldown down, or skip.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SpawnDecision {
    // Slot is at quota — prime the throttle so the next death pays a full
    // cooldown wait. (Leaving the throttle at 0 here would let the very
    // first death respawn instantly.)
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

// Startup-only: fill every spawn zone to its `count`. Runs once when the
// world boots, irrespective of `respawns` — initial fill is universal.
// Spawns are queued, not spawned: each waits out its beam-in warning window
// in `PendingActorSpawns` before `actor_pending_spawn_system` materializes it.
pub fn actor_initial_spawn_system(
    mut pending: ResMut<PendingActorSpawns>,
    mut spawner: ResMut<ActorSpawner>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    players: Query<&Position, With<PlayerMarker>>,
) {
    // Avoid spawning on top of any players that may already exist (none, in
    // practice, at Startup — but cheap and consistent with the respawn path).
    let mut occupied_positions: Vec<Position> = players.iter().copied().collect();
    let mut rng = rng();

    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        // Configs are cross-validated against the map at startup, so any
        // zone kind here is guaranteed to resolve in both configs.
        let actor_config = gameplay_config.validated_actor(&zone.kind);
        let kind_server_config = server_gameplay_config.validated_actor(&zone.kind);
        let actor_physics = actor_config.physics();
        for _ in 0..zone.count {
            queue_actor_spawn_in_zone(
                &mut pending,
                &mut spawner,
                &mut occupied_positions,
                &mut rng,
                &map_config,
                &map_geometry,
                &collision_world,
                kind_server_config,
                actor_physics,
                zone_idx,
                &zone.kind,
            );
        }
    }
}

// Per-tick: refill zones whose kind opted into respawning. Non-respawning
// kinds are skipped; their `ActorSpawnThrottles` entries are never inserted.
//
// For respawning kinds, the throttle clock is the existing model: it sits at
// 0 while the slot is full; on a death the throttle clock starts ticking
// (via `Tick`) and only reaches 0 — at which point a `Spawn` fires and the
// throttle is reset to the kind's `spawn_throttle_time`.
pub fn actor_respawn_system(
    mut pending: ResMut<PendingActorSpawns>,
    mut spawner: ResMut<ActorSpawner>,
    mut throttles: ResMut<ActorSpawnThrottles>,
    actors: Res<ActorMap>,
    time: Res<Time>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
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

    // One pass for per-zone live counts instead of rescanning the whole
    // ActorMap once per zone (O(zones·actors)). Each zone reads its count once
    // before its single possible spawn, and a spawn only ever adds to its own
    // zone, so this tick-start snapshot stays correct for the read.
    let mut live_by_zone = vec![0u32; map_config.actor_spawn_zones.len()];
    for info in actors.values() {
        if let Some(count) = live_by_zone.get_mut(info.spawn_zone_index) {
            *count += 1;
        }
    }
    // Pending spawns count toward their zone's quota (materialization is
    // unconditional, so they are as good as live) and reserve their spot so
    // a second spawn can't pick it during the warning window.
    for entry in &pending.0 {
        if let Some(count) = live_by_zone.get_mut(entry.zone_idx) {
            *count += 1;
        }
        occupied_positions.push(entry.pos);
    }

    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        let kind_server_config = server_gameplay_config.validated_actor(&zone.kind);
        if !kind_server_config.respawn.enabled {
            // One-shot kind: no replacement after deaths, ever.
            continue;
        }
        let actor_config = gameplay_config.validated_actor(&zone.kind);
        let actor_physics = actor_config.physics();
        let throttle_time = kind_server_config.respawn.delay_secs;

        let live = live_by_zone[zone_idx];
        let throttle = throttles.0.entry(zone_idx).or_insert(0.0);

        match decide_spawn(live, zone.count, *throttle) {
            // Slot is full: keep the throttle primed at `throttle_time` so
            // the next death starts the countdown from full. Lazily-inserted
            // entries start at 0.0, which without this would let the first
            // death after startup respawn instantly.
            SpawnDecision::Skip => *throttle = throttle_time,
            SpawnDecision::Tick => *throttle -= dt,
            SpawnDecision::Spawn => {
                queue_actor_spawn_in_zone(
                    &mut pending,
                    &mut spawner,
                    &mut occupied_positions,
                    &mut rng,
                    &map_config,
                    &map_geometry,
                    &collision_world,
                    kind_server_config,
                    actor_physics,
                    zone_idx,
                    &zone.kind,
                );
                *throttle = throttle_time;
            }
        }
    }
}

// Ticks the beam-in warning windows and materializes due spawns at their
// reserved spot, unconditionally — a player squatting on it resolves via
// contact detonation on the next tick. Runs at the head of the network chain
// (see main.rs) so a pending entry's removal and its actor's appearance land
// in the same snapshot.
pub fn actor_pending_spawn_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    mut pending: ResMut<PendingActorSpawns>,
    time: Res<Time>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
) {
    let due = take_due_spawns(&mut pending.0, time.delta_secs());
    if due.is_empty() {
        return;
    }
    let mut rng = rng();
    for spawn in due {
        materialize_actor(
            &mut commands,
            &mut actors,
            &mut rng,
            gameplay_config.validated_actor(&spawn.kind),
            server_gameplay_config.validated_actor(&spawn.kind),
            spawn,
        );
    }
}

fn take_due_spawns(pending: &mut Vec<PendingActorSpawn>, dt: f32) -> Vec<PendingActorSpawn> {
    for entry in pending.iter_mut() {
        entry.remaining_secs -= dt;
    }
    let (due, rest): (Vec<_>, Vec<_>) = pending.drain(..).partition(|entry| entry.remaining_secs <= 0.0);
    *pending = rest;
    due
}

// Reserve an id, spot, and heading for one actor and queue it for beam-in.
// The heading is rolled now so the client ghost and the materialized actor
// face the same way.
#[allow(clippy::too_many_arguments)]
fn queue_actor_spawn_in_zone(
    pending: &mut PendingActorSpawns,
    spawner: &mut ActorSpawner,
    occupied_positions: &mut Vec<Position>,
    rng: &mut ThreadRng,
    map_config: &MapConfig,
    map_geometry: &MapGeometry,
    collision_world: &CollisionWorld,
    kind_server_config: &crate::config::ActorKindServerConfig,
    actor_physics: common::config::CharacterPhysicsConfig,
    zone_idx: usize,
    spawn_kind: &str,
) {
    let pos = generate_actor_spawn_position_in_zone(
        map_config,
        map_geometry,
        zone_idx,
        collision_world,
        occupied_positions,
        actor_physics,
    );
    occupied_positions.push(pos);

    pending.0.push(PendingActorSpawn {
        actor_id: spawner.allocate(),
        zone_idx,
        kind: spawn_kind.to_string(),
        pos,
        face_dir: rng.random_range(0.0..std::f32::consts::TAU),
        remaining_secs: kind_server_config.respawn.warning_secs,
        warning_secs: kind_server_config.respawn.warning_secs,
    });
}

fn materialize_actor(
    commands: &mut Commands,
    actors: &mut ActorMap,
    rng: &mut ThreadRng,
    actor_config: &common::config::ActorGameplayConfig,
    kind_server_config: &crate::config::ActorKindServerConfig,
    spawn: PendingActorSpawn,
) {
    let move_intent = ActorMoveIntent::Moving {
        direction: spawn.face_dir,
        speed: actor_config.patrol_speed,
    };
    let entity = commands
        .spawn((
            ActorMarker,
            spawn.actor_id,
            spawn.pos,
            move_intent,
            FaceDirection(spawn.face_dir),
            CharacterVerticalVelocity::default(),
            Health(actor_config.health().max),
        ))
        .id();

    actors.insert(
        spawn.actor_id,
        ActorInfo::new(
            entity,
            spawn.zone_idx,
            spawn.kind,
            crate::actors::ActorGoal::Patrol {
                intent: move_intent,
                direction_timer: random_direction_time(rng, kind_server_config),
                ledge_escape_timer: 0.0,
            },
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::ActorId;

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

    // Regression: the throttle map is lazily inserted at 0.0 on first
    // access, which without the `Skip => prime` arm would let the very
    // first death after startup respawn instantly while subsequent deaths
    // paid the full delay.
    #[test]
    fn first_death_after_startup_pays_full_delay() {
        let count = 1;
        let delay = 2.0_f32;
        let dt = 0.5;

        // Simulate the production loop's body for one zone, including the
        // lazy-insert at 0.0 and the Skip-prime fix.
        let mut live = count; // initial spawn filled the slot
        let mut throttle = 0.0_f32; // lazy-insert default

        // A few ticks while full: each Skip should prime the throttle so a
        // subsequent death starts from `delay`, not from 0.
        for _ in 0..5 {
            match decide_spawn(live, count, throttle) {
                SpawnDecision::Skip => throttle = delay,
                SpawnDecision::Tick => throttle -= dt,
                SpawnDecision::Spawn => {
                    live += 1;
                    throttle = delay;
                }
            }
        }
        assert_eq!(throttle, delay, "Skip should keep the throttle primed");

        // Now kill — the throttle must Tick down for `delay` seconds before
        // the next Spawn is decided.
        live -= 1;
        let ticks_to_zero = (delay / dt).ceil() as u32;
        for _ in 0..ticks_to_zero {
            assert_eq!(
                decide_spawn(live, count, throttle),
                SpawnDecision::Tick,
                "first death after startup must wait the full delay"
            );
            throttle -= dt;
        }
        assert_eq!(decide_spawn(live, count, throttle), SpawnDecision::Spawn);
    }

    fn pending_spawn(id: u32, remaining_secs: f32) -> PendingActorSpawn {
        PendingActorSpawn {
            actor_id: ActorId(id),
            zone_idx: 0,
            kind: "mine_1".to_string(),
            pos: Position::default(),
            face_dir: 0.0,
            remaining_secs,
            warning_secs: 2.0,
        }
    }

    #[test]
    fn not_due_spawns_tick_down_and_stay_queued() {
        let mut pending = vec![pending_spawn(1, 2.0), pending_spawn(2, 0.5)];

        let due = take_due_spawns(&mut pending, 0.25);

        assert!(due.is_empty());
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].remaining_secs, 1.75);
        assert_eq!(pending[1].remaining_secs, 0.25);
    }

    #[test]
    fn due_spawns_drain_in_queue_order() {
        let mut pending = vec![pending_spawn(1, 0.1), pending_spawn(2, 5.0), pending_spawn(3, 0.2)];

        let due = take_due_spawns(&mut pending, 0.3);

        assert_eq!(
            due.iter().map(|spawn| spawn.actor_id).collect::<Vec<_>>(),
            vec![ActorId(1), ActorId(3)]
        );
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].actor_id, ActorId(2));
    }

    #[test]
    fn window_reaching_exactly_zero_is_due() {
        let mut pending = vec![pending_spawn(1, 0.5)];

        let due = take_due_spawns(&mut pending, 0.5);

        assert_eq!(due.len(), 1);
        assert!(pending.is_empty());
    }
}
