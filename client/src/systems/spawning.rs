use bevy::prelude::*;
use common::{
    components::Projectile,
    protocol::{Movement, PlayerId, Position},
};

use crate::components::LocalPlayer;

// ============================================================================
// Entity Spawning
// ============================================================================

// Player cuboid dimensions - make it asymmetric so we can see orientation
pub const PLAYER_WIDTH: f32 = 20.0; // meters - side to side
pub const PLAYER_HEIGHT: f32 = 80.0; // meters - up/down
pub const PLAYER_DEPTH: f32 = 40.0; // meters - front to back (longer)

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
    // For local player, just spawn the entity with components but no mesh
    if is_local {
        commands.spawn((
            PlayerId(player_id),
            *position,
            *movement,
            LocalPlayer,
        ));
        return;
    }

    // For other players, spawn the full visual representation
    let color = Color::srgb(1.0, 0.3, 0.3);

    // Main body
    let entity = commands.spawn((
        PlayerId(player_id),
        *position, // Add Position component
        *movement, // Add Movement component
        Mesh3d(meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_DEPTH))),
        MeshMaterial3d(materials.add(color)),
        Transform::from_xyz(
            position.x,
            PLAYER_HEIGHT / 2.0, // Lift so bottom is at y=0
            position.z,
        ),
        Visibility::default(),
    ));

    let entity_id = entity.id();

    // Add a "nose" marker at the front (yellow sphere) as a child
    let front_marker_color = Color::srgb(1.0, 1.0, 0.0); // Yellow
    let nose_id = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(5.0))),
            MeshMaterial3d(materials.add(front_marker_color)),
            Transform::from_xyz(
                0.0,
                20.0,               // Y is up/down
                PLAYER_DEPTH / 2.0, // Center aligned with front face
            ),
            Visibility::Inherited,
            ViewVisibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    // Add two "eyes" above the nose (white spheres) as children
    let eye_color = Color::srgb(1.0, 1.0, 1.0); // White
    let left_eye_id = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(3.0))),
            MeshMaterial3d(materials.add(eye_color)),
            Transform::from_xyz(
                -6.0,               // Left side
                30.0,               // Y is up/down - above nose
                PLAYER_DEPTH / 2.0, // Center aligned with front face
            ),
            Visibility::Inherited,
            ViewVisibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    let right_eye_id = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(3.0))),
            MeshMaterial3d(materials.add(eye_color)),
            Transform::from_xyz(
                6.0,                // Right side
                30.0,               // Y is up/down - above nose
                PLAYER_DEPTH / 2.0, // Center aligned with front face
            ),
            Visibility::Inherited,
            ViewVisibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    commands
        .entity(entity_id)
        .add_children(&[nose_id, left_eye_id, right_eye_id]);
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
    let spawn_pos = Projectile::calculate_spawn_position(Vec3::new(pos.x, pos.y, pos.z), mov.face_dir);

    // Create projectile with common parameters
    let projectile = Projectile::new(mov.face_dir);

    let projectile_color = Color::srgb(10.0, 10.0, 0.0); // Very bright yellow
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(5.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: projectile_color,
            emissive: LinearRgba::rgb(10.0, 10.0, 0.0), // Make it glow
            ..default()
        })),
        Transform::from_translation(spawn_pos),
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
