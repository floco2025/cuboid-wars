use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};
use std::f32::consts::TAU;

use crate::{
    actors::{ActorInfo, ActorMap, ActorSpawnThrottles, ActorSpawner, PendingActorSpawn, PendingActorSpawns},
    characters::generate_actor_spawn_position_in_zone,
    config::ServerGameplayConfig,
    map::MapConfig,
};
use common::{
    config::GameplayConfig,
    map::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMarker, ActorMoveIntent, FaceYaw, Health, PlayerMarker, Position},
};

// Per-tick decision for one zone: refill it, tick the cooldown down, or skip.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SpawnDecision {
    // Slot is at quota — prime the throttle so the next death pays a full
    // cooldown wait. (Leaving the throttle at 0 here would let the very
    // first death respawn instantly.)
    Skip,
    // Slot is short but the throttle is still running — tick it down.
    Tick,
    // Zone is short and throttle has expired — refill every missing slot.
    Refill,
}

fn decide_spawn(live: u32, count: u32, throttle: f32) -> SpawnDecision {
    if live >= count {
        SpawnDecision::Skip
    } else if throttle > 0.0 {
        SpawnDecision::Tick
    } else {
        SpawnDecision::Refill
    }
}

// Startup-only: fill every spawn zone to its `count`. Runs once when the
// world boots, irrespective of `respawns` — initial fill is universal.
// Spawns are queued, not spawned: each waits out its beam-in warning window
// in `PendingActorSpawns` before `actors_pending_spawn_system` materializes it.
pub fn actors_initial_spawn_system(
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
        let actor_config = gameplay_config.expect_actor(&zone.kind);
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
                server_gameplay_config.actor_settings.spawn_warning_secs,
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
// (via `Tick`) and only reaches 0 — at which point every missing slot is
// queued together and the throttle is reset to the kind's respawn delay.
pub fn actors_respawn_system(
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
    actor_positions: Query<&Position, (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let dt = time.delta_secs();
    let mut occupied_positions: Vec<Position> = players.iter().chain(&actor_positions).copied().collect();
    let mut rng = rng();

    // One pass for per-zone live counts instead of rescanning the whole
    // ActorMap once per zone (O(zones·actors)). Each zone reads its count once
    // before its possible batch refill, which only adds to that same zone.
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
        let kind_server_config = server_gameplay_config.expect_actor(&zone.kind);
        let Some(throttle_time) = kind_server_config.respawn_delay_secs else {
            continue;
        };
        let actor_config = gameplay_config.expect_actor(&zone.kind);
        let actor_physics = actor_config.physics();

        let live = live_by_zone[zone_idx];
        let throttle = throttles.0.entry(zone_idx).or_insert(0.0);

        match decide_spawn(live, zone.count, *throttle) {
            // Slot is full: keep the throttle primed at `throttle_time` so
            // the next death starts the countdown from full. Lazily-inserted
            // entries start at 0.0, which without this would let the first
            // death after startup respawn instantly.
            SpawnDecision::Skip => *throttle = throttle_time,
            SpawnDecision::Tick => *throttle -= dt,
            SpawnDecision::Refill => {
                for _ in live..zone.count {
                    queue_actor_spawn_in_zone(
                        &mut pending,
                        &mut spawner,
                        &mut occupied_positions,
                        &mut rng,
                        &map_config,
                        &map_geometry,
                        &collision_world,
                        server_gameplay_config.actor_settings.spawn_warning_secs,
                        actor_physics,
                        zone_idx,
                        &zone.kind,
                    );
                }
                *throttle = throttle_time;
            }
        }
    }
}

pub(crate) fn expire_actor_spawn_cooldowns(
    actors: &ActorMap,
    pending: &mut PendingActorSpawns,
    throttles: &mut ActorSpawnThrottles,
    map_config: &MapConfig,
    server_gameplay_config: &ServerGameplayConfig,
    actor_kind: Option<&str>,
) -> usize {
    let mut occupied_by_zone = vec![0u32; map_config.actor_spawn_zones.len()];
    for info in actors.values() {
        if let Some(count) = occupied_by_zone.get_mut(info.spawn_zone_index) {
            *count += 1;
        }
    }
    let mut respawning = 0usize;
    for spawn in &mut pending.0 {
        if let Some(count) = occupied_by_zone.get_mut(spawn.zone_idx) {
            *count += 1;
        }
        if actor_kind.is_none_or(|kind| spawn.kind == kind) {
            spawn.remaining_secs = 0.0;
            respawning += 1;
        }
    }

    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        if actor_kind.is_some_and(|kind| zone.kind != kind) {
            continue;
        }
        let kind_server_config = server_gameplay_config.expect_actor(&zone.kind);
        if kind_server_config.respawn_delay_secs.is_some() {
            let missing = zone.count.saturating_sub(occupied_by_zone[zone_idx]);
            if missing > 0 {
                throttles.0.insert(zone_idx, 0.0);
                respawning += missing as usize;
            }
        }
    }

    respawning
}

