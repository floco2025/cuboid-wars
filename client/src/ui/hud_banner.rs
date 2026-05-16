use std::collections::HashMap;

use bevy::prelude::*;
use common::protocol::QuestId;

use crate::config::ClientSettings;

// Translucent black background at full opacity. The fade-out tail
// modulates this base alpha so the panel and text disappear together at
// end-of-life rather than snapping out.
const HUD_BANNER_BG_BASE_ALPHA: f32 = 0.45;

// Map of currently-active (not-yet-completed) quests this client holds,
// keyed by quest id. Value is the cached announcement text so the
// announcement banner can be re-shown on respawn without re-querying the
// server. Co-located with the banner widget for now; if a second
// banner-using subsystem needs its own resource this can move to a
// dedicated quests module.
//
// Populated by the `SQuestNew` handler at login; entries are removed by
// the `SQuestAchieved` handler when the corresponding quest completes.
#[derive(Resource, Default)]
pub struct ActiveQuests {
    pub pending: HashMap<QuestId, String>,
}

// Marker for the centered, transient HUD banner. The banner UI node owns
// a `HudBannerTimer` and despawns when the timer hits 0.
#[derive(Component)]
pub struct HudBannerMarker;

#[derive(Component)]
pub struct HudBannerTimer {
    pub remaining_secs: f32,
}

// Spawn a vertically-centered full-width band (1/5 screen height) with
// `text` centered both horizontally and vertically, fading out over the
// final `client_settings.hud.banner.fade_duration_secs` of its lifetime.
//
// Single-banner invariant: any existing banner entity is despawned first.
// Two banners overlapping looks broken (same position, same translucent
// background) so the widget reads cleanly as one HUD message at a time.
pub fn spawn_hud_banner(
    commands: &mut Commands,
    existing: &Query<Entity, With<HudBannerMarker>>,
    text: &str,
    duration_secs: f32,
    font_size: f32,
) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    commands
        .spawn((
            HudBannerMarker,
            HudBannerTimer {
                remaining_secs: duration_secs,
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
                Text::new(text),
                TextFont { font_size, ..default() },
                TextColor(Color::WHITE),
            ));
        });
}

// Ticks the lifetime timer and fades the banner's background and text
// alpha together over the final `fade_duration_secs`. While `remaining`
// is above that window, the `min(1.0)` keeps everything at full opacity.
pub fn tick_hud_banner_system(
    mut commands: Commands,
    time: Res<Time>,
    client_settings: Res<ClientSettings>,
    mut banners: Query<(Entity, &mut HudBannerTimer, &mut BackgroundColor, &Children), With<HudBannerMarker>>,
    mut texts: Query<&mut TextColor>,
) {
    let delta = time.delta_secs();
    let fade_secs = client_settings.hud.banner.fade_duration_secs;
    for (entity, mut timer, mut bg, children) in &mut banners {
        timer.remaining_secs -= delta;
        if timer.remaining_secs <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let fade = (timer.remaining_secs / fade_secs).min(1.0);
        bg.0.set_alpha(HUD_BANNER_BG_BASE_ALPHA * fade);
        for child in children {
            if let Ok(mut tc) = texts.get_mut(*child) {
                tc.0.set_alpha(fade);
            }
        }
    }
}
