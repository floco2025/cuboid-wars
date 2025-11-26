use bevy::prelude::*;
use common::protocol::PlayerId;

use crate::components::LocalPlayer;

// ============================================================================
// UI Components and Constants
// ============================================================================

// World dimensions
pub const WORLD_SIZE: f32 = 2000.0;

// Camera settings
pub const CAMERA_X: f32 = 0.0;
pub const CAMERA_Y: f32 = 1500.0;
pub const CAMERA_Z: f32 = 2000.0;

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
    commands
        .spawn((
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

    // Create crosshair UI (initially hidden, shown when local player exists)
    let crosshair_size = 20.0;
    let crosshair_thickness = 2.0;
    let crosshair_color = Color::srgba(1.0, 1.0, 1.0, 0.8);
    
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(crosshair_size * 2.0),
                height: Val::Px(crosshair_size * 2.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
            CrosshairUI,
        ))
        .with_children(|parent| {
            // Horizontal line
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
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
    player_query: Query<&PlayerId>,
    local_player_query: Query<&PlayerId, With<LocalPlayer>>,
    player_list_ui: Single<Entity, With<PlayerListUI>>,
    existing_entries: Query<(Entity, &PlayerEntryUI)>,
) {
    // Get the latest player data from server messages
    let mut player_data: std::collections::HashMap<PlayerId, i32> = std::collections::HashMap::new();
    
    // We'll need to track this separately - for now just initialize everyone to 0
    // In the future, we'll update this when we receive player updates
    for player_id in player_query.iter() {
        player_data.insert(*player_id, 0);
    }
    
    // Get local player ID if it exists
    let local_player_id = local_player_query.iter().next().copied();
    
    // Remove all existing entries - we'll rebuild the list in sorted order
    for (entity, _) in existing_entries.iter() {
        commands.entity(entity).despawn();
    }
    
    // Collect and sort players by ID
    let mut sorted_players: Vec<(PlayerId, i32)> = player_data.iter()
        .map(|(id, hits)| (*id, *hits))
        .collect();
    sorted_players.sort_by_key(|(id, _)| id.0);
    
    // Create entries for all players in sorted order
    for (player_id, hits) in &sorted_players {
        let player_num = player_id.0;
        let is_local = local_player_id == Some(*player_id);
        
        let hit_color = if *hits > 0 {
            Color::srgb(0.3, 0.6, 1.0) // Blue for positive
        } else if *hits < 0 {
            Color::srgb(1.0, 0.3, 0.3) // Red for negative
        } else {
            Color::srgb(0.8, 0.8, 0.8) // Gray for zero
        };
        
        let sign = if *hits >= 0 { "+" } else { "" };
        
        // Highlight local player with yellow background
        let background_color = if is_local {
            BackgroundColor(Color::srgba(0.8, 0.8, 0.0, 0.3))
        } else {
            BackgroundColor(Color::NONE)
        };
        
        commands.entity(*player_list_ui).with_children(|parent| {
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(5.0)),
                    ..default()
                },
                background_color,
                PlayerEntryUI(*player_id),
            )).with_children(|row| {
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
            });
        });
    }
}

// Show crosshair only when local player exists
pub fn toggle_crosshair_system(
    local_player_query: Query<(), With<LocalPlayer>>,
    crosshair_query: Single<&mut Visibility, With<CrosshairUI>>,
) {
    *crosshair_query.into_inner() = if local_player_query.iter().count() > 0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}
