use bevy::{
    ecs::system::SystemParam,
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    math::Vec2,
    prelude::*,
};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, CollisionWorld, player_jump_velocity},
    protocol::{CJump, ClientMessage, FaceYaw, MapSettings, PlayerMoveIntent, Position},
};
use std::f32::consts::{FRAC_PI_2, PI};

use crate::{
    cameras::{CameraInputState, CameraViewMode, FollowCamera, TopDownCameraYaw},
    config::ClientSettings,
    constants::INPUT_MOUSE_SENSITIVITY_BASE,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
    ui::{ConsoleState, SettingsMenuState},
};

#[derive(SystemParam)]
pub struct CameraMovementInput<'w, 's> {
    view: ResMut<'w, CameraViewMode>,
    third: ResMut<'w, FollowCamera>,
    state: Res<'w, CameraInputState>,
    wheel: MessageReader<'w, 's, MouseWheel>,
    console: Res<'w, ConsoleState>,
    menu: Res<'w, SettingsMenuState>,
}

pub const MAX_PITCH: f32 = FRAC_PI_2 - 0.05;

type LocalPlayerInputQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static mut PlayerMoveIntent,
        &'static mut FaceYaw,
        &'static mut CharacterVerticalVelocity,
    ),
    With<LocalPlayerMarker>,
>;

// Handle WASD movement and mouse rotation at render rate. Writes
// `PlayerMoveIntent` and `FaceYaw` to the local-player ECS components
// continuously so the camera and local prediction stay smooth; the network
// commit happens once per game tick in `commit_player_input_system`. Jumps
// are sent immediately on key-press — discrete events feel best with no
// commit-tick latency.
pub fn input_movement_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut camera_input: CameraMovementInput,
    to_server: Res<ClientToServerChannel>,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    mut top_down_camera_yaw: ResMut<TopDownCameraYaw>,
    mut local_player_query: LocalPlayerInputQuery,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    map_settings: Res<MapSettings>,
    client_settings: Res<ClientSettings>,
) {
    let mouse_sensitivity = INPUT_MOUSE_SENSITIVITY_BASE * client_settings.preferences.mouse_sensitivity;
    // Wait for the local player entity to exist before sampling input.
    // Otherwise we'd compute a face direction from the default camera
    // transform and write it to ECS, overwriting the authoritative spawn-time
    // facing.
    if local_player_query.is_empty() {
        for _ in mouse_motion.read() {}
        return;
    }

    let zoom: f32 = camera_input
        .wheel
        .read()
        .map(|event| match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 40.0,
        })
        .sum();
    if camera_input.state.released || camera_input.console.open || camera_input.menu.open {
        // Drain mouse events and force idle intent locally; the commit
        // system will pick it up at the next tick boundary.
        for _ in mouse_motion.read() {}
        for (_, mut input, _, _) in local_player_query.iter_mut() {
            *input = PlayerMoveIntent::Idle;
        }
        return;
    }

    let current_view = *camera_input.view;
    let view_mode = camera_input.third.zoom(
        current_view,
        zoom,
        client_settings.preferences.zoom_sensitivity,
        client_settings.camera.follow,
    );
    camera_input.view.set_if_neq(view_mode);
    let orbit = view_mode == CameraViewMode::ThirdPerson && !camera_input.third.locked;
    let (current_yaw, _) = calculate_current_orientation(
        &mut mouse_motion,
        &view_mode,
        &mut local_player_info,
        &mut top_down_camera_yaw,
        mouse_sensitivity,
        client_settings.preferences.invert_y,
    );
    let face_yaw = current_yaw + PI;
    // Death disables movement and jump just like stunned (and overrides it).
    let movement_disabled = local_player_info.is_dead || local_player_stunned(my_player_id.0, &players);
    let move_intent = calculate_move_intent(&keyboard, face_yaw, movement_disabled);
    let jump_requested = !movement_disabled && keyboard.just_pressed(KeyCode::Space);

    update_player_input_face_and_jump(
        move_intent,
        (!orbit).then_some(face_yaw),
        jump_requested,
        &collision_world,
        &gameplay_config,
        map_settings.movement.player.jump_speed,
        &mut local_player_query,
    );

    // Jump is event-shaped, sent immediately. Move-intent and face are state,
    // sent by the per-tick commit system.
    if jump_requested {
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Jump(CJump {})));
    }
}

fn calculate_current_orientation(
    mouse_motion: &mut MessageReader<MouseMotion>,
    view_mode: &CameraViewMode,
    local_player_info: &mut LocalPlayerInfo,
    top_down_camera_yaw: &mut TopDownCameraYaw,
    mouse_sensitivity: f32,
    invert_y: bool,
) -> (f32, f32) {
    // Portal presentation tilt must not feed back into mouse orientation or movement.
    let (mut current_yaw, mut current_pitch) = if !view_mode.is_top_down() {
        (local_player_info.stored_yaw, local_player_info.stored_pitch)
    } else {
        (top_down_camera_yaw.0, 0.0)
    };

    for motion in mouse_motion.read() {
        if !view_mode.is_top_down() {
            current_yaw = motion.delta.x.mul_add(-mouse_sensitivity, current_yaw);
            let pitch_step = if invert_y {
                mouse_sensitivity
            } else {
                -mouse_sensitivity
            };
            current_pitch = motion.delta.y.mul_add(pitch_step, current_pitch);
        } else {
            top_down_camera_yaw.0 = motion.delta.x.mul_add(-mouse_sensitivity, top_down_camera_yaw.0);
            current_yaw = top_down_camera_yaw.0;
        }
    }

    if !view_mode.is_top_down() {
        current_pitch = current_pitch.clamp(-MAX_PITCH, MAX_PITCH);
    } else {
        current_pitch = 0.0;
    }

    if !view_mode.is_top_down() {
        local_player_info.stored_yaw = current_yaw;
        local_player_info.stored_pitch = current_pitch;
    }
    (current_yaw, current_pitch)
}

