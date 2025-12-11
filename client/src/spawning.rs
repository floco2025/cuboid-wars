use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

use crate::{
    constants::*,
    systems::{
        map::{RoofMarker, WallMarker},
        players::{BumpFlashState, LocalPlayer},
    },
};
use common::{collision::Projectile, constants::*, protocol::*, spawning::calculate_projectile_spawns};

// ============================================================================
// Components
// ============================================================================

#[derive(Component)]
pub struct PlayerIdText;

#[derive(Component)]
pub struct PlayerIdTextMesh;

#[derive(Component)]
pub struct ItemAnimTimer(pub f32);

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct PlayerBundle {
    player_id: PlayerId,
    position: Position,
    velocity: Velocity,
    face_direction: FaceDirection,
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
}

#[derive(Bundle)]
struct FaceSphereBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    view_visibility: ViewVisibility,
    inherited_visibility: InheritedVisibility,
}

#[derive(Bundle)]
struct ItemBundle {
    item_id: ItemId,
    position: Position,
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
}

#[derive(Bundle)]
struct GhostBundle {
    ghost_id: GhostId,
    position: Position,
    velocity: Velocity,
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
}

#[derive(Bundle)]
struct ProjectileBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    projectile: Projectile,
}

impl ProjectileBundle {
    fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        position: Vec3,
        direction: f32,
        reflects: bool,
    ) -> Self {
        Self {
            mesh: Mesh3d(meshes.add(Sphere::new(PROJECTILE_RADIUS))),
            material: MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(10.0, 10.0, 0.0),
                emissive: LinearRgba::rgb(10.0, 10.0, 0.0),
                ..default()
            })),
            transform: Transform::from_translation(position),
            projectile: Projectile::new(direction, reflects),
        }
    }
}

#[derive(Bundle)]
struct WallBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: WallMarker,
}

#[derive(Bundle)]
struct RoofBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: RoofMarker,
}

// ============================================================================
// Player Spawning
// ============================================================================

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
    let entity = commands
        .spawn(PlayerBundle {
            player_id: PlayerId(player_id),
            position: *position,
            velocity,
            face_direction: FaceDirection(face_dir),
            mesh: Mesh3d(meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_DEPTH))),
            material: MeshMaterial3d(materials.add(player_color(is_local))),
            transform: Transform::from_xyz(position.x, PLAYER_HEIGHT / 2.0, position.z)
                .with_rotation(Quat::from_rotation_y(face_dir)),
            visibility: player_visibility(is_local),
        })
        .id();

    if is_local {
        commands.entity(entity).insert((LocalPlayer, BumpFlashState::default()));
    }

    // Nose and eyes share the same component boilerplate; spawn each and attach.
    let nose = spawn_face_sphere(
        commands,
        meshes,
        materials,
        PLAYER_NOSE_RADIUS,
        Color::srgb(1.0, 1.0, 0.0),
        Vec3::new(0.0, PLAYER_NOSE_HEIGHT, PLAYER_DEPTH / 2.0),
    );
    let eye_color = Color::WHITE;
    let left_eye = spawn_face_sphere(
        commands,
        meshes,
        materials,
        PLAYER_EYE_RADIUS,
        eye_color,
        Vec3::new(-PLAYER_EYE_SPACING, PLAYER_EYE_HEIGHT, PLAYER_DEPTH / 2.0),
    );
    let right_eye = spawn_face_sphere(
        commands,
        meshes,
        materials,
        PLAYER_EYE_RADIUS,
        eye_color,
        Vec3::new(PLAYER_EYE_SPACING, PLAYER_EYE_HEIGHT, PLAYER_DEPTH / 2.0),
    );

    let mut children = vec![nose, left_eye, right_eye];

    // Create individual texture and camera for this player's ID text
    let (image_handle, text_camera) = setup_player_id_text_rendering(commands, images);
    let (_text_entity, mesh_entity) =
        spawn_player_id_display(commands, meshes, materials, player_name, image_handle, text_camera);
    children.push(mesh_entity);

    commands.entity(entity).add_children(&children);

    entity
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
        .spawn(FaceSphereBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(radius))),
            material: MeshMaterial3d(materials.add(color)),
            transform: Transform::from_translation(translation),
            visibility: Visibility::Inherited,
            view_visibility: ViewVisibility::default(),
            inherited_visibility: InheritedVisibility::default(),
        })
        .id()
}

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

// ============================================================================
// Projectile Spawning
// ============================================================================

// Spawn projectile(s) on whether player has multi-shot power-up
pub fn spawn_projectiles(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    pos: &Position,
    face_dir: f32,
    has_multi_shot: bool,
    has_reflect: bool,
    walls: &[Wall],
) {
    let spawns = calculate_projectile_spawns(pos, face_dir, has_multi_shot, has_reflect, walls);

    for spawn_info in spawns {
        spawn_single_projectile(commands, meshes, materials, &spawn_info);
    }
}

