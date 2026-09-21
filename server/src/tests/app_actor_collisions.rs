use super::*;
use crate::actors::{ActorCharacter, ActorMode, SurfaceAgent, SurfaceGoal, navigation::surface::fixtures};
use common::{
    physics::{CharacterSupport, CharacterVerticalVelocity, character_positions_intersect},
    protocol::{ActorAnchor, CLogin, CarrierId, ClientMessage, FaceYaw},
};

#[test]
fn ground_actors_cannot_walk_through_other_actors_with_combat_enabled() {
    for other_kind in ["scuttler", "bruiser", "turret", "zapper"] {
        let mut config = fixtures::config();
        for actor in config.actors.values_mut() {
            actor.vision_range = 0.0;
        }
        let mut app = build_server_app_with_loader(config, ServerAppOptions {
            map: None, god: true, peace: false, initial_spawn: None,
            checkpoint: None, logging: false, network: NetworkOverrides::default(),
        }, None, None, |_, hz, settings| {
            let floor = |col, row| serde_json::json!({"col":col,"row":row,"all":"basement-floor"});
            fixtures::compile(serde_json::json!({"map": {
                "grid_cols":8,"grid_rows":8,"fireworks":null,
                "levels":[{"floors":(0..8).flat_map(|row| (0..8).map(move |col| floor(col,row))).collect::<Vec<_>>()}],
                "checkpoints":[{"level":0,"cols":[0,1],"rows":[0,1],"number":0,"type":"individual"}],
                "actor_spawn_zones":[
                    {"level":0,"cols":[2,4],"rows":[2,6],"kind":"scuttler","count":[1],"respawn_secs":null,"roam_distance":10},
                    {"level":0,"cols":[4,6],"rows":[2,6],"kind":other_kind,"count":[1],"respawn_secs":null,"roam_distance":10}
                ]
            }}), hz, settings)
        }).expect("actor collision scene");
        let (client, _receiver) = super::fixtures::connect(&mut app);
        client
            .send(ClientMessage::Login(CLogin {
                name: "Observer".into(),
            }))
            .expect("login");
        app.update();
        let mut bodies: Vec<_> = app
            .world()
            .resource::<ActorMap>()
            .iter()
            .map(|(id, info)| (info.spawn_zone_index, *id, info.entity))
            .collect();
        bodies.sort_by_key(|(zone, ..)| *zone);
        assert_eq!(bodies.len(), 2);
        let positions = [
            Position {
                x: -2.0,
                y: 0.0,
                z: 0.0,
            },
            Position { x: 2.0, y: 0.0, z: 0.0 },
        ];
        for (index, &(_, id, entity)) in bodies.iter().enumerate() {
            app.world_mut().entity_mut(entity).insert((
                positions[index],
                FaceYaw(if index == 0 {
                    std::f32::consts::FRAC_PI_2
                } else {
                    -std::f32::consts::FRAC_PI_2
                }),
                CharacterVerticalVelocity(0.0),
                CharacterSupport::Ground,
            ));
            if let Some(mut agent) = app.world_mut().get_mut::<SurfaceAgent>(entity) {
                *agent = SurfaceAgent::default();
                agent.goal = Some(SurfaceGoal {
                    carrier: CarrierId::WORLD,
                    position: Position {
                        x: positions[1 - index].x * 2.0,
                        ..positions[1 - index]
                    },
                });
                agent.decision_secs = f32::MAX;
            }
            let mut actors = app.world_mut().resource_mut::<ActorMap>();
            let info = actors.get_mut(&id).expect("actor");
            info.mode = ActorMode::Roam;
            info.decision_timer = f32::MAX;
            if info.anchor.is_some() {
                info.anchor = Some(ActorAnchor {
                    carrier: CarrierId::WORLD,
                    pos: positions[index],
                });
            }
            if let Some(flight) = &mut info.flight {
                flight.route.clear();
            }
        }
        let physics: Vec<_> = bodies
            .iter()
            .map(|&(_, _, entity)| app.world().get::<ActorCharacter>(entity).expect("body").0.physics())
            .collect();
        let mut approached = false;
        for _ in 0..180 {
            app.update();
            assert!(!app.world().resource::<ActorMap>().peaceful);
            let first = app.world().get::<Position>(bodies[0].2).expect("first actor");
            let second = app.world().get::<Position>(bodies[1].2).expect("second actor");
            approached |= first.x > positions[0].x + 0.5;
            assert!(
                !character_positions_intersect(first, physics[0], second, physics[1]),
                "scuttler passed into {other_kind}: {first:?}, {second:?}"
            );
        }
        assert!(approached, "the actor must walk up to the other body");
        assert!(
            app.world().get::<Position>(bodies[0].2).expect("first actor").x > 2.5,
            "scuttler must find room to pass {other_kind}"
        );
    }
}
