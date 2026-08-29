use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use super::fade::fade_out_alpha;
use crate::{
    config::ClientSettings,
    constants::{BANNER_FADE_SECS, BANNER_MAX_MESSAGES},
};

// Translucent black background at full opacity. The longest-lived line's
// fade-out modulates it so the band and its last text disappear together.
const HUD_BANNER_BG_BASE_ALPHA: f32 = 0.45;

// The band: spawned once, hidden until a message arrives.
#[derive(Component)]
pub struct HudBannerMarker;

// One message row, identified by the message's sequence number.
#[derive(Component)]
pub struct HudBannerLine {
    seq: u64,
}

// Console-like HUD banner: messages stack in arrival order (oldest on top),
// each fading out on its own timer; the band shows while any is alive.
// Every caller just pushes — a completion, the quest it unlocks, and "You
// died!" all get their own line whatever order they arrive in.
#[derive(Resource, Default)]
pub struct HudBanner {
    messages: Vec<BannerMessage>,
    next_seq: u64,
}

struct BannerMessage {
    seq: u64,
    text: String,
    remaining_secs: f32,
}

impl HudBanner {
    pub fn push(&mut self, text: String, duration_secs: f32) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.messages.push(BannerMessage {
            seq,
            text,
            remaining_secs: duration_secs,
        });
        seq
    }

    // Age every message; expired ones go, and the oldest beyond the cap go
    // too so a burst can't fill the screen.
    fn tick(&mut self, delta: f32) {
        for message in &mut self.messages {
            message.remaining_secs -= delta;
        }
        self.messages.retain(|message| message.remaining_secs > 0.0);
        let overflow = self.messages.len().saturating_sub(BANNER_MAX_MESSAGES);
        self.messages.drain(..overflow);
    }
}

pub fn spawn_hud_banner_root(commands: &mut Commands) {
    commands.spawn((
        HudBannerMarker,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            // Below the crosshair, above the console; grows downward with
            // its lines.
            top: Val::Percent(60.0),
            width: Val::Percent(100.0),
            padding: UiRect::vertical(Val::Px(12.0)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(4.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, HUD_BANNER_BG_BASE_ALPHA)),
        Visibility::Hidden,
    ));
}

// Ages the stack, drops rows whose message is gone, spawns rows for new
// messages (ascending seq keeps the newest at the bottom), fades each row
// over its final `BANNER_FADE_SECS`, and shows the band while any line lives.
pub fn hud_banner_system(
    mut commands: Commands,
    time: Res<Time>,
    mut banner: ResMut<HudBanner>,
    client_settings: Res<ClientSettings>,
    band: Single<(Entity, &mut Visibility, &mut BackgroundColor), With<HudBannerMarker>>,
    mut lines: Query<(Entity, &HudBannerLine, &mut TextColor)>,
) {
    banner.tick(time.delta_secs());
    let fades: HashMap<u64, f32> = banner
        .messages
        .iter()
        .map(|message| (message.seq, fade_out_alpha(message.remaining_secs, BANNER_FADE_SECS)))
        .collect();

    let mut shown: HashSet<u64> = HashSet::new();
    for (entity, line, mut color) in &mut lines {
        match fades.get(&line.seq) {
            Some(fade) => {
                color.0.set_alpha(*fade);
                shown.insert(line.seq);
            }
            None => commands.entity(entity).despawn(),
        }
    }

    let (band_entity, mut visibility, mut background) = band.into_inner();
    let font_size = client_settings.hud.font_sizes.banner;
    for message in &banner.messages {
        if shown.contains(&message.seq) {
            continue;
        }
        let row = commands
            .spawn((
                HudBannerLine { seq: message.seq },
                Text::new(message.text.clone()),
                TextFont {
                    font_size: FontSize::Px(font_size),
                    ..default()
                },
                TextColor(Color::WHITE),
            ))
            .id();
        commands.entity(band_entity).add_child(row);
    }

    let band_fade = fades.values().copied().fold(0.0_f32, f32::max);
    background.0.set_alpha(HUD_BANNER_BG_BASE_ALPHA * band_fade);
    visibility.set_if_neq(if banner.messages.is_empty() {
        Visibility::Hidden
    } else {
        Visibility::Visible
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(banner: &HudBanner) -> Vec<&str> {
        banner.messages.iter().map(|message| message.text.as_str()).collect()
    }

    #[test]
    fn push_assigns_increasing_sequence_numbers() {
        let mut banner = HudBanner::default();
        assert_eq!(banner.push("first".to_owned(), 5.0), 0);
        assert_eq!(banner.push("second".to_owned(), 10.0), 1);
        assert_eq!(texts(&banner), ["first", "second"]);
    }

    #[test]
    fn tick_expires_messages_independently() {
        let mut banner = HudBanner::default();
        banner.push("short".to_owned(), 5.0);
        banner.push("long".to_owned(), 10.0);

        banner.tick(6.0);
        assert_eq!(texts(&banner), ["long"]);

        banner.tick(5.0);
        assert!(texts(&banner).is_empty());
    }

    #[test]
    fn stack_is_capped_at_the_oldest() {
        let mut banner = HudBanner::default();
        for index in 0..=BANNER_MAX_MESSAGES {
            banner.push(index.to_string(), 10.0);
        }
        banner.tick(0.0);
        assert_eq!(banner.messages.len(), BANNER_MAX_MESSAGES);
        assert_eq!(texts(&banner)[0], "1", "the oldest message is evicted first");
    }
}
