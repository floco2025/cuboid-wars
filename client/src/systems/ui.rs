use bevy::{camera::Viewport, prelude::*, render::render_resource::Face};
use std::time::Duration;

use crate::{
    constants::*,
    resources::{CameraViewMode, FpsMeasurement, MyPlayerId, PlayerInfo, PlayerMap, RoundTripTime},
    spawning::item_type_color,
};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_COLS, GRID_ROWS, GRID_SIZE, PLAYER_HEIGHT, WALL_WIDTH},
    protocol::{ItemType, PlayerId},
};

// ============================================================================
// Components
// ============================================================================

// Marker component for the player list UI
#[derive(Component)]
pub struct PlayerListUIMarker;

// Marker component for the crosshair UI
#[derive(Component)]
pub struct CrosshairUIMarker;

// Marker component for the RTT display
#[derive(Component)]
pub struct RttUIMarker;

// Marker component for the FPS display
#[derive(Component)]
pub struct FpsUIMarker;

// Marker component for the bump flash overlay
#[derive(Component)]
pub struct BumpFlashUIMarker;

// Marker component for player entry rows
#[derive(Component)]
pub struct PlayerEntryMarker;


// ============================================================================
// UI Setup System
// ============================================================================

pub fn setup_world_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Create skybox sphere
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(500.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(asset_server.load("background.jpg")),
            unlit: true,
            cull_mode: Some(Face::Front), // Cull front faces to see inside
            ..default()
        })),
    ));

    // Create the ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(FIELD_WIDTH, FIELD_DEPTH))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(asset_server.load("floor.png")),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::default(),
    ));

    // Create grid lines (optional)
    if GRID_LINES {
        let grid_material = materials.add(Color::srgb(0.5, 0.5, 0.5)); // Grey color
        let line_height = 0.01; // Slightly above ground to avoid z-fighting

        // Vertical grid lines (along X axis, varying Z position)
        for i in 0..=GRID_ROWS {
            let z_pos = (i as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(FIELD_WIDTH, line_height, WALL_WIDTH))),
                MeshMaterial3d(grid_material.clone()),
                Transform::from_xyz(0.0, line_height / 2.0, z_pos),
            ));
        }

        // Horizontal grid lines (along Z axis, varying X position)
        for i in 0..=GRID_COLS {
            let x_pos = (i as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(WALL_WIDTH, line_height, FIELD_DEPTH))),
                MeshMaterial3d(grid_material.clone()),
                Transform::from_xyz(x_pos, line_height / 2.0, 0.0),
            ));
        }
    }

    // Add main camera (initial position will be immediately overridden by sync system)
    commands.spawn((
        Camera3d::default(),
        Camera {
            // Render first to full window
            order: 0,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: FPV_CAMERA_FOV_DEGREES.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, PLAYER_HEIGHT * FPV_CAMERA_HEIGHT_RATIO, 0.0)
            .looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
        IsDefaultUiCamera, // Mark this as the UI camera
    ));

    // Add rearview mirror camera (renders to lower-right viewport)
    commands.spawn((
        Camera3d::default(),
        Camera {
            // Render after main camera to its viewport only
            order: 1,
            // Viewport will be set by rearview_camera_viewport_system
            viewport: Some(Viewport {
                physical_position: UVec2::ZERO,
                physical_size: UVec2::new(100, 100),
                depth: 0.0..1.0,
            }),
            // Don't clear the viewport - render on top
            clear_color: bevy::camera::ClearColorConfig::None,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: REARVIEW_FOV_DEGREES.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, PLAYER_HEIGHT * FPV_CAMERA_HEIGHT_RATIO, 0.0)
            .looking_at(Vec3::new(0.0, 0.0, 1.0), Vec3::Y), // Looking backwards (positive Z)
        super::players::RearviewCamera,
    ));

    // Add soft directional light from above for shadows and definition
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 15.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Add ambient light for diffuse fill lighting
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 500.0,
        affects_lightmapped_meshes: false,
    });

    // Create player list UI
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            top: Val::Px(10.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            ..default()
        },
        PlayerListUIMarker,
    ));

    // Create crosshair UI
    let crosshair_size = 20.0;
    let crosshair_thickness = 2.0;
    let crosshair_color = Color::srgba(1.0, 1.0, 1.0, 0.8);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            CrosshairUIMarker,
        ))
        .with_children(|parent| {
            // Horizontal line
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-crosshair_size / 2.0),
                    top: Val::Px(-crosshair_thickness / 2.0),
                    width: Val::Px(crosshair_size),
                    height: Val::Px(crosshair_thickness),
                    ..default()
                },
                BackgroundColor(crosshair_color),
            ));
            // Vertical line
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-crosshair_thickness / 2.0),
                    top: Val::Px(-crosshair_size / 2.0),
                    width: Val::Px(crosshair_thickness),
                    height: Val::Px(crosshair_size),
                    ..default()
                },
                BackgroundColor(crosshair_color),
            ));
        });

    // Create RTT display in lower left corner
    commands.spawn((
        Text::new("RTT: --ms"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            bottom: Val::Px(40.0),
            ..default()
        },
        RttUIMarker,
    ));

    // Create FPS display below RTT
    commands.spawn((
        Text::new("FPS: --"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            bottom: Val::Px(10.0),
            ..default()
        },
        FpsUIMarker,
    ));

    // Create bump flash overlay (invisible by default, shown on wall collision)
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0)), // Transparent by default (white)
        BumpFlashUIMarker,
        Visibility::Hidden, // Start hidden
    ));
}

