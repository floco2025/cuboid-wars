use std::time::Duration;

use bevy::{ecs::world::CommandQueue, prelude::*};
use crossbeam_channel::{Receiver, unbounded};

use crate::{
    actors::{ActorMap, ActorSpawner, PendingActorSpawns, actors_pending_spawn_system, actors_respawn_system},
    combat::{DeathSource, PendingExplosions, kill_player},
    config::{ActorRespawnConfig, ActorRespawnScope, PlayerRespawnMode, RespawnConfig, ServerGameplayConfig, fixtures},
    map::{ActorSpawnZone, MapConfig},
    missiles::MissileMap,
    players::{PlayerInfo, PlayerMap, PlayerQuestState, players_group_respawn_system, players_respawn_system},
    portals::PortalAssignments,
    schedule::{ServerSet, configure_server_schedule},
    test_geometry::{floored_level, geometry},
};
use common::{
    config::DeathTrigger,
    map::{Carriers, MapGeometry},
    physics::CollisionWorld,
    protocol::{
        CarrierId, Checkpoint, CheckpointKind, Health, MapLayout, PlayerId, PlayerMarker, PortalMode, Position,
        QuestId, ServerMessage, ServerTick, SwitchState, server_tick_advance_system,
    },
};

pub(crate) fn respawn_app(mode: PlayerRespawnMode, scope: ActorRespawnScope) -> App {
    let mut config = fixtures::server_config();
    config.player.respawn_secs = 2.0;
    let settings = config.settings.clone();
    let geometry = geometry(6, 1);
    let mut map = MapConfig::for_grid(vec![floored_level(6, 1)], geometry);
    map.actor_spawn_zones = (3..6)
        .map(|col| ActorSpawnZone {
            initially_on: true,

            carrier: CarrierId::WORLD,
            level: 0,
            levels: 1,
            roam_distance: 0.0,
            cols: [col, col + 1],
            rows: [0, 1],
            kind: "turret".into(),
            count: vec![1],
            respawn_secs: None,
            beam_in_secs: 3.0,
            switch: None,
            until_checkpoint: None,
            on_checkpoint: Default::default(),
        })
        .collect();
    let layout = MapLayout {
        checkpoints: vec![start_checkpoint(&geometry)],
        ..default()
    };
    let mut app = App::new();
    app.insert_resource(Time::<()>::default())
        .insert_resource(config.gameplay_config())
        .insert_resource(config)
        .insert_resource(settings)
        .insert_resource(map)
        .insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(layout)
        .insert_resource(PlayerMap::new(
            RespawnConfig {
                players: mode,
                actors: ActorRespawnConfig {
                    on_player_death: DeathTrigger::Any,
                    scope,
                },
            },
            Default::default(),
        ))
        .init_resource::<Carriers>()
        .init_resource::<ActorMap>()
        .init_resource::<ActorSpawner>()
        .init_resource::<PendingActorSpawns>()
        .init_resource::<PendingExplosions>()
        .init_resource::<MissileMap>()
        .init_resource::<ServerTick>()
        .init_resource::<SwitchState>()
        .insert_resource(PortalAssignments::new(PortalMode::Both));
    configure_server_schedule(&mut app);
    app.add_systems(
        Update,
        (
            server_tick_advance_system.in_set(ServerSet::Prepare),
            actors_pending_spawn_system
                .in_set(ServerSet::Prepare)
                .after(server_tick_advance_system),
            (players_group_respawn_system, players_respawn_system)
                .chain()
                .in_set(ServerSet::Lifecycle),
            actors_respawn_system
                .in_set(ServerSet::Lifecycle)
                .after(players_respawn_system),
        ),
    );
    advance(&mut app, 0.0);
    app
}

// The start, over the fixture's first three cells.
pub(crate) fn start_checkpoint(geometry: &MapGeometry) -> Checkpoint {
    Checkpoint {
        kind: CheckpointKind::Individual,
        number: 0,
        carrier: CarrierId::WORLD,
        level: 0,
        cols: [0, 3],
        rows: [0, 1],
        min_x: geometry.cell_to_world_x(0),
        max_x: geometry.cell_to_world_x(3),
        min_z: geometry.cell_to_world_z(0),
        max_z: geometry.cell_to_world_z(1),
        y: 0.0,
    }
}

pub(crate) fn advance(app: &mut App, secs: f32) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(secs));
    app.update();
}

pub(crate) fn add_player(app: &mut App, id: PlayerId) -> (Entity, Receiver<ServerMessage>) {
    let pos = Position {
        x: -8.0 + id.0 as f32,
        y: 0.0,
        z: 0.0,
    };
    let entity = app.world_mut().spawn((PlayerMarker, id, pos, Health(30.0))).id();
    let (tx, rx) = unbounded();
    let mut info = PlayerInfo::new(entity, tx);
    info.connection.logged_in = true;
    info.connection.name = format!("Player {}", id.0);
    info.session.score = 42;
    info.session
        .quest_states
        .insert(QuestId("progress".into()), PlayerQuestState::Individual { progress: 3 });
    info.add_missiles(2, 3);
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);
    (entity, rx)
}

pub(crate) fn kill(app: &mut App, id: PlayerId) {
    let config = app.world().resource::<ServerGameplayConfig>().clone();
    app.world_mut().resource_scope(|world, mut players: Mut<PlayerMap>| {
        let entity = players
            .get(&id)
            .and_then(PlayerInfo::entity)
            .expect("live player missing");
        let pos = *world.get::<Position>(entity).expect("player position missing");
        world.resource_scope(|world, mut explosions: Mut<PendingExplosions>| {
            let mut queue = CommandQueue::default();
            kill_player(
                &mut Commands::new(&mut queue, world),
                &mut players,
                id,
                entity,
                pos,
                config.player.respawn_secs,
                DeathSource::Beam { kind: "turret".into() },
                &config,
                &mut explosions,
            );
            queue.apply(world);
        });
    });
}

pub(crate) fn disconnect(app: &mut App, id: PlayerId) {
    let delay = app.world().resource::<ServerGameplayConfig>().player.respawn_secs;
    let info = app
        .world_mut()
        .resource_mut::<PlayerMap>()
        .disconnect(&id, delay)
        .expect("departing player missing");
    if let Some(entity) = info.entity() {
        app.world_mut().despawn(entity);
    }
}
