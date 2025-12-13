use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat, TextureUsages},
};

use crate::{
    constants::*,
    systems::{
        map::{RoofMarker, WallMarker},
        players::{BumpFlashState, LocalPlayer},
    },
};
use common::{
    collision::Projectile,
    constants::*,
    protocol::*,
    spawning::{ProjectileSpawnInfo, calculate_projectile_spawns},
};

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
// Mesh Helpers
// ============================================================================

// Build a cuboid mesh with UVs that tile based on a single tile size.
// Maps U to X extent on ±X faces, and to Z extent on ±Z faces; V maps to Y on side faces.
fn tiled_cuboid(size_x: f32, size_y: f32, size_z: f32, tile_size: f32) -> Mesh {
    let hx = size_x / 2.0;
    let hy = size_y / 2.0;
    let hz = size_z / 2.0;

    let repeat_x = size_x / tile_size;
    let repeat_y = size_y / tile_size;
    let repeat_z = size_z / tile_size;

    let mut positions = Vec::with_capacity(36);
    let mut normals = Vec::with_capacity(36);
    let mut uvs = Vec::with_capacity(36);

    // Helper to push two triangles (quad) given four corner positions (p0..p3) in CCW order.
    let mut push_face = |
        p0: [f32; 3],
        p1: [f32; 3],
        p2: [f32; 3],
        p3: [f32; 3],
        normal: [f32; 3],
        uv00: [f32; 2],
        uv11: [f32; 2],
    | {
        // Triangle 1: p0 (uv00), p1 (u max), p2 (u,v max)
        positions.extend_from_slice(&[p0, p1, p2]);
        normals.extend_from_slice(&[normal; 3]);
        uvs.extend_from_slice(&[
            [uv00[0], uv00[1]],
            [uv11[0], uv00[1]],
            [uv11[0], uv11[1]],
        ]);

        // Triangle 2: p0 (uv00), p2 (u,v max), p3 (v max)
        positions.extend_from_slice(&[p0, p2, p3]);
        normals.extend_from_slice(&[normal; 3]);
        uvs.extend_from_slice(&[
            [uv00[0], uv00[1]],
            [uv11[0], uv11[1]],
            [uv00[0], uv11[1]],
        ]);
    };

    // +X face (U along height to keep scale; V clamped across thin thickness)
    push_face(
        [hx, -hy, -hz],
        [hx, hy, -hz],
        [hx, hy, hz],
        [hx, -hy, hz],
        [1.0, 0.0, 0.0],
        [0.0, 0.0],
        [repeat_y, 1.0],
    );

    // -X face
    push_face(
        [-hx, -hy, hz],
        [-hx, hy, hz],
        [-hx, hy, -hz],
        [-hx, -hy, -hz],
        [-1.0, 0.0, 0.0],
        [0.0, 0.0],
        [repeat_y, 1.0],
    );

    // +Y face (u along X, v along Z)
    push_face(
        [-hx, hy, -hz],  // p0: X min, Z min
        [-hx, hy, hz],   // p1: X min, Z max (Z increases → V increases)
        [hx, hy, hz],    // p2: X max, Z max
        [hx, hy, -hz],   // p3: X max, Z min (X increases from p1→p2)
        [0.0, 1.0, 0.0],
        [0.0, 0.0],
        [repeat_z, repeat_x],  // Swapped: U along Z, V along X
    );

    // -Y face (u along X, v along Z)
    push_face(
        [-hx, -hy, hz],   // p0: X min, Z max
        [-hx, -hy, -hz],  // p1: X min, Z min (Z decreases → V increases)
        [hx, -hy, -hz],   // p2: X max, Z min
        [hx, -hy, hz],    // p3: X max, Z max (X increases from p1→p2)
        [0.0, -1.0, 0.0],
        [0.0, 0.0],
        [repeat_z, repeat_x],  // Swapped: U along Z, V along X
    );

    // +Z face (u along length X, v along Y) - main face
    push_face(
        [-hx, -hy, hz],
        [hx, -hy, hz],
        [hx, hy, hz],
        [-hx, hy, hz],
        [0.0, 0.0, 1.0],
        [0.0, 0.0],
        [repeat_x, repeat_y],
    );

    // -Z face
    push_face(
        [hx, -hy, -hz],
        [-hx, -hy, -hz],
        [-hx, hy, -hz],
        [hx, hy, -hz],
        [0.0, 0.0, -1.0],
        [0.0, 0.0],
        [repeat_x, repeat_y],
    );

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
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
    spawn_info: &ProjectileSpawnInfo,
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

// Load a texture with repeat addressing so UVs beyond 0..1 tile instead of clamping.
fn load_repeating_texture(asset_server: &AssetServer, path: &'static str) -> Handle<Image> {
    asset_server.load_with_settings(path, |settings: &mut ImageLoaderSettings| {
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            ..default()
        });
    })
}

