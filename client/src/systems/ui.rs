#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;

#[allow(clippy::wildcard_imports)]
use crate::constants::*;
use crate::resources::{MyPlayerId, PlayerMap};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_COLS, GRID_ROWS, GRID_SIZE, PLAYER_HEIGHT, WALL_WIDTH},
    protocol::PlayerId,
};

// ============================================================================
// Components
// ============================================================================

// Marker component for the player list UI
#[derive(Component)]
pub struct PlayerListUI;

// Marker component for individual player entries
#[derive(Component)]
pub struct PlayerEntryUI(pub PlayerId);

// Marker component for the crosshair UI
#[derive(Component)]
pub struct CrosshairUI;

// Marker component for the RTT display
#[derive(Component)]
pub struct RttUI;

// Marker component for the bump flash overlay
#[derive(Component)]
pub struct BumpFlashUI;

// ============================================================================
// UI Setup System
// ============================================================================

pub fn setup_world_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create the ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(FIELD_WIDTH, FIELD_DEPTH))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::default(),
    ));

    // Create grid lines
    let grid_material = materials.add(Color::srgb(0.5, 0.5, 0.5)); // Grey color
    let line_height = 0.01; // Slightly above ground to avoid z-fighting

    // Vertical grid lines (along X axis, varying Z position)
    for i in 0..=GRID_ROWS {
        let z_pos = (i as f32 * GRID_SIZE) - FIELD_DEPTH / 2.0;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(FIELD_WIDTH, line_height, WALL_WIDTH))),
            MeshMaterial3d(grid_material.clone()),
            Transform::from_xyz(0.0, line_height / 2.0, z_pos),
        ));
    }

    // Horizontal grid lines (along Z axis, varying X position)
    for i in 0..=GRID_COLS {
        let x_pos = (i as f32 * GRID_SIZE) - FIELD_WIDTH / 2.0;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(WALL_WIDTH, line_height, FIELD_DEPTH))),
            MeshMaterial3d(grid_material.clone()),
            Transform::from_xyz(x_pos, line_height / 2.0, 0.0),
        ));
    }

    // Add camera (initial position will be immediately overridden by sync system)
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, PLAYER_HEIGHT * FPV_CAMERA_HEIGHT_RATIO, 0.0)
            .looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
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
        PlayerListUI,
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
            CrosshairUI,
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

    // Create RTT display in upper right corner
    commands.spawn((
        Text::new("RTT: --"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(10.0),
            top: Val::Px(10.0),
            ..default()
        },
        RttUI,
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
        BumpFlashUI,
        Visibility::Hidden, // Start hidden
    ));
}

// ============================================================================
// UI Update Systems
// ============================================================================

// Update RTT display
pub fn update_rtt_system(rtt: Res<crate::resources::RoundTripTime>, mut query: Single<&mut Text, With<RttUI>>) {
    if !rtt.is_changed() {
        return;
    }

    if rtt.rtt > 0.0 {
        query.0 = format!("RTT: {:.0}ms", rtt.rtt * 1000.0);
    } else {
        query.0 = "RTT: --".to_string();
    }
}

// Update player list UI with all players and their hit counts
pub fn update_player_list_system(
    mut commands: Commands,
    players: Res<PlayerMap>,
    my_player_id: Option<Res<MyPlayerId>>,
    player_list_ui: Single<Entity, With<PlayerListUI>>,
    existing_entries: Query<(Entity, &PlayerEntryUI, &Children)>,
    mut text_and_color_query: Query<(&mut Text, &mut TextColor)>,
) {
    // Only run when PlayerMap changes
    if !players.is_changed() {
        return;
    }

    // Get local player ID if it exists
    let local_player_id = my_player_id.as_ref().map(|id| id.0);

    // Collect existing entries into a map
    let existing_map: std::collections::HashMap<PlayerId, (Entity, &Children)> = existing_entries
        .iter()
        .map(|(entity, entry, children)| (entry.0, (entity, children)))
        .collect();

    // Check if we need to rebuild (players added/removed)
    let players_changed = existing_map.len() != players.0.len()
        || existing_map.keys().any(|id| !players.0.contains_key(id))
        || players.0.keys().any(|id| !existing_map.contains_key(id));

    // Remove entries for players that no longer exist
    if players_changed {
        let to_remove: Vec<PlayerId> = existing_map
            .keys()
            .filter(|player_id| !players.0.contains_key(player_id))
            .copied()
            .collect();

        for player_id in to_remove {
            if let Some((entity, _)) = existing_map.get(&player_id) {
                commands.entity(*entity).despawn();
            }
        }
    }

    // Sort players by ID for consistent ordering
    let mut sorted_players: Vec<_> = players.0.iter().collect();
    sorted_players.sort_by_key(|(player_id, _)| player_id.0);

    if players_changed {
        // Rebuild entire list in sorted order
        let mut sorted_entries = Vec::new();

        for (player_id, player_info) in sorted_players {
            let hits = player_info.hits;
            let player_num = player_id.0;
            let is_local = local_player_id == Some(*player_id);

            if let Some(&(existing_entity, _)) = existing_map.get(player_id) {
                // Reuse existing entity
                sorted_entries.push(existing_entity);
            } else {
                // Create new entry
                let hit_color = if hits > 0 {
                    Color::srgb(0.3, 0.6, 1.0)
                } else if hits < 0 {
                    Color::srgb(1.0, 0.3, 0.3)
                } else {
                    Color::srgb(0.8, 0.8, 0.8)
                };

                let sign = if hits >= 0 { "+" } else { "" };
                let background_color = if is_local {
                    BackgroundColor(Color::srgba(0.8, 0.8, 0.0, 0.3))
                } else {
                    BackgroundColor(Color::NONE)
                };

                let entry_entity = commands
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(10.0),
                            padding: UiRect::all(Val::Px(5.0)),
                            ..default()
                        },
                        background_color,
                        PlayerEntryUI(*player_id),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new(format!("Player {}", player_num)),
                            TextFont {
                                font_size: 20.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));

                        row.spawn((
                            Text::new(format!("{}{}", sign, hits)),
                            TextFont {
                                font_size: 20.0,
                                ..default()
                            },
                            TextColor(hit_color),
                        ));
                    })
                    .id();

                sorted_entries.push(entry_entity);
            }
        }

        // Rebuild children in sorted order
        commands.entity(*player_list_ui).replace_children(&sorted_entries);
    } else {
        // Just update hit counters (no rebuild needed)
        for (player_id, player_info) in sorted_players {
            let hits = player_info.hits;

            if let Some(&(_existing_entity, entry_children)) = existing_map.get(player_id) {
                if entry_children.len() >= 2 {
                    let hit_text_entity = entry_children[1];

                    if let Ok((mut text, mut text_color)) = text_and_color_query.get_mut(hit_text_entity) {
                        let sign = if hits >= 0 { "+" } else { "" };
                        let new_text = format!("{}{}", sign, hits);
                        **text = new_text;

                        let hit_color = if hits > 0 {
                            Color::srgb(0.3, 0.6, 1.0)
                        } else if hits < 0 {
                            Color::srgb(1.0, 0.3, 0.3)
                        } else {
                            Color::srgb(0.8, 0.8, 0.8)
                        };
                        text_color.0 = hit_color;
                    }
                }
            }
        }
    }
}
