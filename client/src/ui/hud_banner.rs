use std::collections::VecDeque;

use bevy::prelude::*;

use super::timed_lines::{TimedLine, TimedLines};
use crate::{
    config::ClientSettings,
    constants::{BANNER_BAND_ALPHA, BANNER_BAND_TOP_PERCENT, BANNER_MAX_LINES, HUD_ROW_GAP_PX},
};

#[derive(Component)]
pub struct HudBannerMarker;

// Centered band of stacked lines, each on its own timer. Every caller just
// pushes — a completion, the quest it unlocks, and "You died!" each get
// their own line in whatever order they arrive.
#[derive(Resource, Default)]
pub struct HudBanner {
    pending: VecDeque<(String, f32)>,
}

impl HudBanner {
    pub fn push(&mut self, text: String, duration_secs: f32) {
        self.pending.push_back((text, duration_secs));
    }

    #[cfg(test)]
    pub fn pending_texts(&self) -> Vec<&str> {
        self.pending.iter().map(|(text, _)| text.as_str()).collect()
    }
}

pub fn spawn_hud_banner(commands: &mut Commands) {
    commands.spawn((
        HudBannerMarker,
        TimedLines {
            max_rows: BANNER_MAX_LINES,
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
    for (text, duration_secs) in banner.pending.drain(..) {
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
