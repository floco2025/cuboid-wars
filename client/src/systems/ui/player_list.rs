use bevy::prelude::*;

use crate::{
    markers::{PlayerEntryMarker, PlayerListUIMarker},
    resources::{MyPlayerId, PlayerInfo, PlayerMap},
    spawning::item_type_color,
};
use common::protocol::{ItemType, PlayerId};

const LOCAL_PLAYER_BG_COLOR: Color = Color::srgba(0.8, 0.8, 0.0, 0.3);

pub fn ui_player_list_system(
    mut commands: Commands,
    players: Res<PlayerMap>,
    my_player_id: Option<Res<MyPlayerId>>,
    player_list_ui: Single<Entity, With<PlayerListUIMarker>>,
    children_query: Query<&Children>,
) {
    if !players.is_changed() {
        return;
    }

    let local_player_id = my_player_id.as_ref().map(|id| id.0);

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
        BackgroundColor(LOCAL_PLAYER_BG_COLOR)
    } else {
        BackgroundColor(Color::NONE)
    };

    commands
        .spawn((
            PlayerEntryMarker,
            player_id,
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(5.0)),
                ..default()
            },
            background_color,
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

pub fn ui_stunned_blink_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    my_player_id: Option<Res<MyPlayerId>>,
    mut query: Query<(&PlayerId, &mut BackgroundColor), With<PlayerEntryMarker>>,
) {
    let local_player_id = my_player_id.as_ref().map(|id| id.0);
    let blink_frequency = 3.0;
    let blink_value = f32::midpoint(
        (time.elapsed_secs() * blink_frequency * std::f32::consts::PI * 2.0).sin(),
        1.0,
    );

    for (entry_id, mut bg_color) in &mut query {
        if let Some(player_info) = players.0.get(entry_id) {
            let is_local = local_player_id == Some(*entry_id);
            let base_color = if is_local { LOCAL_PLAYER_BG_COLOR } else { Color::NONE };

            if player_info.stunned {
                let stun_color = Color::srgba(1.0, 0.0, 0.0, 0.5);
                let base = base_color.to_srgba();
                let stun = stun_color.to_srgba();

                *bg_color = BackgroundColor(Color::srgba(
                    base.red.mul_add(1.0 - blink_value, stun.red * blink_value),
                    base.green.mul_add(1.0 - blink_value, stun.green * blink_value),
                    base.blue.mul_add(1.0 - blink_value, stun.blue * blink_value),
                    base.alpha.mul_add(1.0 - blink_value, stun.alpha * blink_value),
                ));
            } else {
                *bg_color = BackgroundColor(base_color);
            }
        }
    }
}
