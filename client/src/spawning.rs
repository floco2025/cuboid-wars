use bevy::prelude::*;

use common::{
    systems::Projectile,
    constants::*,
    protocol::{Movement, PlayerId, Position, Wall, WallOrientation},
};
use crate::systems::sync::LocalPlayer;

// ============================================================================
// Entity Spawning
// ============================================================================

// Spawn a player cuboid at the given position
pub fn spawn_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_id: u32,
    position: &Position,
    movement: &Movement,
    is_local: bool,
) -> Entity {
    // Choose color: local player is blue, other players are red
    let color = if is_local {
        Color::srgb(0.3, 0.3, 1.0) // Blue for local player
    } else {
        Color::srgb(1.0, 0.3, 0.3) // Red for other players
    };

    // Set initial visibility: hidden for local player (first-person view), visible for others
    let initial_visibility = if is_local {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };

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
        )
        .with_rotation(Quat::from_rotation_y(movement.face_dir)),
        initial_visibility,
    ));

    let mut entity_cmd = entity;
    
    // Add LocalPlayer marker if this is the local player
    if is_local {
        entity_cmd.insert(LocalPlayer);
    }

    let entity_id = entity_cmd.id();

    // Add a "nose" marker at the front (yellow sphere) as a child
    let front_marker_color = Color::srgb(1.0, 1.0, 0.0); // Yellow
    let nose_id = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(PLAYER_NOSE_RADIUS))),
            MeshMaterial3d(materials.add(front_marker_color)),
            Transform::from_xyz(
                0.0,
                PLAYER_NOSE_HEIGHT,
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
            Mesh3d(meshes.add(Sphere::new(PLAYER_EYE_RADIUS))),
            MeshMaterial3d(materials.add(eye_color)),
            Transform::from_xyz(
                -PLAYER_EYE_SPACING,
                PLAYER_EYE_HEIGHT,
                PLAYER_DEPTH / 2.0, // Center aligned with front face
            ),
            Visibility::Inherited,
            ViewVisibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    let right_eye_id = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(PLAYER_EYE_RADIUS))),
            MeshMaterial3d(materials.add(eye_color)),
            Transform::from_xyz(
                PLAYER_EYE_SPACING,
                PLAYER_EYE_HEIGHT,
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

    entity_id
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
        Mesh3d(meshes.add(Sphere::new(PROJECTILE_RADIUS))),
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
    player_pos_mov_query: &Query<(&Position, &Movement), With<PlayerId>>,
    entity: Entity,
) {
    // Get position and movement for this player entity
    if let Ok((pos, mov)) = player_pos_mov_query.get(entity) {
        spawn_projectile_local(commands, meshes, materials, pos, mov);
    }
}

// Spawn a wall segment
pub fn spawn_wall(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    wall: &Wall,
) {
    let wall_color = Color::srgb(0.0, 0.7, 0.0); // Green

    let (size_x, size_z) = match wall.orientation {
        WallOrientation::Horizontal => (WALL_LENGTH, WALL_WIDTH),
        WallOrientation::Vertical => (WALL_WIDTH, WALL_LENGTH),
    };

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(size_x, WALL_HEIGHT, size_z))),
        MeshMaterial3d(materials.add(wall_color)),
        Transform::from_xyz(
            wall.x,
            WALL_HEIGHT / 2.0, // Lift so bottom is at y=0
            wall.z,
        ),
        Visibility::default(),
    ));
}
