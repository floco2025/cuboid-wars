pub(super) use super::super::{
    beam::{BeamContext, find_beam_target, retarget_beam, tick_beam_state},
    perception::{PlayerState, update_awareness},
    stationary::{decide_stationary_actor, stationary_actors_behavior_system},
    surface::surface_actors_behavior_system,
};
pub(super) use crate::{
    actors::{
        ActorCharacter, ActorInfo, ActorMap, ActorMode, BeamState, SurfaceAgent,
        navigation::{ActorTerritories, surface::SurfaceNavigation},
        resources::AwarePlayer,
        test_kinds::{self, BEAM, CONTACT, CONTACT_BEAM, IMMOVABLE, KINDS},
    },
    combat::{PendingExplosions, actors_beam_damage_system},
    config::ServerGameplayConfig,
    map::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    players::{Invincibility, PlayerInfo, PlayerMap},
    test_geometry::{CELL, LEVEL_HEIGHT, WALL_HEIGHT, geometry},
};
pub(super) use bevy::prelude::*;
pub(super) use common::{
    config::GameplayConfig,
    constants::TICK_SECS,
    map::{Carriers, MapGeometry},
    physics::{CharacterSupport, CollisionWorld},
    protocol::{
        ActorId, ActorMarker, Barrier, CarrierId, FieldId, Health, MapItems, MapLayout, PlayerId, PlayerMarker,
        Position, ServerMessage, ServerTick, SwitchState, Wall,
    },
};
pub(super) use crossbeam_channel::{Receiver, unbounded};
pub(super) use std::time::Duration;

// Combat-only fixture: the normal controllers and damage run without movement.
pub(crate) struct Fixture {
    pub(crate) territories: ActorTerritories,
    pub(crate) collision_world: CollisionWorld,
    pub(crate) gameplay: GameplayConfig,
    pub(crate) server: ServerGameplayConfig,
    pub(crate) geometry: MapGeometry,
    pub(crate) carriers: Carriers,
}

impl Fixture {
    pub(crate) fn new(kind: &str) -> Self {
        let geometry = geometry(12, 5);
        let map = MapConfig {
            actor_spawn_zones: vec![ActorSpawnZone {
                roam_distance: CELL * 2.0,
                ..test_kinds::spawn_zone(kind, [1, 2], [2, 3])
            }],
            ..MapConfig::for_grid(
                vec![LevelGrid {
                    cells: CellGrid::new(12, 5),
                    edges: EdgeGrid::new(12, 5),
                }],
                geometry,
            )
        };
        let server = test_kinds::server_config();
        Self {
            territories: ActorTerritories::new(&map, &server),
            collision_world: CollisionWorld::from_map_layout(&MapLayout::default()),
            gameplay: server.gameplay_config(),
            server,
            geometry,
            carriers: Carriers::default(),
        }
    }
    pub(crate) fn pos(&self, col: i32, row: i32) -> Position {
        Position {
            x: self.geometry.cell_to_world_x(col) + 2.0,
            y: 0.0,
            z: self.geometry.cell_to_world_z(row) + 2.0,
        }
    }
    pub(super) fn context(&self, kind: &str, pos: Position) -> BeamContext<'_> {
        BeamContext {
            tick: 0,
            world_pos: pos,
            kind_config: self.server.expect_actor(kind),
            player_physics: self.gameplay.player.physics(),
            collision_world: &self.collision_world,
            open_fields: &[],
        }
    }
}

pub(crate) fn info(kind: &str) -> ActorInfo {
    ActorInfo::new(Entity::from_bits(1), 0, kind.into(), CarrierId::WORLD)
}

pub(crate) fn aware(id: u32, pos: Position, support: CharacterSupport, visible: bool) -> AwarePlayer {
    AwarePlayer {
        stance: Default::default(),
        id: PlayerId(id),
        pos,
        carrier: CarrierId::WORLD,
        carrier_pos: pos,
        support,
        visible,
        forget_remaining_secs: 10.0,
    }
}

pub(crate) fn actor_app(kind: &str, health: f32) -> (App, Entity, Receiver<ServerMessage>) {
    let fixture = Fixture::new(kind);
    let origin = fixture.pos(1, 2);
    let target = fixture.pos(3, 2);
    let character = ActorCharacter(fixture.gameplay.expect_actor(kind).clone());
    let mut app = App::new();
    app.insert_resource(fixture.territories)
        .insert_resource(fixture.carriers)
        .insert_resource(fixture.collision_world)
        .insert_resource(fixture.gameplay)
        .insert_resource(fixture.server)
        .init_resource::<SurfaceNavigation>()
        .init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<MapItems>()
        .init_resource::<SwitchState>()
        .init_resource::<ServerTick>()
        .init_resource::<Time>()
        .init_resource::<PendingExplosions>()
        .insert_resource(Invincibility(false))
        .add_systems(
            Update,
            (
                stationary_actors_behavior_system,
                surface_actors_behavior_system,
                actors_beam_damage_system,
            )
                .chain(),
        );
    let actor = app
        .world_mut()
        .spawn((ActorId(1), ActorMarker, origin, character, CharacterSupport::Ground))
        .id();
    if !test_kinds::kind(kind).character.immovable {
        app.world_mut().entity_mut(actor).insert(SurfaceAgent::default());
    }
    app.world_mut()
        .resource_mut::<ActorMap>()
        .insert(ActorId(1), ActorInfo::new(actor, 0, kind.into(), CarrierId::WORLD));
    let player = app
        .world_mut()
        .spawn((PlayerMarker, PlayerId(7), target, Health(health)))
        .id();
    let (sender, receiver) = unbounded();
    let mut player_info = PlayerInfo::new(player, sender);
    player_info.connection.logged_in = true;
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .insert(PlayerId(7), player_info);
    (app, player, receiver)
}

// The fixture's one actor, `ActorId(1)`.
pub(crate) fn actor(app: &App) -> &ActorInfo {
    app.world()
        .resource::<ActorMap>()
        .get(&ActorId(1))
        .expect("actor missing")
}

pub(crate) fn health(app: &App, entity: Entity) -> f32 {
    app.world().get::<Health>(entity).expect("body health missing").0
}

// Logs in player `id` standing at `pos`.
pub(crate) fn add_player(app: &mut App, id: u32, pos: Position, health: f32) -> Entity {
    let entity = app
        .world_mut()
        .spawn((PlayerMarker, PlayerId(id), pos, Health(health)))
        .id();
    let (sender, _receiver) = unbounded();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(id), info);
    entity
}

pub(crate) fn step_tick(app: &mut App) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(1.0 / 30.0));
    app.world_mut().resource_mut::<ServerTick>().0 += 1;
    app.update();
}
