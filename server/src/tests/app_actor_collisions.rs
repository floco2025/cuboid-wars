use super::*;
use crate::{
    actors::{ActorCharacter, ActorMode, SurfaceAgent, SurfaceGoal, navigation::surface::fixtures},
    config::ServerGameplayConfig,
};
use common::{
    config::CharacterPhysicsConfig,
    physics::{CharacterSupport, CharacterVerticalVelocity, character_positions_intersect},
    protocol::{ActorAnchor, CLogin, CarrierId, ClientMessage, FaceYaw},
};
use serde_json::{Value, json};

const STARTS: [Position; 2] = [
    Position {
        x: -4.0,
        y: 0.0,
        z: -1.5,
    },
    Position {
        x: 4.0,
        y: 0.0,
        z: -1.5,
    },
];

struct Scene {
    app: App,
    bodies: [Entity; 2],
    physics: [CharacterPhysicsConfig; 2],
}

impl Scene {
    // An 8×8 floor with `walls`, one actor of each kind standing at `STARTS`
    // facing the other, the ground ones walking to `goals`.
    fn new(mut config: ServerGameplayConfig, kinds: [&str; 2], walls: Value, goals: [Position; 2]) -> Self {
        for actor in config.actors.values_mut() {
            actor.vision_range = 0.0;
        }
        let options = ServerAppOptions {
            map: None,
            god: true,
            peace: false,
            initial_spawn: None,
            checkpoint: None,
            logging: false,
            network: NetworkOverrides::default(),
        };
        let mut app = build_server_app_with_loader(config, options, None, None, |_, hz, settings| {
            let floors: Vec<_> = (0..8)
                .flat_map(|row| (0..8).map(move |col| json!({"col": col, "row": row, "all": "basement-floor"})))
                .collect();
            let zone = |cols: [u8; 2], kind: &str| {
                json!({"level": 0, "cols": cols, "rows": [2, 6], "kind": kind, "count": [1],
                    "respawn_secs": null, "roam_distance": 10})
            };
            fixtures::compile(
                json!({"map": {
                    "grid_cols": 8, "grid_rows": 8, "fireworks": null,
                    "levels": [{"floors": floors, "walls": walls}],
                    "checkpoints": [{"level": 0, "cols": [0, 1], "rows": [0, 1], "number": 0, "type": "individual"}],
                    "actor_spawn_zones": [zone([2, 4], kinds[0]), zone([4, 6], kinds[1])]
                }}),
                hz,
                settings,
            )
        })
        .expect("actor collision scene");
        let (client, _receiver) = super::fixtures::connect(&mut app);
        client
            .send(ClientMessage::Login(CLogin {
                name: "Observer".into(),
            }))
            .expect("login");
        app.update();
        let mut spawned: Vec<_> = app
            .world()
            .resource::<ActorMap>()
            .iter()
            .map(|(id, info)| (info.spawn_zone_index, *id, info.entity))
            .collect();
        spawned.sort_by_key(|(zone, ..)| *zone);
        assert_eq!(spawned.len(), 2);
        for (index, &(_, id, entity)) in spawned.iter().enumerate() {
            let facing = if index == 0 { 1.0 } else { -1.0 } * std::f32::consts::FRAC_PI_2;
            app.world_mut().entity_mut(entity).insert((
                STARTS[index],
                FaceYaw(facing),
                CharacterVerticalVelocity(0.0),
                CharacterSupport::Ground,
            ));
            if let Some(mut agent) = app.world_mut().get_mut::<SurfaceAgent>(entity) {
                *agent = SurfaceAgent::default();
                agent.goal = Some(SurfaceGoal {
                    carrier: CarrierId::WORLD,
                    position: goals[index],
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
                    pos: STARTS[index],
                });
            }
            if let Some(flight) = &mut info.flight {
                flight.route.clear();
            }
        }
        let bodies = [spawned[0].2, spawned[1].2];
        let physics = bodies.map(|entity| app.world().get::<ActorCharacter>(entity).expect("body").0.physics());
        Self { app, bodies, physics }
    }

    fn positions(&self) -> [Position; 2] {
        self.bodies
            .map(|entity| *self.app.world().get::<Position>(entity).expect("actor position"))
    }

    fn overlapping(&self) -> bool {
        let [first, second] = self.positions();
        character_positions_intersect(&first, self.physics[0], &second, self.physics[1])
    }
}

fn beyond(start: Position) -> Position {
    Position {
        x: start.x * 2.0,
        ..start
    }
}

#[test]
fn ground_actors_cannot_walk_through_other_actors_with_combat_enabled() {
    for other_kind in ["scuttler", "bruiser", "turret", "zapper"] {
        let goals = [beyond(STARTS[1]), beyond(STARTS[0])];
        let mut scene = Scene::new(fixtures::config(), ["scuttler", other_kind], json!([]), goals);
        let mut approached = false;
        for _ in 0..240 {
            scene.app.update();
            assert!(!scene.app.world().resource::<ActorMap>().peaceful);
            let [first, second] = scene.positions();
            approached |= first.x > STARTS[0].x + 0.5;
            assert!(
                !scene.overlapping(),
                "scuttler passed into {other_kind}: {first:?}, {second:?}"
            );
        }
        assert!(approached, "the actor must walk up to the other body");
        assert!(
            scene.positions()[0].x > STARTS[1].x + 0.5,
            "scuttler must find room to pass {other_kind}"
        );
    }
}

#[test]
fn a_roamer_gives_up_a_goal_that_another_body_covers() {
    let goal = Position {
        x: STARTS[1].x + 0.1,
        z: STARTS[1].z + 0.1,
        ..STARTS[1]
    };
    let mut scene = Scene::new(fixtures::config(), ["scuttler", "turret"], json!([]), [goal, goal]);
    let walker = scene.bodies[0];
    let mut gave_up = false;
    for _ in 0..300 {
        scene.app.update();
        assert!(!scene.overlapping(), "{:?}", scene.positions());
        let agent = scene.app.world().get::<SurfaceAgent>(walker).expect("surface agent");
        gave_up |= agent.goal.is_none_or(|current| current.position != goal);
    }
    assert!(gave_up, "the goal under the turret must fail and be replaced");
}

#[test]
fn movers_too_wide_to_pass_in_a_corridor_end_their_standoff() {
    let mut config = fixtures::config();
    let bruiser = &mut config
        .actors
        .get_mut("bruiser")
        .expect("fixture actor")
        .character
        .character;
    bruiser.movement_collider.diameter = 1.6;
    bruiser.hitbox.width = 1.6;
    bruiser.hitbox.depth = 1.6;
    // Row 3 becomes a corridor 2.7 m clear: either bruiser fits, both do not.
    let walls: Vec<_> = (0..8)
        .flat_map(|col| {
            [3, 4].map(|row| json!({"c0": col, "r0": row, "c1": col + 1, "r1": row, "all": "basement-floor"}))
        })
        .collect();
    let goals = [beyond(STARTS[1]), beyond(STARTS[0])];
    let mut scene = Scene::new(config, ["bruiser", "bruiser"], json!(walls), goals);
    let mut met = false;
    for _ in 0..450 {
        scene.app.update();
        let [first, second] = scene.positions();
        met |= second.x - first.x < 2.0;
        if first.x > STARTS[1].x && second.x < STARTS[0].x {
            break;
        }
    }
    let [first, second] = scene.positions();
    assert!(met, "the bruisers must meet in the corridor");
    assert!(
        first.x > STARTS[1].x && second.x < STARTS[0].x,
        "both must get past: {first:?}, {second:?}"
    );
    assert!(!scene.overlapping());
}
