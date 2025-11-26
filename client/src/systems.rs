use crate::{
    components::LocalPlayer,
    net::{ClientToServer, ServerToClient},
    resources::{ClientToServerChannel, MyPlayerId, ServerToClientChannel},
};
use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use common::{components::Projectile, protocol::*};

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

// Toggle cursor lock with Escape key or mouse click
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
    // Don't consume the click - let it pass through to shooting system
    if mouse.just_pressed(bevy::input::mouse::MouseButton::Left) && cursor_options.visible {
        cursor_options.visible = false;
        cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
        // Note: The click event will still be available for the shooting system
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

    // Create player list UI
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                top: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                ..default()
            },
            PlayerListUI,
        ));

    // Create crosshair UI (initially hidden, shown when local player exists)
    let crosshair_size = 20.0;
    let crosshair_thickness = 2.0;
    let crosshair_color = Color::srgba(1.0, 1.0, 1.0, 0.8);
    
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(crosshair_size * 2.0),
                height: Val::Px(crosshair_size * 2.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
            CrosshairUI,
        ))
        .with_children(|parent| {
            // Horizontal line
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(crosshair_size),
                    height: Val::Px(crosshair_thickness),
                    ..default()
                },
                BackgroundColor(crosshair_color),
            ));
            // Vertical line
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(crosshair_thickness),
                    height: Val::Px(crosshair_size),
                    ..default()
                },
                BackgroundColor(crosshair_color),
            ));
        });
}

// Marker component for the player list UI
#[derive(Component)]
pub struct PlayerListUI;

// Marker component for individual player entries
#[derive(Component)]
pub struct PlayerEntryUI(pub PlayerId);

