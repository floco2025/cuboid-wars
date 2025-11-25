use crate::{
    components::LocalPlayer,
    net::{ClientToServer, ServerToClient},
    resources::{ClientToServerChannel, MyPlayerId, ServerToClientChannel},
};
use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;

// ============================================================================
// Setup World System
// ============================================================================

// World dimensions
pub const WORLD_SIZE: f32 = 2000.0;

// Camera settings
pub const CAMERA_X: f32 = 0.0;
pub const CAMERA_Y: f32 = 1500.0;
pub const CAMERA_Z: f32 = 2000.0;

// Player cuboid dimensions - make it asymmetric so we can see orientation
pub const PLAYER_WIDTH: f32 = 20.0; // side to side
pub const PLAYER_HEIGHT: f32 = 80.0; // up/down
pub const PLAYER_DEPTH: f32 = 40.0; // front to back (longer)

/// Toggle cursor lock with Escape key or mouse click
pub fn cursor_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    mut cursor_options: Single<&mut bevy::window::CursorOptions>,
) {
    // Escape key toggles cursor lock
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor_options.visible = !cursor_options.visible;
        cursor_options.grab_mode = if cursor_options.visible {
            bevy::window::CursorGrabMode::None
        } else {
            bevy::window::CursorGrabMode::Locked
        };
    }

    // Left click locks cursor if it's currently unlocked
    if mouse.just_pressed(bevy::input::mouse::MouseButton::Left) && cursor_options.visible {
        cursor_options.visible = false;
        cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
    }
}

pub fn setup_world_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create the ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(WORLD_SIZE, WORLD_SIZE))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::default(),
    ));

    // Add camera with top-down view
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(CAMERA_X, CAMERA_Y, CAMERA_Z).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Add a directional light
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Add ambient light so everything is visible
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.5,
        affects_lightmapped_meshes: false,
    });
}

// ============================================================================
// Server Event Processing System
// ============================================================================

// System to process messages from the server
pub fn process_server_events_system(
    mut commands: Commands,
    mut from_server: ResMut<ServerToClientChannel>,
    mut exit: MessageWriter<AppExit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player_query: Query<(Entity, &PlayerId)>,
    mut my_player_id: Local<Option<PlayerId>>,
) {
    // Process all messages from the server
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToClient::Disconnected => {
                error!("disconnected from server");
                exit.write(AppExit::Success);
            }
            ServerToClient::Message(message) => {
                if my_player_id.is_some() {
                    process_message_logged_in(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &player_query,
                        *my_player_id,
                        &message,
                    );
                } else {
                    process_message_not_logged_in(&mut commands, &message, &mut my_player_id);
                }
            }
        }
    }
}

// ============================================================================
// Process Messages
// ============================================================================

fn process_message_not_logged_in(
    commands: &mut Commands,
    msg: &ServerMessage,
    my_player_id: &mut Local<Option<PlayerId>>,
) {
    match msg {
        ServerMessage::Init(init_msg) => {
            debug!("received Init: my_id={:?}", init_msg.id);

            // Store in Local (immediate) and insert resource (deferred)
            **my_player_id = Some(init_msg.id);
            commands.insert_resource(MyPlayerId(init_msg.id));

            // Note: We don't spawn anything here. The first SUpdate will contain
            // all players including ourselves and will trigger spawning via the
            // Update message handler.
        }
        _ => {
            warn!("received non-Init message before Init (out-of-order delivery)");
        }
    }
}

