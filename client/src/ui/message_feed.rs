use std::collections::VecDeque;

use bevy::{ecs::hierarchy::ChildSpawnerCommands, prelude::*};
use common::protocol::{FeedStyle, SFeed};

use super::timed_lines::{TimedLine, TimedLines};
use crate::{
    barriers::BarrierAssets,
    config::ClientSettings,
    constants::{
        CONSOLE_TEXT_COLOR, FEED_CHAT_TEXT_COLOR, FEED_DIM_TEXT_COLOR, FEED_TEXT_COLOR, HUD_LINE_FADE_SECS,
        HUD_ROW_GAP_PX,
    },
};

#[derive(Resource, Default)]
pub struct MessageFeed {
    pending: VecDeque<SFeed>,
}

impl MessageFeed {
    pub fn push(&mut self, line: SFeed) {
        self.pending.push_back(line);
    }
}

#[derive(Component)]
pub struct MessageFeedMarker;

pub fn spawn_message_feed(column: &mut ChildSpawnerCommands, client_settings: &ClientSettings) {
    column.spawn((
        MessageFeedMarker,
        TimedLines {
            max_rows: client_settings.hud.message_feed.max_entries,
            background_alpha: 0.0,
        },
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(HUD_ROW_GAP_PX),
            ..default()
        },
        Visibility::Hidden,
    ));
}

pub fn ui_message_feed_system(
    mut commands: Commands,
    mut feed: ResMut<MessageFeed>,
    client_settings: Res<ClientSettings>,
    barrier_assets: Res<BarrierAssets>,
    root: Single<Entity, With<MessageFeedMarker>>,
) {
    if feed.pending.is_empty() {
        return;
    }
    let duration = client_settings.hud.message_feed.entry_duration_secs;
    let font_size = client_settings.hud.font_sizes.message_feed;
    for line in feed.pending.drain(..) {
        commands
            .spawn((
                TimedLine {
                    fade_out_secs: HUD_LINE_FADE_SECS,
                    remaining_secs: duration,
                },
                ChildOf(*root),
                Node {
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
            ))
            .with_children(|row| {
                for span in line.spans {
                    row.spawn((
                        Text::new(span.text),
                        TextFont {
                            font_size: FontSize::Px(font_size),
                            ..default()
                        },
                        TextColor(style_color(span.style, &barrier_assets)),
                    ));
                }
            });
    }
}

fn style_color(style: FeedStyle, barrier_assets: &BarrierAssets) -> Color {
    match style {
        FeedStyle::Default => FEED_TEXT_COLOR,
        FeedStyle::Dim => FEED_DIM_TEXT_COLOR,
        FeedStyle::Chat => FEED_CHAT_TEXT_COLOR,
        FeedStyle::Console => CONSOLE_TEXT_COLOR,
        FeedStyle::Barrier(kind) => color_with_full_alpha(barrier_assets.base_color(kind)),
    }
}

// The barrier meshes deliberately use translucent alpha in the world, but
// text needs full opacity to be legible.
fn color_with_full_alpha(color: Color) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, 1.0)
}

#[cfg(test)]
#[path = "tests/message_feed.rs"]
mod tests;
