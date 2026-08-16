use anyhow::{Context, Result, bail};
use bevy::prelude::*;
use clap::Parser;
use quinn::Endpoint;
use std::{collections::HashSet, net::SocketAddr};
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{self, Duration, Instant, MissedTickBehavior},
};

use common::{config::GameplayConfig, constants::TICK_HZ, physics::CollisionWorld};
use server::{
    actors::{ActorMap, ActorSpawnThrottles, ActorSpawner, PendingActorSpawns},
    items::{ItemMap, ItemSpawner, RandomItems},
    map::MapConfig,
    players::PlayerMap,
};
use server::{
    actors::{
        actor_behavior_system, actor_initial_spawn_system, actor_pending_spawn_system, actor_removal_system,
        actor_respawn_system, navigation::NavGraph,
    },
    characters::{characters_health_regeneration_system, characters_movement_system, knockback_decay_system},
    combat::{PendingExplosions, actor_beam_damage_system, explosions_system},
    config::{ServerGameplayConfig, configure_server},
    items::{
        item_collection_system, placed_item_respawn_system, placed_item_spawn_system, random_item_despawn_system,
        random_item_spawn_system,
    },
    map::{OpenBarrierKinds, WeatherState, compute_open_barrier_kinds_system, generate_map, weather_system},
    missiles::{AirGraph, missiles_guidance_system, missiles_movement_system},
    network::{
        FromClientsChannel, accept_connections_task, network_broadcast_snapshot_system,
        network_process_client_messages_system,
    },
    players::{
        Invincibility, players_fall_damage_system, players_fall_death_system, players_respawn_system,
        players_status_timers_system,
    },
    projectiles::projectiles_movement_system,
};

const LOG_FILTER: &str = "wgpu=error,naga=warn";

// ============================================================================
// CLI Argument Parsing
// ============================================================================

#[derive(Parser)]
#[command(author, version, about = "Cuboid Wars Server", long_about = None)]
struct Args {
    // Address to bind server to
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    bind: String,

    // Make players invincible (overrides `player.invincible` in
    // config/server/gameplay.json)
    #[arg(long)]
    invincible: bool,

    // Map to load (overrides `default_map` in config/server/gameplay.json)
    #[arg(long)]
    map: Option<String>,
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let addr: SocketAddr = args.bind.parse()?;
    let server_config = configure_server()?;
    let gameplay_config = GameplayConfig::load_default()?;
    let mut server_gameplay_config = ServerGameplayConfig::load_default()?;
    if args.invincible {
        server_gameplay_config.player.invincible = true;
    }
    let endpoint = Endpoint::server(server_config, addr)?;
    println!("quic server listening on {addr}");

    let map_name = args.map.unwrap_or_else(|| server_gameplay_config.default_map.clone());
    let Some(map_server_config) = server_gameplay_config.maps.get(&map_name).cloned() else {
        let mut known: Vec<&str> = server_gameplay_config.maps.keys().map(String::as_str).collect();
        known.sort_unstable();
        bail!("unknown map {map_name:?} (available: {known:?})");
    };
    let map_settings = map_server_config.settings.clone();
    let weather_state = WeatherState::new(map_server_config.rain.clone());
    let random_items =
        map_server_config
            .random_items
            .as_ref()
            .map_or_else(RandomItems::default, |random_items_config| RandomItems {
                pool: random_items_config
                    .types
                    .iter()
                    .map(|id| {
                        common::protocol::ItemType::from_config_id(id)
                            .expect("random item type missing from ItemType config ids after config validation")
                    })
                    .collect(),
                max_number: random_items_config.max_number,
                despawn_secs: random_items_config.despawn_secs,
            });

