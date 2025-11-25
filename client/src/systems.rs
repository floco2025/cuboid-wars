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

// Player cuboid dimensions
pub const PLAYER_WIDTH: f32 = 20.0;
pub const PLAYER_HEIGHT: f32 = 80.0;
pub const PLAYER_DEPTH: f32 = 20.0;

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
// Server Message Processing System
// ============================================================================

// System to process messages from the server
pub fn process_server_messages_system(
    mut commands: Commands,
    mut from_server: ResMut<ServerToClientChannel>,
    mut exit: MessageWriter<AppExit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player_query: Query<(Entity, &PlayerId)>,
) {
    // Process all messages from the server
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToClient::Disconnected => {
                error!("disconnected from server");
                exit.write(AppExit::Success);
            }
            ServerToClient::Message(message) => {
                process_message(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &player_query,
                    &message,
                );
            }
        }
    }
}

// ============================================================================
// Message Handler
// ============================================================================

fn process_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_query: &Query<(Entity, &PlayerId)>,
    msg: &ServerMessage,
) {
    match msg {
        ServerMessage::Init(init_msg) => {
            info!(
                "received Init: my_id={:?}, {} existing players",
                init_msg.id,
                init_msg.other_players.len()
            );

            // Insert MyPlayerId resource
            commands.insert_resource(MyPlayerId(init_msg.id));

            // Spawn all existing players (these are other players, not us)
            for (id, player) in &init_msg.other_players {
                spawn_player(
                    commands,
                    meshes,
                    materials,
                    id.0,
                    &player.kin.pos,
                    &player.kin.vel,
                    false, // Other players are never local
                );
            }

            // Spawn ourselves as the local player with position from server
            spawn_player(
                commands,
                meshes,
                materials,
                init_msg.id.0,
                &init_msg.player.kin.pos,
                &init_msg.player.kin.vel,
                true, // This is us!
            );
        }
        ServerMessage::Login(login_msg) => {
            info!("player {:?} logged in", login_msg.id);

            // Login is always for another player (server doesn't send our own login back)
            spawn_player(
                commands,
                meshes,
                materials,
                login_msg.id.0,
                &login_msg.player.kin.pos,
                &login_msg.player.kin.vel,
                false, // Never local
            );
        }
        ServerMessage::Logoff(logoff_msg) => {
            info!(
                "player {:?} logged off (graceful: {})",
                logoff_msg.id, logoff_msg.graceful
            );

            // Find and despawn the entity with this PlayerId
            for (entity, player_id) in player_query.iter() {
                if *player_id == logoff_msg.id {
                    commands.entity(entity).despawn();
                    break;
                }
            }
        }
        ServerMessage::PlayerVelocity(vel_msg) => {
            // Update player velocity (both local and remote)
            for (entity, player_id) in player_query.iter() {
                if *player_id == vel_msg.id {
                    commands.entity(entity).insert(vel_msg.vel);
                    break;
                }
            }
        }
        ServerMessage::Kinematics(kin_msg) => {
            // Server authoritative kinematics updates for all players
            for (id, kin) in &kin_msg.kinematics {
                for (entity, player_id) in player_query.iter() {
                    if *player_id == *id {
                        commands.entity(entity).insert((kin.pos, kin.vel));
                        break;
                    }
                }
            }
        }
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
    is_local: bool,
) {
    let color = if is_local {
        Color::srgb(0.2, 0.7, 1.0) // Blue for local player
    } else {
        Color::srgb(1.0, 0.3, 0.3) // Red for other players
    };

    let mut entity = commands.spawn((
        PlayerId(player_id),
        *position, // Add Position component
        *velocity, // Add Velocity component
        Mesh3d(meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_DEPTH))),
        MeshMaterial3d(materials.add(color)),
        Transform::from_xyz(
            position.x as f32 / 1000.0,  // mm to meters
            PLAYER_HEIGHT / 2.0,
            position.y as f32 / 1000.0   // mm to meters
        ),
        Visibility::default(),
    ));

    if is_local {
        entity.insert(LocalPlayer);
    }
}

// ============================================================================
// Input System
// ============================================================================

/// Handle WASD input and send velocity updates to server
pub fn input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    to_server: Res<ClientToServerChannel>,
    mut last_velocity: Local<(f32, f32)>,
    mut local_player_query: Query<&mut Velocity, With<LocalPlayer>>,
) {
    const SPEED: f32 = 100_000.0; // 100,000 mm/sec = 100 meters/sec
    
    let mut dx = 0.0_f32;
    let mut dy = 0.0_f32;

    if keyboard.pressed(KeyCode::KeyW) {
        dy += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        dy -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        dx -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        dx += 1.0;
    }

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

    // Only send and update if velocity changed
    if vel.x != last_velocity.0 || vel.y != last_velocity.1 {
        // Update local player's velocity immediately
        for mut player_vel in local_player_query.iter_mut() {
            *player_vel = vel;
        }
        
        // Send to server
        let msg = ClientMessage::Velocity(CVelocity { vel });
        let _ = to_server.send(ClientToServer::Send(msg));
        *last_velocity = (vel.x, vel.y);
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
