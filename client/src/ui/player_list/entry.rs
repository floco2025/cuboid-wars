use bevy::prelude::*;
use common::protocol::{BarrierKindTable, Health, ItemType, PlayerId};

use super::components::{LOCAL_PLAYER_BG_COLOR, PlayerEntryMarker};
use crate::{
    barriers::BarrierAssets,
    constants::{
        HEALTH_BAR_PLAYER_LIST_HEIGHT, HEALTH_BAR_PLAYER_LIST_WIDTH, HUD_KEY_GAP_PX, HUD_KEY_ICON_HEIGHT_PX,
        HUD_KEY_ICON_WIDTH_PX,
    },
    items::item_type_color,
    players::PlayerInfo,
    ui::spawn_health_bar,
};

pub(super) fn spawn_player_entry(
    commands: &mut Commands,
    player_info: &PlayerInfo,
    player_id: PlayerId,
    is_local: bool,
    max_health: f32,
    current_health: f32,
    kind_table: &BarrierKindTable,
    barrier_assets: Option<&BarrierAssets>,
    font_size: f32,
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
                ..default()
            },
            background_color,
        ))
        .with_children(|entry| {
            entry
                .spawn((Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(10.0),
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn((
                        Text::new(&player_info.name),
                        TextFont {
                            font_size,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    row.spawn((
                        Text::new(format_signed_score(player_info.score)),
                        TextFont {
                            font_size,
                            ..default()
                        },
                        TextColor(score_value_color(player_info.score)),
                    ));

                    spawn_power_up_icon(row, player_info.speed_power_up, ItemType::SpeedPowerUp);
                    spawn_power_up_icon(row, player_info.multi_shot_power_up, ItemType::MultiShotPowerUp);
                    spawn_power_up_icon(row, player_info.phasing_power_up, ItemType::PhasingPowerUp);
                    spawn_power_up_icon(row, player_info.anti_gravity_power_up, ItemType::AntiGravityPowerUp);

                    if !player_info.held_keys.is_empty()
                        && let Some(assets) = barrier_assets
                    {
                        // Spacer between power-up icons and the held-keys
                        // strip so the two categories read as different.
                        row.spawn(Node {
                            width: Val::Px(HUD_KEY_GAP_PX),
                            ..default()
                        });
                        for kind in &player_info.held_keys {
                            // Defensive: skip kinds the client wasn't told
                            // about (config drift between server + client).
                            if kind_table.id(*kind).is_none() {
                                continue;
                            }
                            spawn_key_icon(row, assets.base_color(*kind));
                        }
                    }
                });

            spawn_health_bar(
                entry,
                player_info.entity,
                max_health,
                current_health,
                HEALTH_BAR_PLAYER_LIST_WIDTH,
                HEALTH_BAR_PLAYER_LIST_HEIGHT,
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

fn spawn_power_up_icon(row: &mut ChildSpawnerCommands, active: bool, item_type: ItemType) {
    if !active {
        return;
    }

    row.spawn((
        Node {
            width: Val::Px(12.0),
            height: Val::Px(12.0),
            align_self: AlignSelf::Center,
            ..default()
        },
        BackgroundColor(item_type_color(item_type)),
    ));
}

// Thin vertical bar — visually distinct from the 12×12 power-up squares so
// "currently active power-up" and "key in inventory" don't read as the same
// category at a glance.
fn spawn_key_icon(row: &mut ChildSpawnerCommands, color: Color) {
    row.spawn((
        Node {
            width: Val::Px(HUD_KEY_ICON_WIDTH_PX),
            height: Val::Px(HUD_KEY_ICON_HEIGHT_PX),
            align_self: AlignSelf::Center,
            ..default()
        },
        BackgroundColor(color),
    ));
}

fn format_signed_score(score: i32) -> String {
    if score >= 0 {
        format!("+{score}")
    } else {
        score.to_string()
    }
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
