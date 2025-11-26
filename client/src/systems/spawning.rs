use bevy::prelude::*;
use common::{components::Projectile, protocol::{PlayerId, Position, Movement}};

use crate::components::LocalPlayer;

// ============================================================================
// Entity Spawning
// ============================================================================

// Player cuboid dimensions - make it asymmetric so we can see orientation
pub const PLAYER_WIDTH: f32 = 20.0; // side to side
pub const PLAYER_HEIGHT: f32 = 80.0; // up/down
pub const PLAYER_DEPTH: f32 = 40.0; // front to back (longer)

// Spawn a player cuboid at the given position
pub fn spawn_player(
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

// Spawn a projectile locally (for local player shooting)
pub fn spawn_projectile_local(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    pos: &Position,
    mov: &Movement,
) {
    // Calculate spawn position using common helper
    let (spawn_x, spawn_y) = Projectile::calculate_spawn_position(pos.x, pos.y, mov.face_dir);
    
    // Create projectile with common parameters
    let projectile = Projectile::new(mov.face_dir);
    
    let projectile_color = Color::srgb(10.0, 10.0, 0.0); // Very bright yellow
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(5.0))), // 2x size
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
        projectile,
    ));
}

// Spawn a projectile for a player (when receiving shot from server)
pub fn spawn_projectile_for_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_query: &Query<(Entity, &PlayerId)>,
    player_pos_mov_query: &Query<(&Position, &Movement), With<PlayerId>>,
    shooter_id: PlayerId,
) {
    // Find the entity with this player ID
    if let Some((entity, _)) = player_query.iter().find(|(_, id)| **id == shooter_id) {
        // Get position and movement for this player
        if let Ok((pos, mov)) = player_pos_mov_query.get(entity) {
            spawn_projectile_local(commands, meshes, materials, pos, mov);
        }
    }
}
