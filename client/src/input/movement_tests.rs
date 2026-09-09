use std::f32::consts::PI;

use bevy::{
    input::{
        mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
    },
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use common::{
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{
        BarrierKindTable, ClientMessage, FaceYaw, MapLayout, PlayerId, PlayerMoveIntent, PortalAccess, PortalPairId,
        Position,
    },
};

use super::{
    WeaponMode, commit_player_input_system, input_camera_view_toggle_system, input_camera_zoom_system,
    input_cursor_capture_system, input_facing_lock_toggle_system, input_movement_system,
};
use crate::{
    cameras::{CameraInputState, CameraViewMode, FollowCamera, TopDownCameraYaw},
    config::ClientSettings,
    constants::{INPUT_ZOOM_PIXELS_PER_LINE, INPUT_ZOOM_SENSITIVITY_BASE},
    map::LevelFocusEnabled,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
    test_fixtures,
    ui::{ConsoleState, SettingsMenuState},
};

fn input_app() -> (App, Entity, Entity) {
    let settings: ClientSettings = serde_json::from_str(include_str!("../../../config/client/client.json"))
        .expect("client settings JSON is invalid");
    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new();
    app.insert_resource(CameraViewMode::ThirdPerson)
        .insert_resource(test_fixtures::gameplay_config())
        .insert_resource(settings)
        .insert_resource(ClientToServerChannel::new(sender))
        .insert_resource(MyPlayerId(PlayerId(1)))
        .insert_resource(test_fixtures::map_settings())
        .insert_resource(CollisionWorld::from_map_layout(
            &MapLayout::default(),
            &BarrierKindTable::default(),
        ))
        .init_resource::<PlayerMap>()
        .init_resource::<LocalPlayerInfo>()
        .init_resource::<TopDownCameraYaw>()
        .init_resource::<FollowCamera>()
        .init_resource::<CameraInputState>()
        .init_resource::<WeaponMode>()
        .insert_resource(PortalAccess::None)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ConsoleState>()
        .init_resource::<SettingsMenuState>()
        .init_resource::<LevelFocusEnabled>()
        .add_message::<MouseMotion>()
        .add_message::<MouseWheel>()
        .add_systems(
            Update,
            (
                input_camera_view_toggle_system,
                input_facing_lock_toggle_system,
                input_cursor_capture_system,
                input_camera_zoom_system,
                input_movement_system,
            )
                .chain(),
        );
    let cursor = app.world_mut().spawn(CursorOptions::default()).id();
    let player = app
        .world_mut()
        .spawn((
            LocalPlayerMarker,
            Position::default(),
            FaceYaw(0.0),
            PlayerMoveIntent::Idle,
            CharacterVerticalVelocity(0.0),
        ))
        .id();
    (app, player, cursor)
}

#[test]
fn unlocked_movement_orbit_lock_and_menu_use_independent_controls() {
    let (mut app, player, cursor) = input_app();
    assert!(app.world().resource::<FollowCamera>().locked);
    app.world_mut().write_message(MouseMotion {
        delta: Vec2::new(100.0, 0.0),
    });
    app.update();
    let locked_face = app
        .world()
        .get::<FaceYaw>(player)
        .expect("facing missing from test player")
        .0;
    assert!((locked_face - (PI - 0.2)).abs() < 1e-5);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyF);
    app.world_mut().write_message(MouseMotion {
        delta: Vec2::new(100.0, 0.0),
    });
    app.update();
    assert!(!app.world().resource::<FollowCamera>().locked);
    assert!((app.world().resource::<LocalPlayerInfo>().stored_yaw + 0.4).abs() < 1e-5);
    assert_eq!(
        app.world()
            .get::<FaceYaw>(player)
            .expect("facing missing from test player")
            .0,
        locked_face
    );
    assert_eq!(
        app.world()
            .get::<CursorOptions>(cursor)
            .expect("cursor options missing from test window")
            .grab_mode,
        CursorGrabMode::Locked
    );

    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyD);
    app.update();
    let direction = match *app
        .world()
        .get::<PlayerMoveIntent>(player)
        .expect("movement intent missing from test player")
    {
        PlayerMoveIntent::Walking { direction } => direction,
        _ => panic!("unlocked camera blocked walking"),
    };
    assert_eq!(
        app.world()
            .get::<FaceYaw>(player)
            .expect("facing missing from test player")
            .0,
        direction
    );

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyF);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyF);
    app.update();
    assert!(app.world().resource::<FollowCamera>().locked);
    assert!(
        (app.world()
            .get::<FaceYaw>(player)
            .expect("facing missing from test player")
            .0
            - (PI - 0.4))
            .abs()
            < 1e-5
    );

    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
    for modifier in [
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ] {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyF);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(modifier);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyF);
        app.update();
        assert!(
            app.world().resource::<FollowCamera>().locked,
            "fullscreen shortcut toggled camera lock"
        );
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(modifier);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
    }
    app.world_mut().resource_mut::<SettingsMenuState>().open = true;
    app.world_mut().write_message(MouseMotion {
        delta: Vec2::new(100.0, 0.0),
    });
    app.update();
    assert!((app.world().resource::<LocalPlayerInfo>().stored_yaw + 0.4).abs() < 1e-5);
    assert_eq!(
        *app.world()
            .get::<PlayerMoveIntent>(player)
            .expect("movement intent missing from test player"),
        PlayerMoveIntent::Idle
    );
    assert_eq!(
        app.world()
            .get::<CursorOptions>(cursor)
            .expect("cursor options missing from test window")
            .grab_mode,
        CursorGrabMode::None
    );
}