// Spawn a wall segment entity based on a shared `Wall` config.
pub fn spawn_wall(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    wall: &Wall,
) {
    use rand::Rng;

    // Calculate wall center and dimensions from corners
    let center_x = f32::midpoint(wall.x1, wall.x2);
    let center_z = f32::midpoint(wall.z1, wall.z2);

    let dx = wall.x2 - wall.x1;
    let dz = wall.z2 - wall.z1;
    let length = dx.hypot(dz);

    // Put length on local X (visible faces will be the ±Z quads after rotation), width on Z is thickness.
    let mesh_size_x = length;
    let mesh_size_z = wall.width;
    let rotation = Quat::from_rotation_y(dz.atan2(dx));

    // Create material based on whether random colors are enabled
    let wall_material = if WALL_RANDOM_COLORS {
        let mut rng = rand::rng();
        StandardMaterial {
            base_color: Color::srgb(
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
            ),
            ..default()
        }
    } else {
        StandardMaterial {
            base_color_texture: Some(load_repeating_texture(asset_server, "wall.png")),
            ..default()
        }
    };

    let mesh = tiled_cuboid(mesh_size_x, WALL_HEIGHT, mesh_size_z, WALL_TEXTURE_TILE_SIZE);

    commands.spawn(WallBundle {
        mesh: Mesh3d(meshes.add(mesh)),
        material: MeshMaterial3d(materials.add(wall_material)),
        transform: Transform::from_xyz(
            center_x,
            WALL_HEIGHT / 2.0, // Lift so bottom is at y=0
            center_z,
        )
        .with_rotation(rotation),
        visibility: Visibility::default(),
        marker: WallMarker,
    });
}

// Spawn a roof entity based on a shared `Roof` config.
pub fn spawn_roof(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    roof: &Roof,
) {
    use rand::Rng;

    // Calculate roof center and dimensions from corners
    let center_x = f32::midpoint(roof.x1, roof.x2);
    let center_z = f32::midpoint(roof.z1, roof.z2);

    let width = (roof.x2 - roof.x1).abs();
    let depth = (roof.z2 - roof.z1).abs();

    // Create material based on whether random colors are enabled
    let roof_material = if ROOF_RANDOM_COLORS {
        let mut rng = rand::rng();
        StandardMaterial {
            base_color: Color::srgb(
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
                rng.random_range(0.2..1.0),
            ),
            ..default()
        }
    } else {
        StandardMaterial {
            base_color_texture: Some(load_repeating_texture(asset_server, "roof.png")),
            ..default()
        }
    };

    // Use the actual aspect ratio to compute tile repeats for square texels
    let mesh = tiled_cuboid(width, roof.thickness, depth, ROOF_TEXTURE_TILE_SIZE);

    commands.spawn(RoofBundle {
        mesh: Mesh3d(meshes.add(mesh)),
        material: MeshMaterial3d(materials.add(roof_material)),
        transform: Transform::from_xyz(
            center_x,
            WALL_HEIGHT + roof.thickness / 2.0, // Position so bottom of roof sits on top of wall
            center_z,
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
