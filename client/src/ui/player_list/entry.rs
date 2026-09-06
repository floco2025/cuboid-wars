use bevy::prelude::*;
use common::protocol::{BarrierKindId, Health, PlayerId, PowerUpKind};

use super::{
    components::{LOCAL_PLAYER_BG_COLOR, PlayerEntryMarker},
    health_bar::spawn_health_bar,
    shapes::HudShapeAssets,
};
use crate::{
    barriers::BarrierAssets,
    constants::{
        HUD_ICON_CATEGORY_GAP_PX, HUD_ICON_GAP_PX, HUD_SLOT_EMPTY_COLOR, ITEM_MISSILE_COLOR, KEY_HUD_ICON_SIZE_PX,
        MISSILE_HUD_ICON_HEIGHT_PX, MISSILE_HUD_ICON_WIDTH_PX,
    },
    items::item_type_color,
    players::PlayerInfo,
};

// Style values for one player-list entry, resolved once per rebuild from
// `ClientSettings` (+ the shared gameplay config's missile cap).
pub(super) struct PlayerEntryStyle {
    pub name_font_size: f32,
    pub score_font_size: f32,
    pub min_entry_width: f32,
    pub health_bar_height: f32,
    pub max_missiles: u32,
    pub show_multi_shot: bool,
    pub show_missiles: bool,
}

pub(super) fn spawn_player_entry(
    commands: &mut Commands,
    player_info: &PlayerInfo,
    player_id: PlayerId,
    is_local: bool,
    max_health: f32,
    current_health: f32,
    key_kinds: &[BarrierKindId],
    barrier_assets: Option<&BarrierAssets>,
    shapes: &HudShapeAssets,
    style: &PlayerEntryStyle,
) -> Entity {
    let background_color = if is_local {
        BackgroundColor(LOCAL_PLAYER_BG_COLOR)
    } else {
        BackgroundColor(Color::NONE)
    };

    commands
        .spawn((
            PlayerEntryMarker,
            player_id,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(5.0)),
                min_width: Val::Px(style.min_entry_width),
                ..default()
            },
            background_color,
        ))
        .with_children(|entry| {
            entry
                .spawn((Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn((
                        Text::new(&player_info.name),
                        TextFont {
                            font_size: FontSize::Px(style.name_font_size),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    row.spawn((
                        // Score is rendered as an unsigned magnitude; sign is
                        // conveyed by the text color (`score_value_color`).
                        Text::new(player_info.score.unsigned_abs().to_string()),
                        TextFont {
                            font_size: FontSize::Px(style.score_font_size),
                            ..default()
                        },
                        TextColor(score_value_color(player_info.score)),
                    ));
                });

            // Icon strip on its own line so the entry stays narrow. Every
            // slot always renders (dim when unfilled), so pickups never
            // resize the panel, and the groups spread across the entry so
            // the strip spans it at any key count.
            entry
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(HUD_ICON_CATEGORY_GAP_PX),
                    ..default()
                })
                .with_children(|strip| {
                    spawn_icon_group(strip, |row| {
                        for kind in PowerUpKind::ALL {
                            if kind == PowerUpKind::MultiShot && !style.show_multi_shot {
                                continue;
                            }
                            spawn_power_up_icon(row, player_info.power_up(kind), kind, shapes);
                        }
                    });
                    if style.show_missiles {
                        spawn_icon_group(strip, |row| {
                            for slot in 0..style.max_missiles {
                                spawn_missile_icon(row, slot < player_info.missiles);
                            }
                        });
                    }
                    spawn_icon_group(strip, |row| {
                        for &kind in key_kinds {
                            let color = barrier_assets
                                .filter(|_| player_info.held_keys.contains(&kind))
                                .map_or(HUD_SLOT_EMPTY_COLOR, |assets| assets.base_color(kind));
                            spawn_key_icon(row, color);
                        }
                    });
                });

            spawn_health_bar(
                entry,
                player_info.entity,
                max_health,
                current_health,
                style.health_bar_height,
            );
        })
        .id()
}

pub(super) fn player_health(player_info: &PlayerInfo, health_query: &Query<&Health>, max_health: f32) -> f32 {
    let Ok(health) = health_query.get(player_info.entity) else {
        return max_health;
    };
    health.0
}

// Per-kind silhouettes matching the in-game meshes and the editor glyphs:
// speed = triangle (tetrahedron), multi-shot = square (cube), low-gravity =
// circle (sphere).
fn spawn_power_up_icon(row: &mut ChildSpawnerCommands, active: bool, kind: PowerUpKind, shapes: &HudShapeAssets) {
    let color = if active {
        item_type_color(kind.to_item_type())
    } else {
        HUD_SLOT_EMPTY_COLOR
    };
    let node = Node {
        width: Val::Px(12.0),
        height: Val::Px(12.0),
        align_self: AlignSelf::Center,
        ..default()
    };
    match kind {
        PowerUpKind::Speed => {
            row.spawn((
                node,
                ImageNode {
                    color,
                    ..ImageNode::new(shapes.triangle.clone())
                },
            ));
        }
        PowerUpKind::LowGravity => {
            let mut node = node;
            node.border_radius = BorderRadius::all(Val::Percent(50.0));
            row.spawn((node, BackgroundColor(color)));
        }
        PowerUpKind::MultiShot => {
            row.spawn((node, BackgroundColor(color)));
        }
    }
}

fn spawn_icon_group(strip: &mut ChildSpawnerCommands, icons: impl FnOnce(&mut ChildSpawnerCommands)) {
    strip
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(HUD_ICON_GAP_PX),
            ..default()
        })
        .with_children(icons);
}

fn spawn_key_icon(row: &mut ChildSpawnerCommands, color: Color) {
    row.spawn((
        Node {
            width: Val::Px(KEY_HUD_ICON_SIZE_PX),
            height: Val::Px(KEY_HUD_ICON_SIZE_PX),
            align_self: AlignSelf::Center,
            ..default()
        },
        BackgroundColor(color),
    ));
}

// Missile bay: a thin vertical line per rocket.
fn spawn_missile_icon(row: &mut ChildSpawnerCommands, filled: bool) {
    let color = if filled {
        ITEM_MISSILE_COLOR
    } else {
        HUD_SLOT_EMPTY_COLOR
    };
    row.spawn((
        Node {
            width: Val::Px(MISSILE_HUD_ICON_WIDTH_PX),
            height: Val::Px(MISSILE_HUD_ICON_HEIGHT_PX),
            align_self: AlignSelf::Center,
            ..default()
        },
        BackgroundColor(color),
    ));
}

const fn score_value_color(score: i32) -> Color {
    if score > 0 {
        Color::srgb(0.3, 0.6, 1.0)
    } else if score < 0 {
        Color::srgb(1.0, 0.3, 0.3)
    } else {
        Color::srgb(0.8, 0.8, 0.8)
    }
}