    let barrier_kind_table = common::protocol::BarrierKindTable::from_ids(gameplay_config.barrier_kinds.clone())
        .with_context(|| "failed to build BarrierKindTable from gameplay.json barrier_kinds")?;
    let (map_layout, map_config, map_geometry) = generate_map(&barrier_kind_table, &map_name);
    let collision_world = CollisionWorld::from_map_layout(&map_layout, &barrier_kind_table);
    let nav_graph = NavGraph::new(map_config.clone(), map_geometry);
    let air_graph = AirGraph::new(map_config.clone(), map_geometry);
    validate_actor_kinds_consistent(&gameplay_config, &server_gameplay_config, &map_config)?;

    // Single channel for registrations and per-client messages. Sharing the
    // channel guarantees a player's `Registration` is observed before any of
    // their subsequent messages, which a separate registration channel can't.
    let (to_server, from_clients) = unbounded_channel();

    tokio::spawn(accept_connections_task(endpoint, to_server));
    let mut app = App::new();

    app.add_plugins(MinimalPlugins).add_plugins(bevy::log::LogPlugin {
        level: bevy::log::Level::INFO,
        filter: LOG_FILTER.to_string(),
        ..default()
    });

    info!(
        "generated map {map_name:?}: {} walls, {} ramps, {} floors",
        map_layout.walls.len(),
        map_layout.ramps.len(),
        map_layout.floors.len(),
    );

    app.insert_resource(map_layout)
        .insert_resource(map_settings)
        .insert_resource(weather_state)
        .insert_resource(Invincibility(server_gameplay_config.player.invincible))
        .insert_resource(collision_world)
        .insert_resource(map_config)
        .insert_resource(map_geometry)
        .insert_resource(nav_graph)
        .insert_resource(air_graph)
        .insert_resource(barrier_kind_table)
        .insert_resource(gameplay_config)
        .insert_resource(server_gameplay_config)
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
        .insert_resource(ItemMap::default())
        .insert_resource(ItemSpawner::default())
        .insert_resource(random_items)
        .insert_resource(ActorSpawner::default())
        .insert_resource(ActorSpawnThrottles::default())
        .insert_resource(PendingActorSpawns::default())
        .insert_resource(FromClientsChannel::new(from_clients))
        .insert_resource(PendingExplosions::default())
        .insert_resource(server::missiles::MissileMap::default())
        .insert_resource(OpenBarrierKinds::default())
        .add_systems(Startup, (actor_initial_spawn_system, placed_item_spawn_system))
        .add_systems(
            Update,
            (
                // Network systems must run in order:
                // 1. Materialize due actor spawns, so a pending entry's
                //    removal and its actor's appearance land in the same
                //    snapshot — snapshots go out at 4 Hz, so a split handoff
                //    would leave a ~250 ms neither-ghost-nor-actor gap.
                // 2. ApplyDeferred (flushes pending spawns from step 1 and
                //    from other systems on this tick — e.g.
                //    `random_item_spawn_system` queues a `commands.spawn` +
                //    inserts into `ItemMap` in one shot, so the entity is
                //    reserved but not yet queryable. Without this barrier, a
                //    login-tick `collect_items` panics on the fresh,
                //    unmaterialized item entity).
                // 3. Process client events (registrations + messages, in arrival order)
                // 4. ApplyDeferred (makes login-side spawns queryable)
                // 5. Broadcast state to all clients
                (
                    actor_pending_spawn_system,
                    ApplyDeferred,
                    network_process_client_messages_system,
                    ApplyDeferred,
                    network_broadcast_snapshot_system,
                )
                    .chain(),
                // Actor decisions are written before movement consumes them.
                actor_behavior_system,
                // Before message processing so an admin-forced phase isn't in
                // a same-tick race with the scheduler tick; the broadcast
                // (after processing in the chain) ships the forced intensity
                // this very tick.
                weather_system.before(network_process_client_messages_system),
                // Determine which barrier kinds are open this tick before
                // movement reads the open set for its collision filter.
                compute_open_barrier_kinds_system.before(characters_movement_system),
                characters_movement_system.after(actor_behavior_system),
                // Decay after movement so planning consumed this tick's step.
                knockback_decay_system.after(characters_movement_system),
                // Guidance steers from post-step character positions;
                // movement then integrates, detonates, and queues blasts
                // for this tick's explosion drain. Nested tuple to stay
                // under the `add_systems` tuple arity limit.
                (
                    projectiles_movement_system,
                    missiles_guidance_system.after(characters_movement_system),
                    missiles_movement_system
                        .after(missiles_guidance_system)
                        .before(explosions_system),
                ),
                // Beam burn starts on the burst's first tick and reads
                // post-step positions; a lethal beam's death blast must drain
                // in this tick's explosion pass.
                actor_beam_damage_system
                    .after(actor_behavior_system)
                    .after(characters_movement_system)
                    .before(explosions_system),
                // Actor removal finalizes health-zero actors and queues their blasts.
                actor_removal_system
                    .after(characters_movement_system)
                    .after(projectiles_movement_system),
                characters_health_regeneration_system
                    .after(characters_movement_system)
                    .after(projectiles_movement_system)
                    .after(explosions_system),
                // Fall recovery must run after movement updates positions.
                players_fall_death_system.after(characters_movement_system),
                // Fall damage observes post-step vy, so must follow movement
                // and precede fall_death (a lethal fall-damage hit and a
                // void-fall on the same tick should resolve via the impact
                // path, not the void path).
                players_fall_damage_system
                    .after(characters_movement_system)
                    .before(players_fall_death_system),
                // Death explosions detonate after every kill source has run
                // so a same-tick death chain resolves in one drain.
                explosions_system
                    .after(projectiles_movement_system)
                    .after(actor_removal_system)
                    .after(players_fall_death_system),
                actor_respawn_system.after(explosions_system),
                players_respawn_system,
                players_status_timers_system,
                random_item_spawn_system,
                random_item_despawn_system,
                item_collection_system,
                placed_item_respawn_system,
            ),
        );

