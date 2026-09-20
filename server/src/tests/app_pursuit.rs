use super::*;
use crate::actors::{ActorMode, SurfaceAgent, navigation::surface::fixtures};
use common::{
    physics::CharacterSupport,
    protocol::{
        CLogin, CMove, ClientMessage, FaceYaw, PlayerGeneration, PlayerId, PlayerMoveIntent, PlayerMovementState,
    },
};
use crossbeam_channel::Sender;

struct PursuitScene {
    app: App,
    client: Sender<ClientMessage>,
    _receiver: crossbeam_channel::Receiver<common::protocol::ServerMessage>,
    generation: PlayerGeneration,
    actor: Entity,
    owner: Entity,
    seq: u32,
}

impl PursuitScene {
    fn new(low_gravity: bool, gap_and_roof: bool) -> Self {
        let mut config = fixtures::config();
        config.power_ups.single_shot = crate::config::PowerUpMode::Always {};
        config.power_ups.low_gravity = if low_gravity {
            crate::config::PowerUpMode::Always {}
        } else {
            crate::config::PowerUpMode::Pickup { duration_secs: None }
        };
        config.settings.movement.player.jump_speed = 12.0;
        config.settings.movement.low_gravity = 5.0;
        let mut app = build_server_app_with_loader(config, ServerAppOptions {
            map: None,  god: true, peace: false, initial_spawn: None,
            checkpoint: None, logging: false,
            network: NetworkOverrides { server_hz: Some(30), update_hz: Some(30), snapshot_hz: Some(4) },
        }, None, None, |_, hz, settings| {
            let floor = |col, row| serde_json::json!({"col":col,"row":row,"all":"basement-floor"});
            fixtures::compile(serde_json::json!({"map": {
                "grid_cols":48,"grid_rows":6,"fireworks":null,
                "levels":[{"floors":(0..6).flat_map(|row| (0..48).filter(move |col| !gap_and_roof || !(24..28).contains(col)).map(move |col| floor(col,row))).collect::<Vec<_>>()}, {},
                    {"inaccessible_floors":if gap_and_roof { vec![floor(24,2),floor(24,3),floor(25,2),floor(25,3)] } else { vec![] }}],
                "checkpoints":[{"level":0,"cols":[44,45],"rows":[2,3],"number":0,"type":"individual"}],
                "actor_spawn_zones":[{"level":0,"cols":[16,17],"rows":[2,3],"kind":"scuttler","count":[1],"respawn_secs":null}]
            }}), hz, settings)
        }).expect("pursuit scene");
        let (client, receiver) = super::fixtures::connect(&mut app);
        client
            .send(ClientMessage::Login(CLogin {
                name: "Observer".into(),
            }))
            .expect("login");
        app.update();
        let player = app.world().resource::<PlayerMap>().get(&PlayerId(1)).expect("owner");
        let generation = player.session.generation;
        let owner = player.entity().expect("owner body");
        let actor = app
            .world()
            .resource::<ActorMap>()
            .values()
            .next()
            .expect("actor")
            .entity;
        app.world_mut().entity_mut(actor).insert((
            Position {
                x: -20.0,
                y: 0.0,
                z: 0.0,
            },
            FaceYaw(std::f32::consts::FRAC_PI_2),
            SurfaceAgent::default(),
        ));
        Self {
            app,
            client,
            _receiver: receiver,
            generation,
            actor,
            owner,
            seq: 0,
        }
    }

    fn report(&mut self, position: Position, support: CharacterSupport, vertical_velocity: f32) {
        self.seq += 1;
        let mut movement = PlayerMovementState::new(position, PlayerMoveIntent::Idle, vertical_velocity, 0.0);
        movement.support = support;
        self.client
            .send(ClientMessage::Move(CMove {
                generation: self.generation,
                seq: self.seq,
                portal_crossing: 0,
                movement,
            }))
            .expect("owner report");
        self.app.update();
        assert_eq!(
            *self.app.world().get::<Position>(self.owner).expect("owner body"),
            position
        );
        assert_eq!(
            self.app
                .world()
                .resource::<PlayerMap>()
                .get(&PlayerId(1))
                .expect("owner")
                .life
                .movement
                .vertical_velocity,
            vertical_velocity
        );
    }