// Ticks the beam-in warning windows and materializes due spawns at their
// reserved spot, unconditionally — a player squatting on it resolves via
// contact detonation on the next tick. Runs at the head of the network chain
// (see main.rs) so a pending entry's removal and its actor's appearance land
// in the same snapshot.
pub fn actors_pending_spawn_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    mut pending: ResMut<PendingActorSpawns>,
    time: Res<Time>,
    gameplay_config: Res<GameplayConfig>,
) {
    let due = take_due_spawns(&mut pending.0, time.delta_secs());
    if due.is_empty() {
        return;
    }
    for spawn in due {
        materialize_actor(
            &mut commands,
            &mut actors,
            gameplay_config.expect_actor(&spawn.kind),
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
    warning_secs: f32,
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
        face_yaw: rng.random_range(0.0..TAU),
        remaining_secs: warning_secs,
        warning_secs,
    });
}

fn materialize_actor(
    commands: &mut Commands,
    actors: &mut ActorMap,
    actor_config: &common::config::ActorGameplayConfig,
    spawn: PendingActorSpawn,
) {
    let move_intent = ActorMoveIntent::Idle;
    let entity = commands
        .spawn((
            ActorMarker,
            spawn.actor_id,
            spawn.pos,
            move_intent,
            FaceYaw(spawn.face_yaw),
            CharacterVerticalVelocity::default(),
            Health(actor_config.health().max),
        ))
        .id();

    actors.insert(spawn.actor_id, ActorInfo::new(entity, spawn.zone_idx, spawn.kind));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid};
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
    fn decide_refill_when_throttle_zero_or_negative() {
        assert_eq!(decide_spawn(0, 3, 0.0), SpawnDecision::Refill);
        assert_eq!(decide_spawn(2, 3, -0.5), SpawnDecision::Refill);
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

        // Throttle is now 0.0; next decision is Refill.
        assert_eq!(decide_spawn(live, count, throttle), SpawnDecision::Refill);
    }

    #[test]
    fn continuous_kills_do_not_starve_the_slot() {
        // count=1: kill the actor every tick, verify the spawner replaces it
        // every `delay` regardless of how often deaths happen. (With the old
        // cooldown-on-death model, this would starve forever because every
        // death reset the timer.)
        let count = 1;
        // Per-refill throttle reset, applied below when a Refill fires.
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
                SpawnDecision::Refill => {
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
                SpawnDecision::Refill => {
                    live += 1;
                    throttle = delay;
                }
            }
        }
        assert_eq!(throttle, delay, "Skip should keep the throttle primed");

        // Now kill — the throttle must Tick down for `delay` seconds before
        // the next Refill is decided.
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
        assert_eq!(decide_spawn(live, count, throttle), SpawnDecision::Refill);
    }

    fn pending_spawn(id: u32, remaining_secs: f32) -> PendingActorSpawn {
        PendingActorSpawn {
            actor_id: ActorId(id),
            zone_idx: 0,
            kind: "zapper".to_string(),
            pos: Position::default(),
            face_yaw: 0.0,
            remaining_secs,
            warning_secs: 2.0,
        }
    }

    #[test]
    fn expiring_selected_cooldowns_advances_pending_and_missing_slots() {
        let map_config = MapConfig {
            levels: vec![LevelGrid {
                cells: CellGrid::new(1, 1),
                edges: EdgeGrid::new(1, 1),
                barrier_edges: EdgeGrid::new(1, 1),
            }],
            actor_spawn_zones: vec![
                ActorSpawnZone {
                    level: 0,
                    cols: [0, 1],
                    rows: [0, 1],
                    kind: "mine".to_owned(),
                    count: 2,
                },
                ActorSpawnZone {
                    level: 0,
                    cols: [0, 1],
                    rows: [0, 1],
                    kind: "zapper".to_owned(),
                    count: 1,
                },
            ],
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        };
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let mut mine = pending_spawn(1, 2.0);
        mine.kind = "mine".to_owned();
        let mut zapper = pending_spawn(2, 2.0);
        zapper.zone_idx = 1;
        let mut pending = PendingActorSpawns(vec![mine, zapper]);
        let mut throttles = ActorSpawnThrottles::default();
        throttles.0.insert(0, 60.0);
        throttles.0.insert(1, 120.0);

        let count = expire_actor_spawn_cooldowns(
            &ActorMap::default(),
            &mut pending,
            &mut throttles,
            &map_config,
            &config,
            Some("mine"),
        );

        assert_eq!(count, 2);
        assert_eq!(pending.0[0].remaining_secs, 0.0);
        assert_eq!(pending.0[1].remaining_secs, 2.0);
        assert_eq!(throttles.0[&0], 0.0);
        assert_eq!(throttles.0[&1], 120.0);
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
