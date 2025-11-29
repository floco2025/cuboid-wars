use bevy::prelude::*;

use crate::systems::sync::{BumpFlashState, LocalPlayer};
use common::{
    constants::*,
    protocol::{FaceDirection, PlayerId, Position, Velocity, Wall, WallOrientation},
    systems::Projectile,
};

// Spawn a player cuboid plus cosmetic children, returning the new entity id.
pub fn spawn_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_id: u32,
    position: &Position,
    velocity: Velocity,
    face_dir: f32,
    is_local: bool,
) -> Entity {
    let entity_id = commands
        .spawn((
        PlayerId(player_id),
        *position,               // Add Position component
        velocity,                // Add Velocity component
        FaceDirection(face_dir), // Add FaceDirection component
        Mesh3d(meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_DEPTH))),
        MeshMaterial3d(materials.add(player_color(is_local))),
        Transform::from_xyz(
            position.x,
            PLAYER_HEIGHT / 2.0, // Lift so bottom is at y=0
            position.z,
        )
        .with_rotation(Quat::from_rotation_y(face_dir)),
        player_visibility(is_local),
    ))
        .id();

    if is_local {
        commands
            .entity(entity_id)
            .insert((LocalPlayer, BumpFlashState::default()));
    }

    // Nose and eyes share the same component boilerplate; spawn each and attach.
    let nose_id = spawn_face_sphere(
        commands,
        meshes,
        materials,
        PLAYER_NOSE_RADIUS,
        Color::srgb(1.0, 1.0, 0.0),
        Vec3::new(0.0, PLAYER_NOSE_HEIGHT, PLAYER_DEPTH / 2.0),
    );
    let eye_color = Color::WHITE;
    let left_eye_id = spawn_face_sphere(
        commands,
        meshes,
        materials,
        PLAYER_EYE_RADIUS,
        eye_color,
        Vec3::new(-PLAYER_EYE_SPACING, PLAYER_EYE_HEIGHT, PLAYER_DEPTH / 2.0),
    );
    let right_eye_id = spawn_face_sphere(
        commands,
        meshes,
        materials,
        PLAYER_EYE_RADIUS,
        eye_color,
        Vec3::new(PLAYER_EYE_SPACING, PLAYER_EYE_HEIGHT, PLAYER_DEPTH / 2.0),
    );

    commands
        .entity(entity_id)
        .add_children(&[nose_id, left_eye_id, right_eye_id]);

    entity_id
}

// Spawn a projectile locally (for local player shooting).
pub fn spawn_projectile_local(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    pos: &Position,
    face_dir: f32,
) {
    let spawn_pos = Projectile::calculate_spawn_position(Vec3::new(pos.x, pos.y, pos.z), face_dir);
    let projectile = Projectile::new(face_dir);
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

// Spawn a projectile for a player (when receiving shot from server).
pub fn spawn_projectile_for_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_pos_face_query: &Query<(&Position, &FaceDirection), With<PlayerId>>,
    entity: Entity,
) {
    // Get position and face direction for this player entity
    if let Ok((pos, face_dir)) = player_pos_face_query.get(entity) {
        spawn_projectile_local(commands, meshes, materials, pos, face_dir.0);
    }
}

// Spawn a wall segment entity based on a shared `Wall` config.
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

fn player_color(is_local: bool) -> Color {
    if is_local {
        Color::srgb(0.3, 0.3, 1.0)
    } else {
        Color::srgb(1.0, 0.3, 0.3)
    }
}

fn player_visibility(is_local: bool) -> Visibility {
    if is_local {
        Visibility::Hidden
    } else {
        Visibility::Visible
    }
}

fn spawn_face_sphere(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    radius: f32,
    color: Color,
    translation: Vec3,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(radius))),
            MeshMaterial3d(materials.add(color)),
            Transform::from_translation(translation),
            Visibility::Inherited,
            ViewVisibility::default(),
            InheritedVisibility::default(),
        ))
        .id()
}
