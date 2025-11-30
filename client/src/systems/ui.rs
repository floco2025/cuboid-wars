use bevy::prelude::*;
use std::{collections::HashMap, time::Duration};

use crate::constants::*;
use crate::resources::{MyPlayerId, PlayerInfo, PlayerMap};
use crate::spawning::item_type_color;
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_COLS, GRID_ROWS, GRID_SIZE, PLAYER_HEIGHT, WALL_WIDTH},
    protocol::{ItemType, PlayerId},
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

#[allow(clippy::too_many_lines)]
#[allow(clippy::cast_precision_loss)]
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

    if rtt.rtt > Duration::ZERO {
        query.0 = format!("RTT: {:.0}ms", rtt.rtt.as_secs_f64() * 1000.0);
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
    // Bail out unless the player list changed
    if !players.is_changed() {
        return;
    }

    let local_player_id = my_player_id.as_ref().map(|id| id.0);

    // Snapshot existing UI entries for quick lookup
    let existing_map: HashMap<PlayerId, (Entity, Vec<Entity>)> = existing_entries
        .iter()
        .map(|(entity, entry, children)| {
            let child_entities = children.iter().collect::<Vec<_>>();
            (entry.0, (entity, child_entities))
        })
        .collect();

    // Determine whether we must rebuild the entire list
    // Rebuild if: player set changed, or any player's item count changed
    let needs_rebuild = existing_map.len() != players.0.len()
        || existing_map.keys().any(|id| !players.0.contains_key(id))
        || players.0.keys().any(|id| !existing_map.contains_key(id))
        || players.0.iter().any(|(id, player_info)| {
            existing_map.get(id).map_or(true, |(_, children)| {
                children.len() != 2 + player_info.items.len()
            })
        });

    if needs_rebuild {
        remove_stale_entries(&mut commands, &players, &existing_map);
        rebuild_player_list(&mut commands, *player_list_ui, &players, &existing_map, local_player_id);
        return;
    }

    // Otherwise just update the text/color values in place
    let mut sorted_players: Vec<_> = players.0.iter().collect();
    sorted_players.sort_by_key(|(player_id, _)| player_id.0);
    update_hit_counters(&sorted_players, &existing_map, &mut text_and_color_query);
}

fn remove_stale_entries(
    commands: &mut Commands,
    players: &PlayerMap,
    existing_map: &HashMap<PlayerId, (Entity, Vec<Entity>)>,
) {
    let stale_players: Vec<PlayerId> = existing_map
        .keys()
        .filter(|player_id| !players.0.contains_key(player_id))
        .copied()
        .collect();

    for player_id in stale_players {
        if let Some((entity, _)) = existing_map.get(&player_id) {
            commands.entity(*entity).despawn();
        }
    }
}

fn rebuild_player_list(
    commands: &mut Commands,
    player_list_entity: Entity,
    players: &PlayerMap,
    existing_map: &HashMap<PlayerId, (Entity, Vec<Entity>)>,
    local_player_id: Option<PlayerId>,
) {
    let mut sorted_players: Vec<_> = players.0.iter().collect();
    sorted_players.sort_by_key(|(player_id, _)| player_id.0);

    let mut ordered_children = Vec::with_capacity(sorted_players.len());
    for (player_id, player_info) in sorted_players {
        // Check if we need to recreate this entry (item count changed)
        let needs_recreate = existing_map.get(player_id).map_or(false, |(_, children)| {
            children.len() != 2 + player_info.items.len()
        });

        let entity = if let Some((entity, _)) = existing_map.get(player_id) {
            if needs_recreate {
                // Despawn old entry and create new one with updated items
                commands.entity(*entity).despawn();
                spawn_player_entry(
                    commands,
                    *player_id,
                    &player_info.name,
                    player_info.hits,
                    &player_info.items,
                    local_player_id == Some(*player_id),
                )
            } else {
                // Reuse existing entry
                *entity
            }
        } else {
            // Create new entry
            spawn_player_entry(
                commands,
                *player_id,
                &player_info.name,
                player_info.hits,
                &player_info.items,
                local_player_id == Some(*player_id),
            )
        };
        ordered_children.push(entity);
    }

    commands.entity(player_list_entity).replace_children(&ordered_children);
}

fn update_hit_counters(
    sorted_players: &[(&PlayerId, &PlayerInfo)],
    existing_map: &HashMap<PlayerId, (Entity, Vec<Entity>)>,
    text_and_color_query: &mut Query<(&mut Text, &mut TextColor)>,
) {
    for (player_id, player_info) in sorted_players {
        if let Some((_entry_entity, children)) = existing_map.get(player_id) {
            if children.len() < 2 {
                continue;
            }

            // Update player name (first child)
            let name_text_entity = children[0];
            if let Ok((mut text, _)) = text_and_color_query.get_mut(name_text_entity) {
                (**text).clone_from(&player_info.name);
            }

            // Update hit counter (second child)
            let hit_text_entity = children[1];
            if let Ok((mut text, mut text_color)) = text_and_color_query.get_mut(hit_text_entity) {
                **text = format_signed_hits(player_info.hits);
                text_color.0 = hit_value_color(player_info.hits);
            }

            // Update item indicators (remaining children)
            for (i, item_type) in player_info.items.iter().enumerate() {
                let child_index = 2 + i;
                if child_index < children.len() {
                    let item_text_entity = children[child_index];
                    if let Ok((_, mut text_color)) = text_and_color_query.get_mut(item_text_entity) {
                        text_color.0 = item_type_color(*item_type);
                    }
                }
            }
        }
    }
}

fn spawn_player_entry(commands: &mut Commands, player_id: PlayerId, name: &str, hits: i32, items: &[ItemType], is_local: bool) -> Entity {
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
            PlayerEntryUI(player_id),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(name),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));

            row.spawn((
                Text::new(format_signed_hits(hits)),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(hit_value_color(hits)),
            ));

            // Add item indicators
            for item_type in items {
                row.spawn((
                    Text::new("●"),
                    TextFont {
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(item_type_color(*item_type)),
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
