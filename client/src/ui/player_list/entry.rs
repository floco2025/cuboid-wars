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
        MISSILE_HUD_ICON_HEIGHT_PX, POWER_UP_HUD_ICON_SIZE_PX,
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
    pub power_up_kinds: Vec<PowerUpKind>,
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

            // Empty inventory slots stay visible so pickups never resize the panel.
            entry
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::FlexStart,
                    column_gap: Val::Px(HUD_ICON_CATEGORY_GAP_PX),
                    ..default()
                })
                .with_children(|strip| {
                    if !style.power_up_kinds.is_empty() {
                        spawn_icon_group(strip, |row| {
                            for &kind in &style.power_up_kinds {
                                spawn_power_up_icon(row, player_info.power_up(kind), kind, shapes);
                            }
                        });
                    }
                    if style.show_missiles && style.max_missiles > 0 {
                        spawn_icon_group(strip, |row| {
                            for slot in 0..style.max_missiles {
                                spawn_missile_icon(row, slot < player_info.missiles, shapes);
                            }
                        });
                    }
                    if !key_kinds.is_empty() {
                        spawn_icon_group(strip, |row| {
                            for &kind in key_kinds {
                                let color = barrier_assets
                                    .filter(|_| player_info.held_keys.contains(&kind))
                                    .map_or(HUD_SLOT_EMPTY_COLOR, |assets| assets.base_color(kind));
                                spawn_key_icon(row, color, shapes);
                            }
                        });
                    }
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

fn spawn_power_up_icon(row: &mut ChildSpawnerCommands, active: bool, kind: PowerUpKind, shapes: &HudShapeAssets) {
    let color = if active {
        item_type_color(kind.to_item_type())
    } else {
        HUD_SLOT_EMPTY_COLOR
    };
    let node = Node {
        width: Val::Px(POWER_UP_HUD_ICON_SIZE_PX),
        height: Val::Px(POWER_UP_HUD_ICON_SIZE_PX),
        align_self: AlignSelf::Center,
        ..default()
    };
    let image = match kind {
        PowerUpKind::Speed => &shapes.speed,
        PowerUpKind::MultiShot => &shapes.multi_shot,
        PowerUpKind::LowGravity => &shapes.low_gravity,
        PowerUpKind::PortalGun => {
            row.spawn((
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(15.0),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Percent(50.0)),
                    ..node
                },
                BorderColor::all(color),
            ));
            return;
        }
    };
    row.spawn((
        node,
        ImageNode {
            color,
            ..ImageNode::new(image.clone())
        },
    ));
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

fn spawn_key_icon(row: &mut ChildSpawnerCommands, color: Color, shapes: &HudShapeAssets) {
    row.spawn((
        Node {
            height: Val::Px(KEY_HUD_ICON_SIZE_PX),
            align_self: AlignSelf::Center,
            ..default()
        },
        ImageNode {
            color,
            ..ImageNode::new(shapes.key.clone())
        },
    ));
}

fn spawn_missile_icon(row: &mut ChildSpawnerCommands, filled: bool, shapes: &HudShapeAssets) {
    let color = if filled {
        ITEM_MISSILE_COLOR
    } else {
        HUD_SLOT_EMPTY_COLOR
    };
    row.spawn((
        Node {
            height: Val::Px(MISSILE_HUD_ICON_HEIGHT_PX),
            align_self: AlignSelf::Center,
            ..default()
        },
        ImageNode {
            color,
            ..ImageNode::new(shapes.missile.clone())
        },
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