// Marker component for the crosshair UI
#[derive(Component)]
pub struct CrosshairUI;

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
                &msg.player.pos,
                &msg.player.mov,
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
        ServerMessage::Movement(msg) => {
            trace!("{:?} movement: {:?}", msg.id, msg);
            // Update player movement (both local and remote)
            let mut found = false;
            for (entity, player_id) in player_query.iter() {
                if *player_id == msg.id {
                    commands.entity(entity).insert(msg.mov);
                    found = true;
                    break;
                }
            }
            if !found {
                warn!("received movement for non-existent player {:?}", msg.id);
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
                    spawn_player(commands, meshes, materials, id.0, &player.pos, &player.mov, is_local);
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
                        commands.entity(entity).insert((player.pos, player.mov));
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

// Handle WASD movement and mouse rotation for first-person view
pub fn input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    to_server: Res<ClientToServerChannel>,
    time: Res<Time>,
    mut last_sent_movement: Local<Movement>, // Last movement sent to server
    mut last_send_time: Local<f32>,          // Time accumulator for send interval throttling
    mut local_player_query: Query<&mut Movement, With<LocalPlayer>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    const MOUSE_SENSITIVITY: f32 = 0.002; // radians per pixel
    const MOVEMENT_SEND_INTERVAL: f32 = 0.1; // Send movement updates at most every 100ms
    const ROTATION_CHANGE_THRESHOLD: f32 = 0.05; // ~3 degrees

    // Only process input when cursor is locked
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;

    if cursor_locked {
        // Get current camera rotation
        let mut camera_rotation = 0.0_f32;
        for transform in camera_query.iter() {
            camera_rotation = transform.rotation.to_euler(EulerRot::YXZ).0;
        }

        // Handle mouse rotation
        for motion in mouse_motion.read() {
            camera_rotation -= motion.delta.x * MOUSE_SENSITIVITY;
        }

        // Get forward/right vectors from camera rotation
        // These convert camera-relative directions to world Position coordinates (x, y)
        let forward_x = -camera_rotation.sin();
        let forward_y = -camera_rotation.cos();
        // Right is 90 degrees clockwise from forward
        let right_x = -forward_y;
        let right_y = forward_x;

        // Handle WASD input relative to camera direction
        let mut forward = 0.0_f32;
        let mut right = 0.0_f32;

        if keyboard.pressed(KeyCode::KeyW) {
            forward += 1.0; // Move forward
        }
        if keyboard.pressed(KeyCode::KeyS) {
            forward -= 1.0; // Move backward
        }
        if keyboard.pressed(KeyCode::KeyA) {
            right -= 1.0; // Move left
        }
        if keyboard.pressed(KeyCode::KeyD) {
            right += 1.0; // Move right
        }

        // Calculate world-space direction
        let dx = forward * forward_x + right * right_x;
        let dy = forward * forward_y + right * right_y;

        // Normalize and determine velocity state
        let len = (dx * dx + dy * dy).sqrt();

        let (vel_state, move_direction) = if len > 0.0 {
            // Moving - calculate movement direction from WASD input
            // Convert from world dx/dy to angle in our coordinate system
            let move_dir = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
            // Check if shift is pressed for running
            let vel = if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                Velocity::Run
            } else {
                Velocity::Walk
            };
            (vel, move_dir)
        } else {
            // Idle - movement direction doesn't matter
            (Velocity::Idle, 0.0)
        };

        // Player faces camera direction
        // Camera rotation maps directly to face direction
        let face_direction = camera_rotation;

        // Create movement
        let mov = Movement {
            vel: vel_state,
            move_dir: move_direction,
            face_dir: face_direction,
        };

        // Always update local player's movement immediately for responsive local movement
        for mut player_mov in local_player_query.iter_mut() {
            *player_mov = mov;
        }

        // Accumulate send time for throttling
        *last_send_time += time.delta_secs();

        // Determine if movement changed significantly
        let vel_state_changed = last_sent_movement.vel != mov.vel;
        let rotation_changed = (mov.face_dir - last_sent_movement.face_dir).abs() > ROTATION_CHANGE_THRESHOLD;

        // Send to server if:
        // 1. Velocity state changed (started/stopped moving, or changed speed), OR
        // 2. Direction changed significantly AND enough time has passed (throttle minor direction updates)
        let should_send = vel_state_changed || (rotation_changed && *last_send_time >= MOVEMENT_SEND_INTERVAL);

        if should_send {
            let msg = ClientMessage::Movement(CMovement { mov });
            let _ = to_server.send(ClientToServer::Send(msg));
            *last_sent_movement = mov;
            *last_send_time = 0.0;
        }

        // Update camera rotation
        for mut transform in camera_query.iter_mut() {
            transform.rotation = Quat::from_rotation_y(camera_rotation);
        }
    } else {
        // Cursor not locked - clear mouse motion events to prevent them from accumulating
        for _ in mouse_motion.read() {}

        // Get current camera rotation for setting idle direction
        let mut camera_rotation = 0.0_f32;
        for transform in camera_query.iter() {
            camera_rotation = transform.rotation.to_euler(EulerRot::YXZ).0;
        }

        // Stop player movement when cursor is unlocked
        if last_sent_movement.vel != Velocity::Idle {
            let mov = Movement {
                vel: Velocity::Idle,
                move_dir: 0.0,
                face_dir: camera_rotation,
            };
            for mut player_mov in local_player_query.iter_mut() {
                *player_mov = mov;
            }
            let msg = ClientMessage::Movement(CMovement { mov });
            let _ = to_server.send(ClientToServer::Send(msg));
            *last_sent_movement = mov;
            *last_send_time = 0.0;
        }
    }
}

// ============================================================================
// Shooting System
// ============================================================================

pub fn shooting_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    local_player_query: Query<(&Position, &Movement), With<LocalPlayer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Only allow shooting when cursor is locked
    let cursor_locked = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;
    
    if cursor_locked && mouse.just_pressed(bevy::input::mouse::MouseButton::Left) {
        if let Some((pos, mov)) = local_player_query.iter().next() {
            // Calculate spawn position in front of player using face_dir
            // Same formula as movement forward direction
            let spawn_offset = 50.0; // mm in front of player
            let spawn_x = pos.x as f32 + (-mov.face_dir.sin()) * spawn_offset;
            let spawn_y = pos.y as f32 + (-mov.face_dir.cos()) * spawn_offset;
            
            // Spawn projectile
            let projectile_speed = 2000.0; // meters per second (10x faster)
            // Velocity uses same forward direction formula as movement
            // forward_x = -sin(face_dir), forward_y = -cos(face_dir) in Position coords
            // Then convert: Transform velocity = (forward_x, 0, forward_y)
            let velocity = Vec3::new(
                -mov.face_dir.sin() * projectile_speed,
                0.0,
                -mov.face_dir.cos() * projectile_speed,
            );
            
            let projectile_color = Color::srgb(10.0, 10.0, 0.0); // Very bright yellow
            commands.spawn((
                Mesh3d(meshes.add(Sphere::new(2.5))), // 25% of original 10 units
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: projectile_color,
                    emissive: LinearRgba::rgb(10.0, 10.0, 0.0), // Make it glow
                    ..default()
                })),
                Transform::from_xyz(
                    spawn_x / 1000.0,
                    PLAYER_HEIGHT / 2.0,
                    spawn_y / 1000.0,
                ),
                Projectile {
                    velocity,
                    lifetime: Timer::from_seconds(3.0, TimerMode::Once),
                },
            ));
        }
    }
}

