use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    physics::CollisionWorld,
    protocol::{FaceYaw, PlayerId, PlayerMoveIntent, PortalAccess, Position},
};
use std::f32::consts::PI;

use super::{WeaponMode, portals::portal_end_for_button};
use crate::{
    cameras::{CameraInputState, CameraViewMode, FollowCamera},
    config::ClientSettings,
    constants::{CAMERA_MAX_PITCH, INPUT_MOUSE_SENSITIVITY_BASE},
    network::PlaybackMode,
    players::{
        JumpRequest, LocalMovementStep, LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap, ladder_facing,
    },
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
        &'static mut JumpRequest,
        &'static LocalMovementStep,
    ),
    With<LocalPlayerMarker>,
>;

// Sample once per frame before fixed simulation. A jump sets the request
// once; the owner's tick consumes it, so it survives a frame without steps.
pub fn input_movement_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    playback: Option<Res<PlaybackMode>>,
    camera_input: CameraMovementInput,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    mut local_player_query: LocalPlayerInputQuery,
    collision_world: Res<CollisionWorld>,
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
        if playback.is_none() {
            for (_, mut input, _, _, _) in local_player_query.iter_mut() {
                *input = PlayerMoveIntent::NONE;
            }
        }
        return;
    }

    let view_mode = *camera_input.view;
    let orbit = view_mode.is_debug() || (view_mode == CameraViewMode::ThirdPerson && !camera_input.follow.locked);
    let current_yaw = calculate_current_orientation(
        camera_input.state.mouse_delta,
        &mut local_player_info,
        mouse_sensitivity,
        client_settings.preferences.invert_y,
    );
    if playback.is_some() {
        return;
    }
    let face_yaw = current_yaw + PI;
    // Death disables movement and jump just like stunned (and overrides it).
    let movement_disabled = local_player_info.is_dead || local_player_stunned(my_player_id.0, &players);
    let mut move_intent = calculate_move_intent(&keyboard, face_yaw, movement_disabled);
    move_intent.pitch = local_player_info.stored_pitch;
    let jump_requested = !movement_disabled && keyboard.just_pressed(KeyCode::Space);
    let locked_yaw = (!orbit || (!local_player_info.is_dead && camera_input.aiming_weapon())).then_some(face_yaw);
    for (pos, mut input, mut face_direction, mut jump, step) in local_player_query.iter_mut() {
        *input = move_intent;
        face_direction.0 = ladder_facing(&collision_world, pos, step.result.support)
            .unwrap_or_else(|| movement_facing(move_intent, locked_yaw, face_direction.0));
        jump.pressed |= jump_requested;
    }
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
        return PlayerMoveIntent::NONE;
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

    PlayerMoveIntent {
        forward: keyboard_vec.y,
        sideways: keyboard_vec.x,
        yaw: face_yaw,
        pitch: 0.0,
        crouch: keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight),
    }
}

fn local_player_stunned(my_player_id: PlayerId, players: &PlayerMap) -> bool {
    players
        .get(&my_player_id)
        .is_some_and(|player_info| player_info.stunned)
}

fn movement_facing(intent: PlayerMoveIntent, locked_yaw: Option<f32>, previous: f32) -> f32 {
    locked_yaw.or_else(|| intent.direction()).unwrap_or(previous)
}
