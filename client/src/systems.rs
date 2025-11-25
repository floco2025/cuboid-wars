use crate::{
    components::LocalPlayer,
    net::ServerToClient,
    resources::{MyPlayerId, ServerToClientChannel},
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
                    &player.pos,
                    false, // Other players are never local
                );
            }

            // Spawn ourselves as the local player with position from server
            spawn_player(
                commands,
                meshes,
                materials,
                init_msg.id.0,
                &init_msg.player.pos,
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
                &login_msg.player.pos,
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
    is_local: bool,
) {
    let color = if is_local {
        Color::srgb(0.2, 0.7, 1.0) // Blue for local player
    } else {
        Color::srgb(1.0, 0.3, 0.3) // Red for other players
    };

    let mut entity = commands.spawn((
        PlayerId(player_id),
        Mesh3d(meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_DEPTH))),
        MeshMaterial3d(materials.add(color)),
        Transform::from_xyz(position.x as f32, PLAYER_HEIGHT / 2.0, position.y as f32),
        Visibility::default(),
    ));

    if is_local {
        entity.insert(LocalPlayer);
    }
}