fn calculate_move_intent(keyboard: &Res<ButtonInput<KeyCode>>, face_yaw: f32, stunned: bool) -> PlayerMoveIntent {
    if stunned {
        return PlayerMoveIntent::Idle;
    }

    let mut keyboard_vec = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        keyboard_vec.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        keyboard_vec.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        keyboard_vec.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        keyboard_vec.x -= 1.0;
    }

    if keyboard_vec.length_squared() > 0.0 {
        let normalized_input = keyboard_vec.normalize();
        let angle_offset = normalized_input.x.atan2(normalized_input.y);
        let direction = face_yaw + angle_offset;
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            PlayerMoveIntent::Running { direction }
        } else {
            PlayerMoveIntent::Walking { direction }
        }
    } else {
        PlayerMoveIntent::Idle
    }
}

fn local_player_stunned(my_player_id: common::protocol::PlayerId, players: &Res<PlayerMap>) -> bool {
    players
        .get(&my_player_id)
        .is_some_and(|player_info| player_info.stunned)
}

fn update_player_input_face_and_jump(
    move_intent: PlayerMoveIntent,
    face_yaw: Option<f32>,
    jump_requested: bool,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    jump_speed: f32,
    local_player_query: &mut LocalPlayerInputQuery,
) {
    for (pos, mut input, mut face_direction, mut motion) in local_player_query.iter_mut() {
        *input = move_intent;
        face_direction.0 = movement_facing(move_intent, face_yaw, face_direction.0);
        if jump_requested
            && let Some(vertical_velocity) = player_jump_velocity(
                motion.0,
                collision_world,
                gameplay_config.player.physics(),
                jump_speed,
                pos,
            )
        {
            motion.0 = vertical_velocity;
        }
    }
}

fn movement_facing(intent: PlayerMoveIntent, locked_yaw: Option<f32>, previous: f32) -> f32 {
    locked_yaw.unwrap_or(match intent {
        PlayerMoveIntent::Walking { direction } | PlayerMoveIntent::Running { direction } => direction,
        PlayerMoveIntent::Idle => previous,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        input::touch::TouchPhase,
        window::{CursorGrabMode, CursorOptions},
    };

    fn input_app() -> (App, Entity, Entity) {
        use crate::{
            input::{input_camera_view_toggle_system, input_cursor_capture_system},
            map::LevelFocusEnabled,
        };
        use common::protocol::{BarrierKindTable, MapLayout, PlayerId};
        let source: serde_json::Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
            .expect("server gameplay JSON is invalid");
        let config: GameplayConfig = serde_json::from_value(serde_json::json!({
            "player": source["player"], "actors": source["actors"]["kinds"],
            "projectiles": source["weapons"]["projectiles"], "missiles": source["weapons"]["missiles"],
            "portals": source["weapons"]["portals"],
        }))
        .expect("client gameplay config is invalid");
        let settings: ClientSettings = serde_json::from_str(include_str!("../../../config/client/client.json"))
            .expect("client settings JSON is invalid");
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new();
        app.insert_resource(CameraViewMode::ThirdPerson)
            .insert_resource(config)
            .insert_resource(settings)
            .insert_resource(ClientToServerChannel::new(sender))
            .insert_resource(MyPlayerId(PlayerId(1)))
            .insert_resource(crate::test_geometry::map_settings())
            .insert_resource(CollisionWorld::from_map_layout(
                &MapLayout::default(),
                &BarrierKindTable::default(),
            ))
            .init_resource::<PlayerMap>()
            .init_resource::<LocalPlayerInfo>()
            .init_resource::<TopDownCameraYaw>()
            .init_resource::<FollowCamera>()
            .init_resource::<CameraInputState>()
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
                    input_cursor_capture_system,
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
    fn wheel_switches_follow_views_and_menu_scroll_does_not_zoom() {
        use crate::constants::INPUT_ZOOM_SENSITIVITY_BASE;
        let (mut app, _, window) = input_app();
        *app.world_mut().resource_mut::<CameraViewMode>() = CameraViewMode::FirstPerson;
        app.world_mut()
            .resource_mut::<ClientSettings>()
            .preferences
            .zoom_sensitivity = 1.0 / INPUT_ZOOM_SENSITIVITY_BASE;
        app.world_mut().write_message(MouseWheel {
            phase: TouchPhase::Moved,
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: -1.0,
            window,
        });
        app.update();
        assert_eq!(*app.world().resource::<CameraViewMode>(), CameraViewMode::ThirdPerson);
        assert_eq!(app.world().resource::<FollowCamera>().distance, 1.0);

        app.world_mut().resource_mut::<FollowCamera>().locked = false;
        app.world_mut().write_message(MouseWheel {
            phase: TouchPhase::Moved,
            unit: MouseScrollUnit::Pixel,
            x: 0.0,
            y: 40.0,
            window,
        });
        app.update();
        assert_eq!(*app.world().resource::<CameraViewMode>(), CameraViewMode::FirstPerson);
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
        assert_eq!(*app.world().resource::<CameraViewMode>(), CameraViewMode::FirstPerson);
        assert_eq!(app.world().resource::<FollowCamera>().distance, 0.0);
    }
}
