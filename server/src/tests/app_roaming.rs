use super::*;
use crate::actors::{
    ActorMode, SurfaceAgent, SurfaceGoal,
    navigation::{
        ActorTerritories,
        surface::{RouteFailure, fixtures},
    },
};
use common::{
    physics::{CharacterSupport, CharacterVerticalVelocity},
    protocol::{CLogin, CarrierId, ClientMessage, FaceYaw},
};

#[test]
fn roaming_rejects_detours_outside_home_and_keeps_roaming_on_reachable_surfaces() {
    for wall_rows in [1..7, 3..5] {
        let detour_leaves_home = wall_rows.start == 1;
        let mut app = build_server_app_with_loader(fixtures::config(), ServerAppOptions {
            map: None, god: true, peace: true, initial_spawn: None,
            checkpoint: None, logging: false, network: NetworkOverrides::default(),
        }, None, None, |_, hz, settings| {
            let floor = |col, row| serde_json::json!({"col":col,"row":row,"all":"basement-floor"});
            fixtures::compile(serde_json::json!({"map": {
                "grid_cols":8,"grid_rows":8,"fireworks":null,
                "levels":[{"floors":(0..8).flat_map(|row| (0..8).map(move |col| floor(col,row))).collect::<Vec<_>>(),
                    "walls":wall_rows.clone().map(|row| serde_json::json!({"c0":4,"r0":row,"c1":4,"r1":row+1,"all":"basement-floor"})).collect::<Vec<_>>()}],
                "checkpoints":[{"level":0,"cols":[0,1],"rows":[0,1],"number":0,"type":"individual"}],
                "actor_spawn_zones":[{"level":0,"cols":[2,6],"rows":[2,6],"kind":"scuttler","count":[1],"respawn_secs":null,"roam_distance":0}]
            }}), hz, settings)
        }).expect("roaming scene");
        let (client, _receiver) = super::fixtures::connect(&mut app);
        client
            .send(ClientMessage::Login(CLogin {
                name: "Observer".into(),
            }))
            .expect("login");
        app.update();
        let (id, entity, zone) = app
            .world()
            .resource::<ActorMap>()
            .iter()
            .map(|(id, info)| (*id, info.entity, info.spawn_zone_index))
            .next()
            .expect("actor");
        let home = app.world().resource::<ActorTerritories>().get(zone).clone();
        let start = Position {
            x: -3.0,
            y: 0.0,
            z: 0.0,
        };
        app.world_mut().entity_mut(entity).insert((
            start,
            FaceYaw(0.0),
            CharacterVerticalVelocity(0.0),
            CharacterSupport::Ground,
            SurfaceAgent::default(),
        ));
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&id)
            .expect("actor")
            .mode = ActorMode::Roam;
        app.world_mut().get_mut::<SurfaceAgent>(entity).expect("agent").goal = Some(SurfaceGoal {
            carrier: CarrierId::WORLD,
            position: Position { x: 3.0, y: 0.0, z: 0.0 },
        });
        app.update();
        let agent = app.world().get::<SurfaceAgent>(entity).expect("agent");
        assert_eq!(
            agent.failure,
            detour_leaves_home.then_some(RouteFailure::OutsideTerritory)
        );
        let mut roamed = false;
        let mut crossed = false;
        for _ in 0..600 {
            app.update();
            let position = *app.world().get::<Position>(entity).expect("position");
            assert!(home.contains_position(position.into()), "left home: {position:?}");
            assert_eq!(
                app.world().resource::<ActorMap>().get(&id).expect("actor").mode,
                ActorMode::Roam
            );
            roamed |= position.horizontal_distance_sq(&start) > 0.25;
            crossed |= position.x > 1.0;
        }
        assert!(roamed, "rejected detours must allow another roaming goal");
        assert_eq!(crossed, !detour_leaves_home, "a detour inside home remains usable");
    }
}
