use std::collections::HashMap;

use bevy::prelude::*;
use common::protocol::QuestId;

// Map of currently-active (not-yet-completed) quests this client holds,
// keyed by quest id. Value is the cached announcement text so the
// announcement overlay can be re-shown on respawn without re-querying the
// server.
//
// Populated by the `SQuestNew` handler at login; entries are removed by
// the `SQuestAchieved` handler when the corresponding quest completes.
#[derive(Resource, Default)]
pub struct ActiveQuests {
    pub pending: HashMap<QuestId, String>,
}

// Marker for the centered, transient quest banner. The banner UI node owns
// a `QuestOverlayTimer` and despawns when the timer hits 0.
#[derive(Component)]
pub struct QuestOverlayMarker;

#[derive(Component)]
pub struct QuestOverlayTimer {
    pub remaining_secs: f32,
}

// Spawn a vertically-centered full-width band (1/4 screen height) with
// `text` centered both horizontally and vertically, fading out after
// `duration_secs`. Caller picks the duration from
// `client_settings.hud.quest_overlay.{announcement,achieved}_duration_secs`.
pub fn spawn_quest_overlay(commands: &mut Commands, text: &str, duration_secs: f32, font_size: f32) {
    commands
        .spawn((
            QuestOverlayMarker,
            QuestOverlayTimer {
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new(text),
                TextFont { font_size, ..default() },
                TextColor(Color::WHITE),
            ));
        });
}

pub fn tick_quest_overlay_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut QuestOverlayTimer)>,
) {
    let delta = time.delta_secs();
    for (entity, mut timer) in &mut query {
        timer.remaining_secs -= delta;
        if timer.remaining_secs <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}
