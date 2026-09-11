use super::*;
use crate::{
    actors::ActorMap,
    cameras::{CameraAim, RENDER_LAYER_LOCAL_PLAYER, camera_aim_system, local_player_view_mode_system},
    constants::CROSSHAIR_THIRD_PERSON_HEIGHT,
    players::{MyPlayerId, PlayerMap},
    test_fixtures,
};
use bevy::camera::visibility::RenderLayers;
use common::protocol::{CarrierId, FaceYaw, PlateState, PlayerId, Wall};
use std::time::Duration;

fn world(wall: bool) -> CollisionWorld {
    let layout = MapLayout {
        walls: if wall {
            vec![Wall {
                x1: -5.0,
                z1: 0.8,
                x2: 5.0,
                z2: 0.8,
                width: 0.2,
                y: 0.0,
                height: 4.0,
                level: 0,
                carrier: CarrierId::WORLD,
            }]
        } else {
            vec![]
        },
        ..default()
    };
    CollisionWorld::from_map_layout(&layout)
}

fn app() -> (App, Entity, f32) {
    let gameplay = test_fixtures::gameplay_config();
    let eye_height = gameplay.player.eye_height();
    let mut settings = ClientSettings::load_default().expect("client settings are invalid");
    settings.camera.follow = test_fixtures::follow_camera();
    let mut app = App::new();
    app.insert_resource(gameplay)
        .insert_resource(settings)
        .insert_resource(test_fixtures::map_settings())
        .insert_resource(world(false))
        .init_resource::<MapLayout>()
        .init_resource::<Time>()
        .init_resource::<Time<Fixed>>()
        .init_resource::<CameraViewMode>()
        .init_resource::<FollowCamera>()
        .init_resource::<TopDownCameraYaw>()
        .init_resource::<LocalPlayerInfo>()
        .init_resource::<CameraAim>()
        .init_resource::<PlayerMap>()
        .init_resource::<ActorMap>()
        .init_resource::<PlateState>()
        .insert_resource(MyPlayerId(PlayerId(1)))
        .add_systems(
            Update,
            (
                local_player_camera_sync_system,
                camera_aim_system,
                local_player_view_mode_system,
            )
                .chain(),
        );
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(1.0 / 60.0));
    app.world_mut().spawn((
        LocalPlayerMarker,
        FaceYaw(0.0),
        Position::default(),
        PreviousTickPosition(Position::default()),
    ));
    let camera = app
        .world_mut()
        .spawn((
            MainCameraMarker,
            Camera3d::default(),
            Transform::default(),
            Projection::default(),
            RenderLayers::default(),
        ))
        .id();
    (app, camera, eye_height)
}

fn assert_view(app: &App, camera: Entity, eye_height: f32, first_person: bool) {
    assert_eq!(app.world().resource::<CameraViewMode>().is_first_person(), first_person);
    let layers = app.world().get::<RenderLayers>(camera).expect("camera layers missing");
    assert_eq!(
        layers.intersects(&RenderLayers::layer(RENDER_LAYER_LOCAL_PLAYER)),
        !first_person
    );
    let pose = app.world().get::<Transform>(camera).expect("camera transform missing");
    if first_person {
        assert_eq!(pose.translation, Vec3::Y * eye_height);
    } else {
        let threshold = app
            .world()
            .resource::<ClientSettings>()
            .camera
            .follow
            .first_person_distance;
        assert!(pose.translation.z > threshold);
    }
}

#[test]
fn zoom_snap_and_body_visibility_change_on_the_same_frame() {
    let (mut app, camera, eye_height) = app();
    app.update();
    assert_view(&app, camera, eye_height, true);
    for distance in [1.2, 0.8, 0.701, 0.7, 0.0, 1.0] {
        app.world_mut().resource_mut::<FollowCamera>().distance = distance;
        app.update();
        assert_view(&app, camera, eye_height, distance <= 0.7);
    }
}