    fn position(&self) -> Position {
        *self.app.world().get::<Position>(self.actor).expect("actor body")
    }

    fn mode(&self) -> ActorMode {
        self.app
            .world()
            .resource::<ActorMap>()
            .values()
            .find(|info| info.entity == self.actor)
            .expect("actor")
            .mode
    }

    fn engaged(&self) {
        assert!(
            matches!(
                self.mode(),
                ActorMode::Engage {
                    target: PlayerId(1),
                    ..
                }
            ),
            "abandoned pursuit at report {}: {:?}",
            self.seq,
            self.mode()
        );
    }
}

#[test]
fn regular_and_low_gravity_jumps_keep_ground_actors_pursuing() {
    for low_gravity in [false, true] {
        let mut scene = PursuitScene::new(low_gravity, false);
        let gravity = if low_gravity { 5.0 } else { 25.0 };
        let duration = 24.0 / gravity;
        let start = scene.position();
        // Discover the player in midair, then follow two complete jumps.
        for _ in 0..2 {
            for tick in 1..=(duration * 30.0_f32).ceil() as u32 {
                let t = (tick as f32 / 30.0).min(duration);
                let y = (12.0 * t - 0.5 * gravity * t * t).max(0.0);
                let support = if t == duration {
                    CharacterSupport::Ground
                } else {
                    CharacterSupport::Airborne
                };
                let target = Position {
                    x: scene.position().x + 12.0,
                    y,
                    z: 0.0,
                };
                scene.report(target, support, if t == duration { 0.0 } else { 12.0 - gravity * t });
                if tick > 4 {
                    scene.engaged();
                }
            }
        }
        assert!(
            scene.position().x > start.x + duration * 4.5,
            "pursuit paused during a jump: {:?} -> {:?}",
            start,
            scene.position()
        );
    }
}

#[test]
fn airborne_players_over_gaps_keep_the_last_goal_until_they_land() {
    let mut scene = PursuitScene::new(true, true);
    let floor = Position {
        x: -8.0,
        y: 0.0,
        z: 0.0,
    };
    for _ in 0..6 {
        scene.report(floor, CharacterSupport::Ground, 0.0);
    }
    scene.engaged();
    let previous = scene
        .app
        .world()
        .get::<SurfaceAgent>(scene.actor)
        .expect("pursuer")
        .goal;
    assert!(previous.is_some());
    for (point, ticks) in [
        (
            Position {
                x: 9.0,
                y: 15.0,
                z: 0.0,
            },
            180,
        ),
        (Position { x: 3.0, y: 8.0, z: 0.0 }, 30),
    ] {
        for _ in 0..ticks {
            scene.report(point, CharacterSupport::Airborne, -0.5);
            scene.engaged();
            assert_eq!(
                scene
                    .app
                    .world()
                    .get::<SurfaceAgent>(scene.actor)
                    .expect("pursuer")
                    .goal,
                previous
            );
        }
    }
    for _ in 0..6 {
        scene.report(Position { x: 3.0, y: 3.0, z: 0.0 }, CharacterSupport::Ground, 0.0);
    }
    assert!(
        matches!(scene.mode(), ActorMode::Evade { .. }),
        "landing on an unreachable roof should allow evasion: {:?}",
        scene.mode()
    );
    let reachable = Position {
        x: scene.position().x - 4.0,
        y: 0.0,
        z: 0.0,
    };
    for _ in 0..6 {
        scene.report(reachable, CharacterSupport::Ground, 0.0);
    }
    scene.engaged();
}
