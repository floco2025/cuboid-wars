use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

use crate::{
    constants::*,
    systems::{
        players::{BumpFlashState, LocalPlayer},
        walls::{RoofMarker, WallMarker},
    },
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

// Spawn projectile(s) locally based on whether player has multi-shot power-up
pub fn spawn_projectiles_local(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    pos: &Position,
    face_dir: f32,
    has_multi_shot: bool,
) {
    // Determine number of shots
    let num_shots = if has_multi_shot { MULTI_SHOT_MULTIPLER as i32 } else { 1 };

    // Spawn projectiles in an arc
    let angle_step = MULTI_SHOT_ANGLE.to_radians();
    let start_offset = -(num_shots - 1) as f32 * angle_step / 2.0;

    for i in 0..num_shots {
        let angle_offset = start_offset + i as f32 * angle_step;
        let shot_dir = face_dir + angle_offset;
        spawn_single_projectile(commands, meshes, materials, pos, shot_dir);
    }
}

// Internal helper to spawn a single projectile
fn spawn_single_projectile(
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
    has_multi_shot: bool,
) {
    // Get position and face direction for this player entity
    if let Ok((pos, face_dir)) = player_pos_face_query.get(entity) {
        spawn_projectiles_local(commands, meshes, materials, pos, face_dir.0, has_multi_shot);
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
        WallMarker,
    ));
}

// Spawn a roof entity based on a shared `Roof` config.
pub fn spawn_roof(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    roof: &Roof,
) {
    let roof_color = Color::srgb(0.0, 0.7, 0.0); // Same as walls

    // Calculate world position from grid coordinates
    #[allow(clippy::cast_precision_loss)]
    let world_x = (roof.col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
    #[allow(clippy::cast_precision_loss)]
    let world_z = (roof.row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

    // Create a thin horizontal plane at wall height
    // Use same dimensions as walls for consistency
    let roof_size = WALL_LENGTH; // Same overlap as walls to cover corners
    let roof_thickness = WALL_WIDTH; // Same thickness as walls

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(roof_size, roof_thickness, roof_size))),
        MeshMaterial3d(materials.add(roof_color)),
        Transform::from_xyz(
            world_x,
            WALL_HEIGHT - roof_thickness / 2.0, // Position so top of roof aligns with top of wall
            world_z,
        ),
        Visibility::default(),
        RoofMarker,
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

// Get the color for an item type
pub const fn item_type_color(item_type: ItemType) -> Color {
    match item_type {
        ItemType::SpeedPowerUp => Color::srgb(ITEM_SPEED_COLOR[0], ITEM_SPEED_COLOR[1], ITEM_SPEED_COLOR[2]),
        ItemType::MultiShotPowerUp => Color::srgb(
            ITEM_MULTISHOT_COLOR[0],
            ITEM_MULTISHOT_COLOR[1],
            ITEM_MULTISHOT_COLOR[2],
        ),
    }
}

// Component to track item animation timer
#[derive(Component)]
pub struct ItemAnimTimer(pub f32);

// Spawn a item cube
pub fn spawn_item(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    item_id: ItemId,
    item_type: ItemType,
    position: &Position,
) -> Entity {
    let color = item_type_color(item_type);

    let random_phase = rand::random::<f32>() * std::f32::consts::TAU;

    commands
        .spawn((
            item_id,
            ItemAnimTimer(random_phase),
            *position,
            Mesh3d(meshes.add(Cuboid::new(ITEM_SIZE, ITEM_SIZE, ITEM_SIZE))),
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
            Transform::from_xyz(position.x, ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0, position.z),
        ))
        .id()
}

// Spawn a ghost cube
pub fn spawn_ghost(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    ghost_id: GhostId,
    position: &Position,
    velocity: &Velocity,
) -> Entity {
    let color = Color::srgba(GHOST_COLOR[0], GHOST_COLOR[1], GHOST_COLOR[2], GHOST_COLOR[3]);

    commands
        .spawn((
            ghost_id,
            *position,
            *velocity,
            Mesh3d(meshes.add(Cuboid::new(GHOST_SIZE, GHOST_SIZE, GHOST_SIZE))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_xyz(position.x, GHOST_SIZE / 2.0, position.z),
        ))
        .id()
}