    info!("starting ECS server loop...");

    // Run the app in a loop manually at LOOP_FREQUENCY Hz
    let tick_duration = Duration::from_nanos(1_000_000_000 / u64::from(TICK_HZ));
    let mut interval = time::interval(tick_duration);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut frame: u64 = 0;
    loop {
        interval.tick().await;

        let update_start = Instant::now();
        app.update();
        let update_elapsed = update_start.elapsed();

        if update_elapsed > tick_duration {
            warn!(
                "tick {} took {:.2}ms (exceeded {:.2}ms budget)",
                frame,
                update_elapsed.as_secs_f64() * 1000.0,
                tick_duration.as_secs_f64() * 1000.0
            );
        }

        frame += 1;
    }
}

// Cross-file consistency check: every actor kind referenced by the map must
// be defined in both the common and server gameplay configs, and the two
// config files must define the same set of kinds. The map validator does not
// know about gameplay configs, so this check lives at startup where all
// three are paired together.
fn validate_actor_kinds_consistent(
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
    map_config: &MapConfig,
) -> Result<()> {
    let common_kinds: HashSet<&str> = gameplay_config.actors.keys().map(String::as_str).collect();
    let server_kinds: HashSet<&str> = server_gameplay_config.actors.keys().map(String::as_str).collect();
    if common_kinds != server_kinds {
        let only_common: Vec<&str> = common_kinds.difference(&server_kinds).copied().collect();
        let only_server: Vec<&str> = server_kinds.difference(&common_kinds).copied().collect();
        bail!(
            "actor kinds disagree between common and server gameplay configs (only in common: {only_common:?}, only in server: {only_server:?})"
        );
    }
    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        if !common_kinds.contains(zone.kind.as_str()) {
            bail!(
                "map actor spawn zone {zone_idx} references unknown actor kind {:?} (known kinds: {:?})",
                zone.kind,
                common_kinds
            );
        }
    }
    Ok(())
}
