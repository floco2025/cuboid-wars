use bevy::prelude::*;

// ============================================================================
// Constants
// ============================================================================

// World dimensions
pub const WORLD_SIZE: f32 = 2000.0; // Total size of the playing field (covers -1000 to +1000)

// Camera settings
pub const CAMERA_X: f32 = 0.0;
pub const CAMERA_Y: f32 = 1500.0;  // Height above ground
pub const CAMERA_Z: f32 = 2000.0;  // Distance back from center

// Player cuboid dimensions
pub const PLAYER_WIDTH: f32 = 20.0;   // X dimension
pub const PLAYER_HEIGHT: f32 = 80.0;  // Y dimension (vertical)
pub const PLAYER_DEPTH: f32 = 20.0;   // Z dimension

// ============================================================================
// Components
// ============================================================================

/// Marker component for player entities
#[derive(Component)]
pub struct PlayerEntity {
    pub player_id: u32,
}

/// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayer;

// ============================================================================
// Setup System
// ============================================================================

pub fn setup_world(
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
        brightness: 300.0,
    });
}

// ============================================================================
// Player Spawning
// ============================================================================

/// Spawn a player cuboid at the given position
pub fn spawn_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_id: u32,
    position: &common::protocol::Position,
    is_local: bool,
) {
    let color = if is_local {
        Color::srgb(0.2, 0.7, 1.0) // Blue for local player
    } else {
        Color::srgb(1.0, 0.3, 0.3) // Red for other players
    };

    let mut entity = commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_DEPTH))),
        MeshMaterial3d(materials.add(color)),
        Transform::from_xyz(position.x as f32, PLAYER_HEIGHT / 2.0, position.y as f32),
        Visibility::default(),
        PlayerEntity { player_id },
    ));

    if is_local {
        entity.insert(LocalPlayer);
    }
}