// Update projectiles
pub fn update_shooting_effects_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Transform, &mut Projectile)>,
) {
    // Update projectiles
    for (entity, mut transform, mut projectile) in projectile_query.iter_mut() {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(entity).despawn();
        } else {
            transform.translation += projectile.velocity * time.delta_secs();
        }
    }
}

// Update camera position to follow local player
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

// Update Transform from Position component for rendering
// Position is in millimeters, Transform is in meters
pub fn sync_position_to_transform_system(mut query: Query<(&Position, &mut Transform)>) {
    for (pos, mut transform) in query.iter_mut() {
        transform.translation.x = pos.x as f32 / 1000.0; // mm to meters
        transform.translation.z = pos.y as f32 / 1000.0; // mm to meters
    }
}

// Update player cuboid rotation from stored movement component
pub fn sync_rotation_to_transform_system(mut query: Query<(&Movement, &mut Transform), Without<Camera3d>>) {
    for (mov, mut transform) in query.iter_mut() {
        // Face direction uses same convention as movement: 0 = facing -Y direction
        // Add π to flip the model 180° so nose points in the right direction
        transform.rotation = Quat::from_rotation_y(mov.face_dir + std::f32::consts::PI);
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
    movement: &Movement,
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
        *movement, // Add Movement component
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

// ============================================================================
// Player List UI System
// ============================================================================

// Update player list UI with all players and their hit counts
pub fn update_player_list_system(
    mut commands: Commands,
    player_query: Query<&PlayerId>,
    local_player_query: Query<&PlayerId, With<LocalPlayer>>,
    player_list_ui: Single<Entity, With<PlayerListUI>>,
    existing_entries: Query<(Entity, &PlayerEntryUI)>,
) {
    // Get the latest player data from server messages
    let mut player_data: std::collections::HashMap<PlayerId, i32> = std::collections::HashMap::new();
    
    // We'll need to track this separately - for now just initialize everyone to 0
    // In the future, we'll update this when we receive player updates
    for player_id in player_query.iter() {
        player_data.insert(*player_id, 0);
    }
    
    // Get local player ID if it exists
    let local_player_id = local_player_query.iter().next().copied();
    
    // Remove all existing entries - we'll rebuild the list in sorted order
    for (entity, _) in existing_entries.iter() {
        commands.entity(entity).despawn();
    }
    
    // Collect and sort players by ID
    let mut sorted_players: Vec<(PlayerId, i32)> = player_data.iter()
        .map(|(id, hits)| (*id, *hits))
        .collect();
    sorted_players.sort_by_key(|(id, _)| id.0);
    
    // Create entries for all players in sorted order
    for (player_id, hits) in &sorted_players {
        let player_num = player_id.0;
        let is_local = local_player_id == Some(*player_id);
        
        let hit_color = if *hits > 0 {
            Color::srgb(0.3, 0.6, 1.0) // Blue for positive
        } else if *hits < 0 {
            Color::srgb(1.0, 0.3, 0.3) // Red for negative
        } else {
            Color::srgb(0.8, 0.8, 0.8) // Gray for zero
        };
        
        let sign = if *hits >= 0 { "+" } else { "" };
        
        // Highlight local player with yellow background
        let background_color = if is_local {
            BackgroundColor(Color::srgba(0.8, 0.8, 0.0, 0.3))
        } else {
            BackgroundColor(Color::NONE)
        };
        
        commands.entity(*player_list_ui).with_children(|parent| {
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(5.0)),
                    ..default()
                },
                background_color,
                PlayerEntryUI(*player_id),
            )).with_children(|row| {
                // Player name
                row.spawn((
                    Text::new(format!("Player {}", player_num)),
                    TextFont {
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
                
                // Hit counter
                row.spawn((
                    Text::new(format!("{}{}", sign, hits)),
                    TextFont {
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(hit_color),
                ));
            });
        });
    }
}

// ============================================================================
// Crosshair Visibility System
// ============================================================================

// Show crosshair only when local player exists
pub fn toggle_crosshair_system(
    local_player_query: Query<(), With<LocalPlayer>>,
    crosshair_query: Single<&mut Visibility, With<CrosshairUI>>,
) {
    *crosshair_query.into_inner() = if local_player_query.iter().count() > 0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}