#[test]
fn obstruction_snaps_to_eye_and_preserves_zoom_for_recovery() {
    let (mut app, camera, eye_height) = app();
    app.world_mut().resource_mut::<FollowCamera>().distance = 3.0;
    app.update();
    assert_view(&app, camera, eye_height, false);
    app.insert_resource(world(true));
    app.update();
    assert_view(&app, camera, eye_height, true);
    assert_eq!(app.world().resource::<FollowCamera>().distance, 3.0);
    app.insert_resource(world(false));
    for _ in 0..30 {
        app.update();
        let first = app.world().resource::<CameraViewMode>().is_first_person();
        assert_view(&app, camera, eye_height, first);
    }
    assert_view(&app, camera, eye_height, false);
}

#[test]
fn obstruction_does_not_relock_an_unlocked_orbit() {
    let (mut app, camera, eye_height) = app();
    {
        let mut third = app.world_mut().resource_mut::<FollowCamera>();
        third.distance = 3.0;
        third.locked = false;
    }
    app.update();
    assert_view(&app, camera, eye_height, false);
    app.insert_resource(world(true));
    app.update();
    assert_view(&app, camera, eye_height, true);
    assert!(!app.world().resource::<FollowCamera>().locked);
}

#[test]
fn follow_pivot_rises_from_the_eye_to_the_pivot_height() {
    let feet = Position { x: 1.0, y: 2.0, z: 3.0 };
    for (blend, height) in [(0.0, 3.6), (1.0, 3.4), (0.5, 3.5)] {
        assert!(follow_pivot(&feet, 1.6, 1.4, blend).abs_diff_eq(Vec3::new(1.0, height, 3.0), 1e-6));
    }
}

#[test]
fn near_plane_radius_never_drops_below_the_near_plane_corner() {
    let corner = Vec3::new(0.2, 0.1, 0.1).length();
    let radius = near_plane_radius(0.1, std::f32::consts::FRAC_PI_2, 2.0, 0.05);
    assert!((radius - corner).abs() < 1e-6);
    assert_eq!(near_plane_radius(0.1, std::f32::consts::FRAC_PI_2, 2.0, 0.5), 0.5);
}

#[test]
fn inward_scroll_while_obstruction_forces_first_person_clears_saved_zoom() {
    let (mut app, camera, eye_height) = app();
    app.world_mut().resource_mut::<FollowCamera>().distance = 3.0;
    app.insert_resource(world(true));
    app.update();
    assert_view(&app, camera, eye_height, true);
    let config = app.world().resource::<ClientSettings>().camera.follow;
    let view = *app.world().resource::<CameraViewMode>();
    app.world_mut()
        .resource_mut::<FollowCamera>()
        .zoom(view, 0.1, 1.0, config);

    app.insert_resource(world(false));
    for _ in 0..30 {
        app.update();
        assert_view(&app, camera, eye_height, true);
    }
    assert_eq!(app.world().resource::<FollowCamera>().distance, 0.0);
}

#[test]
fn crosshair_height_is_fixed_in_third_person_and_recentres_when_obstructed() {
    let (mut app, _, _) = app();
    app.update();
    assert_eq!(app.world().resource::<CameraAim>().crosshair_height_offset, 0.0);
    for distance in [1.0, 3.0, 6.0] {
        app.world_mut().resource_mut::<FollowCamera>().distance = distance;
        app.update();
        let offset = app.world().resource::<CameraAim>().crosshair_height_offset;
        assert_eq!(offset, CROSSHAIR_THIRD_PERSON_HEIGHT);
    }
    app.insert_resource(world(true));
    app.update();
    assert_eq!(app.world().resource::<CameraAim>().crosshair_height_offset, 0.0);
    app.insert_resource(world(false));
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<CameraAim>().crosshair_height_offset,
        CROSSHAIR_THIRD_PERSON_HEIGHT
    );
    app.insert_resource(CameraViewMode::TopDown);
    app.update();
    assert_eq!(app.world().resource::<CameraAim>().crosshair_height_offset, 0.0);
}
