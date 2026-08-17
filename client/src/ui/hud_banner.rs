use crate::constants::BANNER_FADE_SECS;
use bevy::prelude::*;

use super::fade::fade_out_alpha;
use crate::config::ClientSettings;

// Translucent black background at full opacity. The fade-out tail
// modulates this base alpha so the panel and text disappear together at
// end-of-life rather than snapping out.
const HUD_BANNER_BG_BASE_ALPHA: f32 = 0.45;

// Marker for the centered, transient HUD banner. The banner UI node owns
// a `HudBannerTimer` and despawns when the timer hits 0.
#[derive(Component)]
pub struct HudBannerMarker;

#[derive(Component)]
pub struct HudBannerTimer {
    pub remaining_secs: f32,
}

// A requested banner, drained by `render_pending_banner_system`. Gameplay
// handlers set it; the last request in a frame wins, matching the HUD's
// one-banner-at-a-time invariant (callers with several things to announce
// combine them into one multi-line text).
#[derive(Resource, Default)]
pub struct PendingBanner(Option<BannerRequest>);

struct BannerRequest {
    text: String,
    duration_secs: f32,
}

impl PendingBanner {
    pub fn set(&mut self, text: String, duration_secs: f32) {
        self.0 = Some(BannerRequest { text, duration_secs });
    }
}

// Spawn the requested banner: a vertically-centered full-width band (1/5
// screen height) with the text centered both ways, fading out over the final
// `BANNER_FADE_SECS` of its lifetime.
//
// Single-banner invariant: any existing banner entity is despawned first.
// Two banners overlapping looks broken (same position, same translucent
// background) so the widget reads cleanly as one HUD message at a time.
pub fn render_pending_banner_system(
    mut commands: Commands,
    mut pending: ResMut<PendingBanner>,
    client_settings: Res<ClientSettings>,
    existing: Query<Entity, With<HudBannerMarker>>,
) {
    if pending.0.is_none() {
        return;
    }
    let Some(request) = pending.0.take() else {
        return;
    };
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    commands
        .spawn((
            HudBannerMarker,
            HudBannerTimer {
                remaining_secs: request.duration_secs,
            },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                // 3/5 empty above, 1/5 band, 1/5 empty below.
                top: Val::Percent(60.0),
                width: Val::Percent(100.0),
                height: Val::Percent(20.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, HUD_BANNER_BG_BASE_ALPHA)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new(request.text),
                TextFont {
                    font_size: FontSize::Px(client_settings.hud.font_sizes.banner),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

// Ticks the lifetime timer and fades the banner's background and text
// alpha together over the final `fade_duration_secs`.
pub fn tick_hud_banner_system(
    mut commands: Commands,
    time: Res<Time>,
    _client_settings: Res<ClientSettings>,
    mut banners: Query<(Entity, &mut HudBannerTimer, &mut BackgroundColor, &Children), With<HudBannerMarker>>,
    mut texts: Query<&mut TextColor>,
) {
    let delta = time.delta_secs();
    let fade_secs = BANNER_FADE_SECS;
    for (entity, mut timer, mut bg, children) in &mut banners {
        timer.remaining_secs -= delta;
        if timer.remaining_secs <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let fade = fade_out_alpha(timer.remaining_secs, fade_secs);
        bg.0.set_alpha(HUD_BANNER_BG_BASE_ALPHA * fade);
        for child in children {
            if let Ok(mut tc) = texts.get_mut(*child) {
                tc.0.set_alpha(fade);
            }
        }
    }
}
