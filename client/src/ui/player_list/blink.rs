use bevy::prelude::*;
use common::protocol::PlayerId;

use super::components::{LOCAL_PLAYER_BG_COLOR, PlayerEntryMarker};
use crate::players::{MyPlayerId, PlayerMap};

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
                *bg_color = BackgroundColor(blink_stunned_color(base_color, blink_value));
            } else {
                *bg_color = BackgroundColor(base_color);
            }
        }
    }
}

fn blink_stunned_color(base_color: Color, blink_value: f32) -> Color {
    let stun_color = Color::srgba(1.0, 0.0, 0.0, 0.5);
    let base = base_color.to_srgba();
    let stun = stun_color.to_srgba();

    Color::srgba(
        base.red.mul_add(1.0 - blink_value, stun.red * blink_value),
        base.green.mul_add(1.0 - blink_value, stun.green * blink_value),
        base.blue.mul_add(1.0 - blink_value, stun.blue * blink_value),
        base.alpha.mul_add(1.0 - blink_value, stun.alpha * blink_value),
    )
}