// ============================================================================
// UI Update Systems
// ============================================================================

// Update RTT display
pub fn ui_rtt_system(rtt: Res<RoundTripTime>, mut query: Single<&mut Text, With<RttUIMarker>>) {
    if !rtt.is_changed() {
        return;
    }

    if rtt.rtt > Duration::ZERO {
        query.0 = format!("RTT: {:.0}ms", rtt.rtt.as_secs_f64() * 1000.0);
    } else {
        query.0 = "RTT: --".to_string();
    }
}

// Update FPS measurement and display
pub fn ui_fps_system(
    time: Res<Time>,
    mut fps: ResMut<FpsMeasurement>,
    mut query: Single<&mut Text, With<FpsUIMarker>>,
) {
    // Update FPS measurement
    fps.frame_count += 1;
    fps.fps_timer += time.delta_secs();

    // Update FPS display once per second
    if fps.fps_timer >= 1.0 {
        fps.fps = fps.frame_count as f32 / fps.fps_timer;
        query.0 = format!("FPS: {:.0}", fps.fps);

        // Reset counters
        fps.frame_count = 0;
        fps.fps_timer = 0.0;
    }
}

// Toggle crosshair visibility based on camera view mode
pub fn ui_toggle_crosshair_system(
    view_mode: Res<CameraViewMode>,
    mut query: Query<&mut Visibility, With<CrosshairUIMarker>>,
) {
    if !view_mode.is_changed() {
        return;
    }

    for mut visibility in &mut query {
        *visibility = match *view_mode {
            CameraViewMode::FirstPerson => Visibility::Visible,
            CameraViewMode::TopDown => Visibility::Hidden,
        };
    }
}

// Update player list UI with all players and their hit counts
pub fn ui_player_list_system(
    mut commands: Commands,
    players: Res<PlayerMap>,
    my_player_id: Option<Res<MyPlayerId>>,
    player_list_ui: Single<Entity, With<PlayerListUIMarker>>,
    children_query: Query<&Children>,
) {
    // Bail out unless the player list changed
    if !players.is_changed() {
        return;
    }

    let local_player_id = my_player_id.as_ref().map(|id| id.0);

    // Just rebuild the entire list on every change for simplicity
    rebuild_player_list(
        &mut commands,
        *player_list_ui,
        &players,
        local_player_id,
        &children_query,
    );
}

fn rebuild_player_list(
    commands: &mut Commands,
    player_list_entity: Entity,
    players: &PlayerMap,
    local_player_id: Option<PlayerId>,
    children_query: &Query<&Children>,
) {
    // Despawn all existing children first
    if let Ok(children) = children_query.get(player_list_entity) {
        for &child in children {
            commands.entity(child).despawn();
        }
    }

    let mut sorted_players: Vec<_> = players.0.iter().collect();
    sorted_players.sort_by_key(|(player_id, _)| player_id.0);

    let mut ordered_children = Vec::with_capacity(sorted_players.len());
    for (player_id, player_info) in sorted_players {
        let entity = spawn_player_entry(commands, player_info, *player_id, local_player_id == Some(*player_id));
        ordered_children.push(entity);
    }

    commands.entity(player_list_entity).replace_children(&ordered_children);
}

