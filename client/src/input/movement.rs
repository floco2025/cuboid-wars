use bevy::{ecs::system::SystemParam, input::mouse::AccumulatedMouseMotion, math::Vec2, prelude::*};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, CollisionWorld, player_jump_velocity},
    protocol::{FaceYaw, MapSettings, PlayerId, PlayerMoveIntent, PortalAccess, Position},
};
use std::f32::consts::PI;

use super::{WeaponMode, portals::portal_end_for_button};
use crate::{
    cameras::{CameraInputState, CameraViewMode, FollowCamera},
    config::ClientSettings,
    constants::{CAMERA_MAX_PITCH, INPUT_MOUSE_SENSITIVITY_BASE},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
    ui::{ConsoleState, SettingsMenuState},
};

#[derive(SystemParam)]
pub struct CameraMovementInput<'w> {
    view: Res<'w, CameraViewMode>,
    follow: Res<'w, FollowCamera>,
    state: Res<'w, CameraInputState>,
    console: Res<'w, ConsoleState>,
    menu: Res<'w, SettingsMenuState>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    weapon: Res<'w, WeaponMode>,
    portal_access: Res<'w, PortalAccess>,
}

impl CameraMovementInput<'_> {
    fn aiming_weapon(&self) -> bool {
        if self.state.suppress_fire {
            return false;
        }
        match *self.weapon {
            WeaponMode::None => false,
            WeaponMode::Portal => [MouseButton::Left, MouseButton::Right].into_iter().any(|button| {
                self.mouse.pressed(button) && portal_end_for_button(*self.portal_access, button).is_some()
            }),
            _ => self.mouse.pressed(MouseButton::Left),
        }
    }
}

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
// continuously so the camera and local prediction stay smooth; the movement
// they produce reaches the server once per game tick in
// `report_player_movement_system`, a jump as the vertical velocity it set.
pub fn input_movement_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    camera_input: CameraMovementInput,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
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
        return;
    }

    if camera_input.state.released || camera_input.console.open || camera_input.menu.open {
        // Force idle intent locally; the commit system will pick it up at
        // the next tick boundary.
        for (_, mut input, _, _) in local_player_query.iter_mut() {
            *input = PlayerMoveIntent::Idle;
        }
        return;
    }

    let view_mode = *camera_input.view;
    let orbit = view_mode.is_debug() || (view_mode == CameraViewMode::ThirdPerson && !camera_input.follow.locked);
    let current_yaw = calculate_current_orientation(
        mouse_motion.delta,
        &mut local_player_info,
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
        (!orbit || (!local_player_info.is_dead && camera_input.aiming_weapon())).then_some(face_yaw),
        jump_requested,
        &collision_world,
        &gameplay_config,
        map_settings.movement.player.jump_speed,
        &mut local_player_query,
    );
}

// Applies this frame's mouse motion to the view yaw and pitch and returns
// the resulting view yaw.
fn calculate_current_orientation(
    mouse_delta: Vec2,
    local_player_info: &mut LocalPlayerInfo,
    mouse_sensitivity: f32,
    invert_y: bool,
) -> f32 {
    // The stored aim is the input, not the camera: portal presentation tilt
    // must not feed back into mouse orientation or movement.
    let pitch_step = if invert_y {
        mouse_sensitivity
    } else {
        -mouse_sensitivity
    };
    local_player_info.stored_yaw = mouse_delta.x.mul_add(-mouse_sensitivity, local_player_info.stored_yaw);
    local_player_info.stored_pitch = mouse_delta.y.mul_add(pitch_step, local_player_info.stored_pitch);
    local_player_info.stored_pitch = local_player_info
        .stored_pitch
        .clamp(-CAMERA_MAX_PITCH, CAMERA_MAX_PITCH);
    local_player_info.stored_yaw
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

fn local_player_stunned(my_player_id: PlayerId, players: &PlayerMap) -> bool {
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
