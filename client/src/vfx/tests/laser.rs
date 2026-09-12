use super::*;
use crate::test_fixtures;
use crate::{actors::ActorInfo, players::PlayerInfo};
use bevy::audio::PlaybackMode;
use common::protocol::{ActorBeam, Health, Player, PlayerMoveIntent};

#[test]
fn wander_offset_stays_inside_the_fraction_scaled_hitbox() {
    let hitbox = HitboxConfig {
        width: 1.0,
        height: 2.0,
        depth: 0.5,
        bottom_offset: 0.0,
    };
    let bound = Vec3::new(
        0.5 * LASER_ENDPOINT_WANDER_WIDTH_FRACTION,
        LASER_ENDPOINT_WANDER_HEIGHT_FRACTION,
        0.25 * LASER_ENDPOINT_WANDER_WIDTH_FRACTION,
    );
    for step in 0..500 {
        let offset = wander_offset(ActorId(3), &hitbox, step as f32 * 0.01);
        assert!(offset.abs().cmple(bound).all(), "wander left the hitbox: {offset}");
    }
    assert_ne!(
        wander_offset(ActorId(1), &hitbox, 0.0),
        wander_offset(ActorId(2), &hitbox, 0.0)
    );
}

#[test]
fn beam_pose_spans_the_muzzle_to_the_clip_point() {
    let origin = Vec3::new(1.0, 2.0, 3.0);
    let direction = Vec3::new(0.0, 0.6, 0.8);
    let pose = beam_pose(origin, direction, 5.0, 0.5).expect("beam pose missing");
    assert!(
        pose.transform_point(Vec3::NEG_Y * 0.5)
            .abs_diff_eq(origin + direction * 0.5, 1e-5)
    );
    assert!(
        pose.transform_point(Vec3::Y * 0.5)
            .abs_diff_eq(origin + direction * 5.0, 1e-5)
    );
    assert!(beam_pose(origin, direction, 0.4, 0.5).is_none());
}

fn sync_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<AudioSource>()
        .insert_resource(test_fixtures::asset_set())
        .insert_resource(test_fixtures::client_settings())
        .init_resource::<ServerTick>()
        .init_resource::<NetworkConfig>()
        .init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .add_systems(Update, laser_beams_sync_system);
    app
}

#[test]
fn missing_fire_sound_keeps_the_beam_visual() {
    let mut app = sync_app();
    let asset_server = app.world().resource::<AssetServer>().clone();
    spawn_laser_beam(
        &mut app.world_mut().commands(),
        &mut Assets::<Mesh>::default(),
        &mut Assets::<StandardMaterial>::default(),
        &asset_server,
        &test_fixtures::asset_set(),
        &test_fixtures::client_settings(),
        LaserBeam {
            actor: ActorId(1),
            target: PlayerId(1),
            started_tick: 1,
        },
        "scuttler",
        Vec3::ZERO,
        0.0,
    );
    app.world_mut().flush();
    let beam = app
        .world_mut()
        .query_filtered::<Entity, With<LaserBeam>>()
        .single(app.world())
        .expect("beam missing");
    assert!(app.world().get::<Mesh3d>(beam).is_some());
    assert!(app.world().get::<AudioPlayer>(beam).is_none());
}

#[test]
fn beam_keeps_one_effect_and_sound_through_retargeting() {
    let mut app = sync_app();
    for id in [PlayerId(1), PlayerId(2)] {
        let entity = app.world_mut().spawn_empty().id();
        let player = Player::new(
            "Player".into(),
            Position::default(),
            PlayerMoveIntent::default(),
            0.0,
            0,
            Health(500.0),
        );
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .insert(id, PlayerInfo::from_snapshot(entity, &player, 0));
    }
    let id = ActorId(1);
    let entity = app.world_mut().spawn((ActorMarker, Position::default())).id();
    let mut actor = ActorInfo {
        entity,
        kind: "turret".into(),
        beam: Default::default(),
    };
    actor.beam.apply(
        1,
        Some(ActorBeam {
            target: PlayerId(1),
            started_tick: 1,
            remaining_secs: 2.0,
        }),
    );
    app.world_mut().resource_mut::<ActorMap>().insert(id, actor);
    app.world_mut().resource_mut::<ServerTick>().0 = 16;
    app.update();
    let beam = app
        .world_mut()
        .query_filtered::<Entity, With<LaserBeam>>()
        .single(app.world())
        .expect("beam missing");
    assert!(app.world().get::<AudioPlayer>(beam).is_some());
    let playback = app
        .world()
        .get::<PlaybackSettings>(beam)
        .expect("beam playback missing");
    assert!(matches!(playback.mode, PlaybackMode::Once));
    assert_eq!(playback.start_position, Some(Duration::from_secs_f32(0.5)));
    for tick in 2..5 {
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&id)
            .expect("turret missing")
            .beam
            .apply(
                tick,
                Some(ActorBeam {
                    target: PlayerId(2),
                    started_tick: 1,
                    remaining_secs: 2.0,
                }),
            );
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<LaserBeam>>()
                .single(app.world())
                .expect("beam missing"),
            beam
        );
        assert_eq!(
            app.world().get::<LaserBeam>(beam).expect("beam missing").target,
            PlayerId(2)
        );
    }
    app.world_mut()
        .resource_mut::<ActorMap>()
        .get_mut(&id)
        .expect("turret missing")
        .beam
        .apply(5, None);
    app.update();
    assert!(app.world().get_entity(beam).is_err());
    app.world_mut()
        .resource_mut::<ActorMap>()
        .get_mut(&id)
        .expect("turret missing")
        .beam
        .apply(
            6,
            Some(ActorBeam {
                target: PlayerId(1),
                started_tick: 6,
                remaining_secs: 2.0,
            }),
        );
    app.update();
    let second_beam = app
        .world_mut()
        .query_filtered::<Entity, With<LaserBeam>>()
        .single(app.world())
        .expect("beam missing");
    // A snapshot can skip the short cooldown and show another burst at the same player.
    app.world_mut()
        .resource_mut::<ActorMap>()
        .get_mut(&id)
        .expect("turret missing")
        .beam
        .apply(
            70,
            Some(ActorBeam {
                target: PlayerId(1),
                started_tick: 70,
                remaining_secs: 2.0,
            }),
        );
    app.world_mut().resource_mut::<ServerTick>().0 = 70;
    app.update();
    assert!(app.world().get_entity(second_beam).is_err());
    let third_beam = app
        .world_mut()
        .query_filtered::<Entity, With<LaserBeam>>()
        .single(app.world())
        .expect("beam missing");
    assert!(app.world().get::<AudioPlayer>(third_beam).is_some());
    app.world_mut().resource_mut::<ServerTick>().0 = 131;
    app.update();
    assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
    app.update();
    assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
    app.world_mut().resource_mut::<ActorMap>().remove(&id);
    app.update();
    assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
}

#[test]
fn peace_mode_removes_beams_including_late_cues() {
    let mut app = sync_app();
    app.world_mut().resource_mut::<ActorMap>().peaceful = true;
    for _ in 0..2 {
        for started_tick in [0, 10] {
            app.world_mut().spawn(LaserBeam {
                actor: ActorId(1),
                target: PlayerId(1),
                started_tick,
            });
        }
        app.update();
        assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
    }
}
