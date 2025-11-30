use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

use crate::{
    constants::*,
    systems::movement::{BumpFlashState, LocalPlayer},
};
use common::{constants::*, protocol::*, systems::Projectile};

#[derive(Component)]
pub struct PlayerIdText;

#[derive(Component)]
pub struct PlayerIdTextMesh;

// Spawn a player cuboid plus cosmetic children, returning the new entity id.
pub fn spawn_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    player_id: u32,
    player_name: &str,
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

    let mut children = vec![nose_id, left_eye_id, right_eye_id];

    // Create individual texture and camera for this player's ID text
    let (image_handle, text_camera) = setup_player_id_text_rendering(commands, images);
    let (_text_entity, mesh_entity) =
        spawn_player_id_display(commands, meshes, materials, player_name, image_handle, text_camera);
    children.push(mesh_entity);

    commands.entity(entity_id).add_children(&children);

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

const fn player_color(is_local: bool) -> Color {
    if is_local {
        Color::srgb(0.3, 0.3, 1.0)
    } else {
        Color::srgb(1.0, 0.3, 0.3)
    }
}

const fn player_visibility(is_local: bool) -> Visibility {
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

// Setup the player ID text rendering system: create texture and camera for a single player
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn setup_player_id_text_rendering(
    commands: &mut Commands,
    images: &mut ResMut<Assets<Image>>,
) -> (Handle<Image>, Entity) {
    let size = Extent3d {
        width: LABEL_TEXTURE_WIDTH,
        height: LABEL_TEXTURE_HEIGHT,
        ..default()
    };

    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[
            (LABEL_BACKGROUND_COLOR[2] * 255.0) as u8, // B
            (LABEL_BACKGROUND_COLOR[1] * 255.0) as u8, // G
            (LABEL_BACKGROUND_COLOR[0] * 255.0) as u8, // R
            (LABEL_BACKGROUND_COLOR[3] * 255.0) as u8, // A
        ],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    let image_handle = images.add(image);

    let text_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: -1,
                target: bevy::camera::RenderTarget::Image(image_handle.clone().into()),
                clear_color: bevy::camera::ClearColorConfig::Custom(Color::srgba(
                    LABEL_BACKGROUND_COLOR[0],
                    LABEL_BACKGROUND_COLOR[1],
                    LABEL_BACKGROUND_COLOR[2],
                    LABEL_BACKGROUND_COLOR[3],
                )),
                ..default()
            },
        ))
        .id();

    (image_handle, text_camera)
}

// Spawn player ID text UI and mesh for a specific player
pub fn spawn_player_id_display(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    player_name: &str,
    image_handle: Handle<Image>,
    text_camera: Entity,
) -> (Entity, Entity) {
    #[allow(clippy::cast_precision_loss)]
    const LABEL_HEIGHT: f32 = LABEL_WIDTH * (LABEL_TEXTURE_HEIGHT as f32 / LABEL_TEXTURE_WIDTH as f32);

    // Create UI text that renders to texture
    let text_entity = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(
                LABEL_BACKGROUND_COLOR[0],
                LABEL_BACKGROUND_COLOR[1],
                LABEL_BACKGROUND_COLOR[2],
                LABEL_BACKGROUND_COLOR[3],
            )),
            UiTargetCamera(text_camera),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(player_name),
                TextFont {
                    font_size: LABEL_FONT_SIZE,
                    ..default()
                },
                TextColor(Color::srgba(
                    LABEL_TEXT_COLOR[0],
                    LABEL_TEXT_COLOR[1],
                    LABEL_TEXT_COLOR[2],
                    LABEL_TEXT_COLOR[3],
                )),
                TextLayout::new_with_no_wrap(),
                PlayerIdText,
            ));
        })
        .id();

    // Create 3D plane mesh with the rendered texture
    let mesh_entity = commands
        .spawn((
            Mesh3d(meshes.add(Rectangle::new(LABEL_WIDTH, LABEL_HEIGHT))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(image_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(
                0.0,
                PLAYER_HEIGHT / 2.0 + LABEL_HEIGHT_ABOVE_PLAYER + LABEL_HEIGHT / 2.0,
                0.0,
            ),
            PlayerIdTextMesh,
        ))
        .id();

    (text_entity, mesh_entity)
}

// Component to mark powerup entities with animation state
#[derive(Component)]
pub struct PowerUpMarker {
    pub power_up_type: PowerUpType,
    pub anim_timer: f32,
}

// Spawn a powerup cube
pub fn spawn_powerup(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    powerup_id: PowerUpId,
    power_up_type: PowerUpType,
    position: &Position,
) -> Entity {
    let color = match power_up_type {
        PowerUpType::Speed => Color::srgba(
            POWERUP_SPEED_COLOR[0],
            POWERUP_SPEED_COLOR[1],
            POWERUP_SPEED_COLOR[2],
            POWERUP_SPEED_COLOR[3],
        ),
        PowerUpType::MultiShot => Color::srgba(
            POWERUP_MULTISHOT_COLOR[0],
            POWERUP_MULTISHOT_COLOR[1],
            POWERUP_MULTISHOT_COLOR[2],
            POWERUP_MULTISHOT_COLOR[3],
        ),
    };

    let random_phase = rand::random::<f32>() * std::f32::consts::TAU;

    commands
        .spawn((
            powerup_id,
            PowerUpMarker { power_up_type, anim_timer: random_phase },
            *position,
            Mesh3d(meshes.add(Cuboid::new(POWERUP_SIZE, POWERUP_SIZE, POWERUP_SIZE))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                emissive: LinearRgba::new(
                    color.to_srgba().red * 0.5,
                    color.to_srgba().green * 0.5,
                    color.to_srgba().blue * 0.5,
                    1.0,
                ),
                ..default()
            })),
            Transform::from_xyz(position.x, POWERUP_HEIGHT_ABOVE_FLOOR + POWERUP_SIZE / 2.0, position.z),
        ))
        .id()
}