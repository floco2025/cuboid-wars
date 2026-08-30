use std::collections::VecDeque;

use bevy::prelude::*;

use super::timed_lines::{TimedLine, TimedLines};
use crate::{
    config::ClientSettings,
    constants::{BANNER_BAND_ALPHA, BANNER_BAND_TOP_PERCENT, HUD_ROW_GAP_PX},
};

const DEATH_TEXT: &str = "You died!";

#[derive(Component)]
pub struct HudBannerMarker;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BannerMessage {
    QuestAnnouncement(String),
    QuestCompleted(String),
    Death,
}

impl BannerMessage {
    #[cfg(test)]
    fn text(&self) -> &str {
        match self {
            Self::QuestAnnouncement(text) | Self::QuestCompleted(text) => text,
            Self::Death => DEATH_TEXT,
        }
    }

    fn into_timed_text(self, client_settings: &ClientSettings) -> (String, f32) {
        match self {
            Self::QuestAnnouncement(text) => (text, client_settings.hud.banner.quest_announcement_secs),
            Self::QuestCompleted(text) => (text, client_settings.hud.banner.quest_completed_secs),
            Self::Death => (DEATH_TEXT.to_owned(), client_settings.hud.banner.death_secs),
        }
    }
}

#[derive(Resource, Default)]
pub struct HudBanner {
    pending: VecDeque<BannerMessage>,
}

impl HudBanner {
    pub fn push(&mut self, message: BannerMessage) {
        self.pending.push_back(message);
    }

    #[cfg(test)]
    pub fn pending_texts(&self) -> Vec<&str> {
        self.pending.iter().map(BannerMessage::text).collect()
    }
}

pub fn spawn_hud_banner(commands: &mut Commands, client_settings: &ClientSettings) {
    commands.spawn((
        HudBannerMarker,
        TimedLines {
            max_rows: client_settings.hud.banner.max_entries,
            background_alpha: BANNER_BAND_ALPHA,
        },
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Percent(BANNER_BAND_TOP_PERCENT),
            width: Val::Percent(100.0),
            padding: UiRect::vertical(Val::Px(12.0)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(HUD_ROW_GAP_PX),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, BANNER_BAND_ALPHA)),
        Visibility::Hidden,
    ));
}

pub fn ui_hud_banner_system(
    mut commands: Commands,
    mut banner: ResMut<HudBanner>,
    client_settings: Res<ClientSettings>,
    root: Single<Entity, With<HudBannerMarker>>,
) {
    if banner.pending.is_empty() {
        return;
    }
    let font_size = client_settings.hud.font_sizes.banner;
    for message in banner.pending.drain(..) {
        let (text, duration_secs) = message.into_timed_text(&client_settings);
        commands.spawn((
            TimedLine {
                remaining_secs: duration_secs,
            },
            ChildOf(*root),
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(font_size),
                ..default()
            },
            TextColor(Color::WHITE),
        ));
    }
}