fn spawn_player_entry(
    commands: &mut Commands,
    player_info: &PlayerInfo,
    player_id: PlayerId,
    is_local: bool,
) -> Entity {
    let background_color = if is_local {
        BackgroundColor(Color::srgba(0.8, 0.8, 0.0, 0.3))
    } else {
        BackgroundColor(Color::NONE)
    };

    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(5.0)),
                ..default()
            },
            background_color,
            PlayerEntryMarker,
            player_id,
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(&player_info.name),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));

            row.spawn((
                Text::new(format_signed_hits(player_info.hits)),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(hit_value_color(player_info.hits)),
            ));

            // Add power-up indicators
            if player_info.speed_power_up {
                row.spawn((
                    Node {
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                    BackgroundColor(item_type_color(ItemType::SpeedPowerUp)),
                ));
            }
            if player_info.multi_shot_power_up {
                row.spawn((
                    Node {
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                    BackgroundColor(item_type_color(ItemType::MultiShotPowerUp)),
                ));
            }
            if player_info.reflect_power_up {
                row.spawn((
                    Node {
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                    BackgroundColor(item_type_color(ItemType::ReflectPowerUp)),
                ));
            }
            if player_info.phasing_power_up {
                row.spawn((
                    Node {
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                    BackgroundColor(item_type_color(ItemType::PhasingPowerUp)),
                ));
            }
            if player_info.ghost_hunt_power_up {
                row.spawn((
                    Node {
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                    BackgroundColor(item_type_color(ItemType::GhostHuntPowerUp)),
                ));
            }
        })
        .id()
}

fn format_signed_hits(hits: i32) -> String {
    if hits >= 0 {
        format!("+{hits}")
    } else {
        hits.to_string()
    }
}

const fn hit_value_color(hits: i32) -> Color {
    if hits > 0 {
        Color::srgb(0.3, 0.6, 1.0)
    } else if hits < 0 {
        Color::srgb(1.0, 0.3, 0.3)
    } else {
        Color::srgb(0.8, 0.8, 0.8)
    }
}

// Make stunned player entries blink
pub fn ui_stunned_blink_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    my_player_id: Option<Res<MyPlayerId>>,
    mut query: Query<(&PlayerId, &mut BackgroundColor), With<PlayerEntryMarker>>,
) {
    let local_player_id = my_player_id.as_ref().map(|id| id.0);
    let blink_frequency = 3.0; // Blinks per second
    let blink_value = f32::midpoint(
        (time.elapsed_secs() * blink_frequency * std::f32::consts::PI * 2.0).sin(),
        1.0,
    );

    for (entry_id, mut bg_color) in &mut query {
        if let Some(player_info) = players.0.get(entry_id) {
            let is_local = local_player_id == Some(*entry_id);

            if player_info.stunned {
                // Blink between red and the base color
                let base_color = if is_local {
                    Color::srgba(0.8, 0.8, 0.0, 0.3)
                } else {
                    Color::srgba(0.0, 0.0, 0.0, 0.0)
                };
                let stun_color = Color::srgba(1.0, 0.0, 0.0, 0.5);

                *bg_color = BackgroundColor(Color::srgba(
                    base_color
                        .to_srgba()
                        .red
                        .mul_add(1.0 - blink_value, stun_color.to_srgba().red * blink_value),
                    base_color
                        .to_srgba()
                        .green
                        .mul_add(1.0 - blink_value, stun_color.to_srgba().green * blink_value),
                    base_color
                        .to_srgba()
                        .blue
                        .mul_add(1.0 - blink_value, stun_color.to_srgba().blue * blink_value),
                    base_color
                        .to_srgba()
                        .alpha
                        .mul_add(1.0 - blink_value, stun_color.to_srgba().alpha * blink_value),
                ));
            } else {
                // Not stunned - reset to base color
                let base_color = if is_local {
                    Color::srgba(0.8, 0.8, 0.0, 0.3)
                } else {
                    Color::NONE
                };
                *bg_color = BackgroundColor(base_color);
            }
        }
    }
}