fn process_message_logged_in(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_query: &Query<(Entity, &PlayerId)>,
    my_player_id: Option<PlayerId>,
    msg: &ServerMessage,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::Login(msg) => {
            debug!("{:?} logged in", msg.id);
            // Login is always for another player (server doesn't send our own login back)
            spawn_player(
                commands,
                meshes,
                materials,
                msg.id.0,
                &msg.player.kin.pos,
                &msg.player.kin.vel,
                &msg.player.kin.rot,
                false, // Never local
            );
        }
        ServerMessage::Logoff(msg) => {
            debug!("{:?} logged off (graceful: {})", msg.id, msg.graceful);
            // Find and despawn the entity with this PlayerId
            for (entity, player_id) in player_query.iter() {
                if *player_id == msg.id {
                    commands.entity(entity).despawn();
                    break;
                }
            }
        }
        ServerMessage::PlayerVelocity(msg) => {
            trace!("{:?} velocity: {:?}", msg.id, msg);
            // Update player velocity (both local and remote)
            let mut found = false;
            for (entity, player_id) in player_query.iter() {
                if *player_id == msg.id {
                    commands.entity(entity).insert(msg.vel);
                    found = true;
                    break;
                }
            }
            if !found {
                warn!("received velocity for non-existent player {:?}", msg.id);
            }
        }
        ServerMessage::PlayerRotation(msg) => {
            trace!("{:?} rotation: {:?}", msg.id, msg);
            // Update player rotation (both local and remote)
            let mut found = false;
            for (entity, player_id) in player_query.iter() {
                if *player_id == msg.id {
                    commands.entity(entity).insert(msg.rot);
                    found = true;
                    break;
                }
            }
            if !found {
                warn!("received rotation for non-existent player {:?}", msg.id);
            }
        }
        ServerMessage::Update(msg) => {
            debug!("update: {:?}", msg);

            // Get my player ID to identify local player
            let my_id: Option<u32> = my_player_id.map(|id| id.0);

            // Collect existing player IDs
            let existing_players: std::collections::HashSet<PlayerId> =
                player_query.iter().map(|(_, id)| *id).collect();

            // Collect player IDs in this Update message
            let update_players: std::collections::HashSet<PlayerId> = msg.players.iter().map(|(id, _)| *id).collect();

            // Spawn missing players (in Update but not in our world)
            for (id, player) in &msg.players {
                if !existing_players.contains(id) {
                    debug!("spawning player {:?} from Update", id);
                    let is_local = my_id.map_or(false, |my| my == (*id).0);
                    spawn_player(
                        commands,
                        meshes,
                        materials,
                        id.0,
                        &player.kin.pos,
                        &player.kin.vel,
                        &player.kin.rot,
                        is_local,
                    );
                }
            }

            // Despawn players that no longer exist (in our world but not in Update)
            for (entity, player_id) in player_query.iter() {
                if !update_players.contains(player_id) {
                    warn!("despawning player {:?} from Update", player_id);
                    commands.entity(entity).despawn();
                }
            }

            // Update all players with new state
            for (id, player) in &msg.players {
                for (entity, player_id) in player_query.iter() {
                    if *player_id == *id {
                        commands
                            .entity(entity)
                            .insert((player.kin.pos, player.kin.vel, player.kin.rot));
                        break;
                    }
                }
            }
        }
    }
}

// ============================================================================
// Input System
// ============================================================================

/// Handle WASD movement and mouse rotation for first-person view
pub fn input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    to_server: Res<ClientToServerChannel>,
    time: Res<Time>,
    mut last_velocity: Local<(f32, f32)>,
    mut last_send_time: Local<f32>,
    mut rotation: Local<f32>,           // Yaw rotation in radians
    mut last_sent_rotation: Local<f32>, // Last rotation sent to server
    mut local_player_query: Query<&mut Velocity, With<LocalPlayer>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    const SPEED: f32 = 100_000.0; // 100,000 mm/sec = 100 meters/sec
    const MOUSE_SENSITIVITY: f32 = 0.002; // radians per pixel
    const VELOCITY_SEND_INTERVAL: f32 = 0.1; // Send velocity updates at most every 100ms
    const VELOCITY_CHANGE_THRESHOLD: f32 = 1000.0; // Only send if velocity changed by at least 1000 mm/sec
    const ROTATION_CHANGE_THRESHOLD: f32 = 0.05; // ~3 degrees

    // Only process input when cursor is locked
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;

    if cursor_locked {
        // Handle mouse rotation
        for motion in mouse_motion.read() {
            *rotation -= motion.delta.x * MOUSE_SENSITIVITY;
        }

        // Get forward/right vectors from rotation
        let forward_x = -rotation.sin();
        let forward_z = -rotation.cos();
        let right_x = forward_z;
        let right_z = -forward_x;

        // Handle WASD input relative to camera direction
        let mut forward = 0.0_f32;
        let mut right = 0.0_f32;

        if keyboard.pressed(KeyCode::KeyW) {
            forward += 1.0;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            forward -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            right += 1.0;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            right -= 1.0;
        }

        // Calculate world-space velocity
        let dx = forward * forward_x + right * right_x;
        let dy = forward * forward_z + right * right_z;

        // Normalize and calculate velocity
        let len = (dx * dx + dy * dy).sqrt();
        let vel = if len > 0.0 {
            Velocity {
                x: (dx / len) * SPEED,
                y: (dy / len) * SPEED,
            }
        } else {
            Velocity { x: 0.0, y: 0.0 }
        };

        // Always update local player's velocity immediately for responsive local movement
        for mut player_vel in local_player_query.iter_mut() {
            *player_vel = vel;
        }

        // Calculate velocity change magnitude
        let vel_change = ((vel.x - last_velocity.0).powi(2) + (vel.y - last_velocity.1).powi(2)).sqrt();

        // Only accumulate send time if we're actually moving
        if vel.x != 0.0 || vel.y != 0.0 {
            *last_send_time += time.delta_secs();
        }

        // Send to server only if:
        // 1. Velocity changed significantly (e.g., started/stopped moving), OR
        // 2. We're moving AND velocity changed AND enough time has passed (throttle rotation-induced updates)
        let should_send =
            vel_change > VELOCITY_CHANGE_THRESHOLD || (vel_change > 0.0 && *last_send_time >= VELOCITY_SEND_INTERVAL);

        if should_send {
            let msg = ClientMessage::Velocity(CVelocity { vel });
            let _ = to_server.send(ClientToServer::Send(msg));
            *last_velocity = (vel.x, vel.y);
            *last_send_time = 0.0;
        }

        // Send rotation to server whenever it changes significantly
        let rotation_change = (*rotation - *last_sent_rotation).abs();
        if rotation_change > ROTATION_CHANGE_THRESHOLD {
            // Convert camera rotation to world rotation
            // Camera uses: forward_x = -sin(rotation), forward_z = -cos(rotation)
            // World rotation: rotation + π
            let yaw_for_server = *rotation + std::f32::consts::PI;
            let msg = ClientMessage::Rotation(CRotation {
                rot: Rotation { yaw: yaw_for_server },
            });
            let _ = to_server.send(ClientToServer::Send(msg));
            *last_sent_rotation = *rotation;
        }

        // Update camera rotation
        for mut transform in camera_query.iter_mut() {
            transform.rotation = Quat::from_rotation_y(*rotation);
        }
    } else {
        // Cursor not locked - clear mouse motion events to prevent them from accumulating
        for _ in mouse_motion.read() {}

        // Stop player movement when cursor is unlocked
        if last_velocity.0 != 0.0 || last_velocity.1 != 0.0 {
            let vel = Velocity { x: 0.0, y: 0.0 };
            for mut player_vel in local_player_query.iter_mut() {
                *player_vel = vel;
            }
            let msg = ClientMessage::Velocity(CVelocity { vel });
            let _ = to_server.send(ClientToServer::Send(msg));
            *last_velocity = (0.0, 0.0);
            *last_send_time = 0.0;
        }
    }
}

