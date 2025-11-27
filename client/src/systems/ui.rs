use bevy::prelude::*;

use common::{constants::FIELD_WIDTH, protocol::PlayerId};
use crate::resources::{MyPlayerId, PlayerMap};

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

// ============================================================================
// UI Components and Constants
// ============================================================================

// World dimensions - ground plane should be larger than field for visibility
pub const WORLD_SIZE: f32 = FIELD_WIDTH * 2.0; // 200m x 200m ground plane

// Camera settings (human scale - third person view)
pub const CAMERA_X: f32 = 0.0;
pub const CAMERA_Y: f32 = 2.5; // 2.5 meters above ground
pub const CAMERA_Z: f32 = 3.0; // 3 meters back

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
        Mesh3d(meshes.add(Plane3d::default().mesh().size(WORLD_SIZE, WORLD_SIZE))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::default(),
    ));

    // Add camera with top-down view
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(CAMERA_X, CAMERA_Y, CAMERA_Z).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Add a directional light
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Add ambient light so everything is visible
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.5,
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
}

// ============================================================================
// UI Update Systems
// ============================================================================

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

    // Remove entries for players that no longer exist
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

    // For each player, either create or update their entry
    for (player_id, player_info) in players.0.iter() {
        let hits = player_info.hits;
        let player_num = player_id.0;
        let is_local = local_player_id == Some(*player_id);

        if let Some(&(_existing_entity, entry_children)) = existing_map.get(player_id) {
            // Entry exists - just update the hit counter text and color
            // The entry IS the row, and has 2 text children: name and hit counter
            if entry_children.len() >= 2 {
                let hit_text_entity = entry_children[1];

                // Update text content and color
                if let Ok((mut text, mut text_color)) = text_and_color_query.get_mut(hit_text_entity) {
                    let sign = if hits >= 0 { "+" } else { "" };
                    let new_text = format!("{}{}", sign, hits);
                    **text = new_text;

                    let hit_color = if hits > 0 {
                        Color::srgb(0.3, 0.6, 1.0) // Blue for positive
                    } else if hits < 0 {
                        Color::srgb(1.0, 0.3, 0.3) // Red for negative
                    } else {
                        Color::srgb(0.8, 0.8, 0.8) // Gray for zero
                    };
                    text_color.0 = hit_color;
                }
            }
        } else {
            // Entry doesn't exist - create it
            let hit_color = if hits > 0 {
                Color::srgb(0.3, 0.6, 1.0) // Blue for positive
            } else if hits < 0 {
                Color::srgb(1.0, 0.3, 0.3) // Red for negative
            } else {
                Color::srgb(0.8, 0.8, 0.8) // Gray for zero
            };

            let sign = if hits >= 0 { "+" } else { "" };

            // Highlight local player with yellow background
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
                    // Player name
                    row.spawn((
                        Text::new(format!("Player {}", player_num)),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    // Hit counter
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

            // Add as child to player list
            commands.entity(*player_list_ui).add_child(entry_entity);
        }
    }
}
