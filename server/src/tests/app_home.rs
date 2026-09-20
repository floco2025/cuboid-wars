use super::*;
use crate::actors::{ActorMode, SurfaceAgent, SurfaceGoal, navigation::surface::fixtures};
use common::{
    physics::{CharacterSupport, CharacterVerticalVelocity},
    protocol::{CAdmin, CLogin, CarrierId, ClientMessage, FaceYaw, PlayerId},
};

#[test]
fn peace_returns_a_rooftop_actor_to_its_spawn_zone_before_roaming() {
    let mut config = fixtures::config();
    config.settings.geometry.level_height = 3.0;
    let physics = config.expect_actor("scuttler").character.physics();
    let mut app = build_server_app_with_loader(
        config,
        ServerAppOptions {
            map: None,
            god: true,
            peace: false,
            initial_spawn: None,
            checkpoint: None,
            logging: false,
            network: NetworkOverrides {
                server_hz: Some(30),
                update_hz: Some(30),
                snapshot_hz: Some(4),
            },
        },
        None,
        None,
        fixtures::generate,
    )
    .expect("multistorey return scene");
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
    let roof = Position { x: 4.5, y: 6.0, z: 0.0 };
    let home = app
        .world()
        .resource::<crate::actors::navigation::ActorTerritories>()
        .get(zone)
        .clone();
    assert!(
        home.contains_position(roof.into()),
        "roof must be within the roaming extension"
    );
    app.world_mut().entity_mut(entity).insert((
        roof,
        CharacterVerticalVelocity(0.0),
        CharacterSupport::Ground,
        FaceYaw(0.0),
        SurfaceAgent::default(),
    ));
    app.world_mut().get_mut::<SurfaceAgent>(entity).expect("agent").goal = Some(SurfaceGoal {
        carrier: CarrierId::WORLD,
        position: roof,
    });
    app.world_mut()
        .resource_mut::<ActorMap>()
        .get_mut(&id)
        .expect("pursuer")
        .mode = ActorMode::Engage {
        target: PlayerId(1),
        target_pos: roof,
    };
    client
        .send(ClientMessage::Admin(CAdmin {
            command: "/peace".into(),
        }))
        .expect("peace command");
    let mut returned = false;
    for tick in 0..1800 {
        app.update();
        let position = *app.world().get::<Position>(entity).expect("returning actor");
        assert!(
            !app.world()
                .resource::<CollisionWorld>()
                .character_penetrates_solid(&position, physics, &[])
        );
        if home
            .volume
            .contains(Vec3::from(position) + Vec3::Y * home.center_height, 0.0)
        {
            returned = true;
            break;
        }
        if tick >= 4 {
            assert_eq!(
                app.world().resource::<ActorMap>().get(&id).expect("actor").mode,
                ActorMode::ReturnHome,
                "roaming before returning: {position:?}"
            );
        }
    }
    let agent = app.world().get::<SurfaceAgent>(entity).expect("agent");
    assert!(
        returned,
        "return failed: {:?} {:?} {:?}",
        agent.goal, agent.failure, agent.executor
    );
    assert!(app.world().resource::<ActorMap>().peaceful);
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<ActorMap>().get(&id).expect("home actor").mode,
        ActorMode::Roam
    );
}

#[test]
fn hotel_scuttlers_return_from_the_roof_after_peace() {
    use crate::actors::{
        ActorCharacter,
        navigation::{ActorTerritories, surface::SurfaceNavigation},
    };
    let mut app = build_server_app(
        ServerAppOptions {
            map: Some("hotel".into()),
            god: true,
            peace: false,
            initial_spawn: None,
            checkpoint: None,
            logging: false,
            network: NetworkOverrides::default(),
        },
        None,
        None,
    )
    .expect("Hotel compatibility scene");
    let (client, _receiver) = super::fixtures::connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin {
            name: "Observer".into(),
        }))
        .expect("login");
    for _ in 0..100 {
        app.update();
    }
    let roof_y = app.world().resource::<MapSettings>().geometry.level_y(4);
    let mut zones = std::collections::BTreeMap::new();
    for (id, info) in app.world().resource::<ActorMap>().iter() {
        if info.spawn_kind == "scuttler" {
            zones.entry(info.spawn_zone_index).or_insert((*id, info.entity));
        }
    }
    let mut tested = std::collections::BTreeSet::new();
    for (zone, (id, entity)) in zones {
        let home = app.world().resource::<ActorTerritories>().get(zone).clone();
        if !tested.insert(home.distance.to_bits()) {
            continue;
        }
        let original: Position = home
            .volume
            .min
            .midpoint(home.volume.max)
            .with_y(home.volume.min.y)
            .into();
        let physics = app.world().get::<ActorCharacter>(entity).expect("body").0.physics();
        let roof = {
            let navigation = app.world().resource::<SurfaceNavigation>();
            let (mesh, _) = navigation.mesh(CarrierId::WORLD, physics).expect("Hotel surface");
            let mut candidates: Vec<_> = (0..mesh.polygon_count())
                .filter_map(|index| mesh.candidate(index))
                .filter(|point| (point.y - roof_y).abs() < 0.3)
                .collect();
            let near = if home.distance > 10.0 {
                Position {
                    x: -25.6,
                    y: roof_y,
                    z: 15.3,
                }
            } else {
                original
            };
            candidates.sort_by(|a, b| {
                a.horizontal_distance_sq(&near)
                    .total_cmp(&b.horizontal_distance_sq(&near))
            });
            candidates
                .into_iter()
                .find(|point| {
                    mesh.locate(original, 1.0)
                        .zip(mesh.locate(*point, 1.0))
                        .is_some_and(|(a, b)| mesh.connected(a, b, false))
                })
                .expect("reachable roof")
        };
        app.world_mut().entity_mut(entity).insert((
            roof,
            CharacterVerticalVelocity(0.0),
            CharacterSupport::Ground,
            SurfaceAgent::default(),
        ));
        app.world_mut().get_mut::<SurfaceAgent>(entity).expect("agent").goal = Some(SurfaceGoal {
            carrier: CarrierId::WORLD,
            position: roof,
        });
        {
            let mut actors = app.world_mut().resource_mut::<ActorMap>();
            actors.set_peaceful(false);
            actors.get_mut(&id).expect("pursuer").mode = ActorMode::Engage {
                target: PlayerId(1),
                target_pos: roof,
            };
        }
        client
            .send(ClientMessage::Admin(CAdmin {
                command: "/peace on".into(),
            }))
            .expect("peace");
        let mut returned = false;
        for tick in 0..4500 {
            app.update();
            let position = *app.world().get::<Position>(entity).expect("returning Hotel actor");
            if home.contains_spawn_position(position.into()) {
                returned = true;
                break;
            }
            if tick > 4 {
                assert_eq!(
                    app.world().resource::<ActorMap>().get(&id).expect("actor").mode,
                    ActorMode::ReturnHome,
                    "zone {zone} roamed at {position:?}"
                );
            }
            assert!(
                !app.world()
                    .resource::<CollisionWorld>()
                    .character_penetrates_solid(&position, physics, &[])
            );
        }
        let agent = app.world().get::<SurfaceAgent>(entity).expect("return state");
        assert!(
            returned,
            "Hotel zone {zone}: {:?}, {:?}, {:?}",
            agent.goal, agent.failure, agent.executor
        );
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<ActorMap>().get(&id).expect("home actor").mode,
            ActorMode::Roam
        );
    }
    assert_eq!(tested.len(), 2, "exercise both Hotel scuttler roaming radii");
}