/// Update camera position to follow local player
pub fn sync_camera_to_player_system(
    local_player_query: Query<&Position, With<LocalPlayer>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    if let Some(pos) = local_player_query.iter().next() {
        for mut camera_transform in camera_query.iter_mut() {
            camera_transform.translation.x = pos.x as f32 / 1000.0;
            camera_transform.translation.z = pos.y as f32 / 1000.0;
            camera_transform.translation.y = 72.0; // 90% of 80 unit height (units are mm, but rendering in weird scale)
        }
    }
}

/// Update Transform from Position component for rendering
/// Position is in millimeters, Transform is in meters
pub fn sync_position_to_transform_system(mut query: Query<(&Position, &mut Transform)>) {
    for (pos, mut transform) in query.iter_mut() {
        transform.translation.x = pos.x as f32 / 1000.0; // mm to meters
        transform.translation.z = pos.y as f32 / 1000.0; // mm to meters
    }
}

/// Update player cuboid rotation from stored rotation component
pub fn sync_rotation_to_transform_system(mut query: Query<(&Rotation, &mut Transform), Without<Camera3d>>) {
    for (rot, mut transform) in query.iter_mut() {
        // Always use stored rotation (player faces where camera is looking)
        transform.rotation = Quat::from_rotation_y(rot.yaw);
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

// Spawn a player cuboid at the given position
fn spawn_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_id: u32,
    position: &Position,
    velocity: &Velocity,
    rotation: &Rotation,
    is_local: bool,
) {
    let color = if is_local {
        Color::srgb(0.2, 0.7, 1.0) // Blue for local player
    } else {
        Color::srgb(1.0, 0.3, 0.3) // Red for other players
    };

    // Main body
    let mut entity = commands.spawn((
        PlayerId(player_id),
        *position, // Add Position component
        *velocity, // Add Velocity component
        *rotation, // Add Rotation component
        Mesh3d(meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_DEPTH))),
        MeshMaterial3d(materials.add(color)),
        Transform::from_xyz(
            position.x as f32 / 1000.0, // mm to meters
            PLAYER_HEIGHT / 2.0,
            position.y as f32 / 1000.0, // mm to meters
        ),
        Visibility::default(),
    ));

    if is_local {
        entity.insert(LocalPlayer);
    }

    let entity_id = entity.id();

    // Add a "nose" marker at the front (yellow sphere) as a child
    let front_marker_color = Color::srgb(1.0, 1.0, 0.0); // Yellow
    let marker_id = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(5.0))),
            MeshMaterial3d(materials.add(front_marker_color)),
            Transform::from_xyz(
                0.0,
                10.0,                     // Slightly above center
                PLAYER_DEPTH / 2.0 + 5.0, // Front of the cuboid
            ),
            Visibility::Inherited,
            ViewVisibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    commands.entity(entity_id).add_children(&[marker_id]);
}