#[test]
fn unlocked_firing_faces_view_without_changing_movement_or_lock_and_commits_facing() {
    for (weapon, button, access) in [
        (WeaponMode::Projectile, MouseButton::Left, PortalAccess::None),
        (WeaponMode::MultiShot(0), MouseButton::Left, PortalAccess::None),
        (WeaponMode::Missile, MouseButton::Left, PortalAccess::None),
        (
            WeaponMode::Portal,
            MouseButton::Left,
            PortalAccess::Both { pair: PortalPairId(1) },
        ),
        (
            WeaponMode::Portal,
            MouseButton::Right,
            PortalAccess::Both { pair: PortalPairId(1) },
        ),
    ] {
        let (mut app, player, _) = input_app();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ClientToServerChannel::new(sender))
            .insert_resource(weapon)
            .insert_resource(access)
            .add_systems(Update, commit_player_input_system.after(input_movement_system));
        app.world_mut().resource_mut::<FollowCamera>().locked = false;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyD);
        app.update();
        let movement = *app
            .world()
            .get::<PlayerMoveIntent>(player)
            .expect("player intent missing");
        let walking_yaw = app.world().get::<FaceYaw>(player).expect("player facing missing").0;
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(button);
        for _ in 0..3 {
            app.update();
            let face = app.world().get::<FaceYaw>(player).expect("player facing missing").0;
            assert!((face - PI).abs() < 1e-5);
            assert_ne!(face, walking_yaw);
            assert_eq!(
                *app.world()
                    .get::<PlayerMoveIntent>(player)
                    .expect("player intent missing"),
                movement
            );
            assert!(!app.world().resource::<FollowCamera>().locked);
            assert_eq!(app.world().resource::<LocalPlayerInfo>().stored_yaw, 0.0);
        }
        let mut committed_yaw = None;
        while let Ok(ClientToServer::Send(message)) = receiver.try_recv() {
            if let ClientMessage::Move(message) = message {
                committed_yaw = Some(message.input.face_yaw);
            }
        }
        assert_eq!(committed_yaw, Some(PI));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(button);
        app.update();
        assert_eq!(
            app.world().get::<FaceYaw>(player).expect("player facing missing").0,
            walking_yaw
        );
    }
}

#[test]
fn unlocked_idle_retains_shot_facing_but_empty_hands_and_menus_do_not_turn() {
    let (mut app, player, _) = input_app();
    app.world_mut().resource_mut::<FollowCamera>().locked = false;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    assert_eq!(
        app.world().get::<FaceYaw>(player).expect("player facing missing").0,
        0.0
    );
    app.insert_resource(WeaponMode::Projectile);
    app.world_mut().resource_mut::<SettingsMenuState>().open = true;
    app.update();
    assert_eq!(
        app.world().get::<FaceYaw>(player).expect("player facing missing").0,
        0.0
    );
    app.world_mut().resource_mut::<SettingsMenuState>().open = false;
    app.update();
    assert_eq!(app.world().get::<FaceYaw>(player).expect("player facing missing").0, PI);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.world_mut().write_message(MouseMotion {
        delta: Vec2::new(100.0, 0.0),
    });
    app.update();
    assert_eq!(app.world().get::<FaceYaw>(player).expect("player facing missing").0, PI);
    assert!(app.world().resource::<LocalPlayerInfo>().stored_yaw.abs() > 0.0);
}

#[test]
fn wheel_zooms_across_the_first_person_threshold_and_menu_scroll_does_not_zoom() {
    let (mut app, _, window) = input_app();
    let follow = app.world().resource::<ClientSettings>().camera.follow;
    let zoom_step = (follow.first_person_distance + follow.max_distance) * 0.5;
    *app.world_mut().resource_mut::<CameraViewMode>() = CameraViewMode::FirstPerson;
    app.world_mut()
        .resource_mut::<ClientSettings>()
        .preferences
        .zoom_sensitivity = zoom_step / INPUT_ZOOM_SENSITIVITY_BASE;
    app.world_mut().write_message(MouseWheel {
        phase: TouchPhase::Moved,
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: -1.0,
        window,
    });
    app.update();
    assert_eq!(app.world().resource::<FollowCamera>().distance, zoom_step);

    app.world_mut().resource_mut::<FollowCamera>().locked = false;
    app.world_mut().write_message(MouseWheel {
        phase: TouchPhase::Moved,
        unit: MouseScrollUnit::Pixel,
        x: 0.0,
        y: INPUT_ZOOM_PIXELS_PER_LINE,
        window,
    });
    app.update();
    assert_eq!(app.world().resource::<FollowCamera>().distance, 0.0);
    assert!(app.world().resource::<FollowCamera>().locked);

    app.world_mut().resource_mut::<SettingsMenuState>().open = true;
    app.world_mut().write_message(MouseWheel {
        phase: TouchPhase::Moved,
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: -4.0,
        window,
    });
    app.update();
    app.world_mut().resource_mut::<SettingsMenuState>().open = false;
    app.update();
    assert_eq!(app.world().resource::<FollowCamera>().distance, 0.0);
}