// Internal helper to spawn a single projectile
fn spawn_single_projectile(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    spawn_info: &common::spawning::ProjectileSpawnInfo,
) {
    let spawn_pos = Vec3::new(spawn_info.position.x, spawn_info.position.y, spawn_info.position.z);

    commands.spawn(ProjectileBundle::new(
        meshes,
        materials,
        spawn_pos,
        spawn_info.direction,
        spawn_info.reflects,
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
    has_reflect: bool,
    walls: &[Wall],
) {
    // Get position and face direction for this player entity
    if let Ok((pos, face_dir)) = player_pos_face_query.get(entity) {
        spawn_projectiles(
            commands,
            meshes,
            materials,
            pos,
            face_dir.0,
            has_multi_shot,
            has_reflect,
            walls,
        );
    }
}

// ============================================================================
// Map Spawning
// ============================================================================

// Spawn a wall segment entity based on a shared `Wall` config.
pub fn spawn_wall(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    wall: &Wall,
) {
    let wall_color = Color::srgb(0.0, 0.7, 0.0); // Green

    // Calculate wall center and dimensions from corners
    let center_x = (wall.x1 + wall.x2) / 2.0;
    let center_z = (wall.z1 + wall.z2) / 2.0;
    
    let dx = wall.x2 - wall.x1;
    let dz = wall.z2 - wall.z1;
    let length = dx.hypot(dz);
    
    // Determine if wall is more horizontal or vertical for mesh sizing
    let (size_x, size_z) = if dx.abs() > dz.abs() {
        (length, wall.wall_width)
    } else {
        (wall.wall_width, length)
    };

    commands.spawn(WallBundle {
        mesh: Mesh3d(meshes.add(Cuboid::new(size_x, WALL_HEIGHT, size_z))),
        material: MeshMaterial3d(materials.add(wall_color)),
        transform: Transform::from_xyz(
            center_x,
            WALL_HEIGHT / 2.0, // Lift so bottom is at y=0
            center_z,
        ),
        visibility: Visibility::default(),
        marker: WallMarker,
    });
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

    commands.spawn(RoofBundle {
        mesh: Mesh3d(meshes.add(Cuboid::new(roof_size, roof_thickness, roof_size))),
        material: MeshMaterial3d(materials.add(roof_color)),
        transform: Transform::from_xyz(
            world_x,
            WALL_HEIGHT - roof_thickness / 2.0, // Position so top of roof aligns with top of wall
            world_z,
        ),
        visibility: Visibility::Visible,
        marker: RoofMarker,
    });
}

// ============================================================================
// Item Spawning
// ============================================================================

// Get the color for an item type
#[must_use]
pub const fn item_type_color(item_type: ItemType) -> Color {
    match item_type {
        ItemType::SpeedPowerUp => Color::srgb(ITEM_SPEED_COLOR[0], ITEM_SPEED_COLOR[1], ITEM_SPEED_COLOR[2]),
        ItemType::MultiShotPowerUp => Color::srgb(
            ITEM_MULTISHOT_COLOR[0],
            ITEM_MULTISHOT_COLOR[1],
            ITEM_MULTISHOT_COLOR[2],
        ),
        ItemType::ReflectPowerUp => Color::srgb(ITEM_REFLECT_COLOR[0], ITEM_REFLECT_COLOR[1], ITEM_REFLECT_COLOR[2]),
        ItemType::Cookie => Color::srgb(COOKIE_COLOR[0], COOKIE_COLOR[1], COOKIE_COLOR[2]),
    }
}

// Spawn an item cube
pub fn spawn_item(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    item_id: ItemId,
    item_type: ItemType,
    position: &Position,
) -> Entity {
    let color = item_type_color(item_type);

    // Cookies are rendered differently - small spheres on the floor
    if item_type == ItemType::Cookie {
        return commands
            .spawn(ItemBundle {
                item_id,
                position: *position,
                mesh: Mesh3d(meshes.add(Sphere::new(COOKIE_SIZE))),
                material: MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    emissive: LinearRgba::new(
                        color.to_srgba().red * 0.3,
                        color.to_srgba().green * 0.3,
                        color.to_srgba().blue * 0.3,
                        1.0,
                    ),
                    ..default()
                })),
                transform: Transform::from_xyz(position.x, COOKIE_HEIGHT, position.z),
            })
            .id();
    }

    // Power-ups are cubes that bounce
    let random_phase = rand::random::<f32>() * std::f32::consts::TAU;

    commands
        .spawn((
            ItemBundle {
                item_id,
                position: *position,
                mesh: Mesh3d(meshes.add(Cuboid::new(ITEM_SIZE, ITEM_SIZE, ITEM_SIZE))),
                material: MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    emissive: LinearRgba::new(
                        color.to_srgba().red * 0.5,
                        color.to_srgba().green * 0.5,
                        color.to_srgba().blue * 0.5,
                        1.0,
                    ),
                    ..default()
                })),
                transform: Transform::from_xyz(position.x, ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0, position.z),
            },
            ItemAnimTimer(random_phase),
        ))
        .id()
}

// ============================================================================
// Ghost Spawning
// ============================================================================

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
        .spawn(GhostBundle {
            ghost_id,
            position: *position,
            velocity: *velocity,
            mesh: Mesh3d(meshes.add(Cuboid::new(GHOST_SIZE, GHOST_SIZE, GHOST_SIZE))),
            material: MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            transform: Transform::from_xyz(position.x, GHOST_SIZE / 2.0, position.z),
        })
        .id()
}
