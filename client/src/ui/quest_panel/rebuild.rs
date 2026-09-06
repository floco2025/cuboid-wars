use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use bevy::prelude::*;

use super::quest_log::{QuestEntry, QuestLog, QuestProgress};
use crate::{
    config::ClientSettings,
    constants::{
        QUEST_BAR_COMPLETE_COLOR, QUEST_BAR_FILL_COLOR, QUEST_BAR_TRACK_COLOR, QUEST_ENTRY_BG_COLOR, QUEST_NOTE_COLOR,
        QUEST_NOTE_FONT_SCALE,
    },
};

// Root node of the quest panel (top-right); its children, one card per
// quest, are rebuilt from `QuestLog`.
#[derive(Component)]
pub struct QuestPanelMarker;

// `QuestLog` is marked changed by every handler that touches it, including
// no-op updates (a stale progress value the max guard discarded, a snapshot
// restating group state), so the content hash decides whether the cards
// actually need rebuilding.
pub fn ui_quest_panel_rebuild_system(
    mut commands: Commands,
    quest_log: Res<QuestLog>,
    client_settings: Res<ClientSettings>,
    quest_panel_ui: Single<Entity, With<QuestPanelMarker>>,
    children_query: Query<&Children>,
    mut last_content: Local<Option<u64>>,
) {
    if !quest_log.is_changed() {
        return;
    }
    let content = quest_panel_content_hash(&quest_log);
    if *last_content == Some(content) {
        return;
    }
    *last_content = Some(content);

    let panel = &client_settings.hud.quest_panel;
    rebuild_quest_panel(
        &mut commands,
        *quest_panel_ui,
        &quest_log,
        client_settings.hud.font_sizes.quest_panel,
        panel.card_width,
        panel.bar_height,
        &children_query,
    );
}

fn rebuild_quest_panel(
    commands: &mut Commands,
    panel_entity: Entity,
    quest_log: &QuestLog,
    font_size: f32,
    card_width: f32,
    bar_height: f32,
    children_query: &Query<&Children>,
) {
    if let Ok(children) = children_query.get(panel_entity) {
        for &child in children {
            commands.entity(child).despawn();
        }
    }

    let ordered_children: Vec<Entity> = quest_log
        .sorted()
        .into_iter()
        .map(|(_, entry)| spawn_quest_entry(commands, entry, font_size, card_width, bar_height))
        .collect();
    commands.entity(panel_entity).replace_children(&ordered_children);
}

// One card per quest, all the same width: title left and counter right on
// the first line, the bar spanning the card, and for group quests a dim
// scope line underneath — so suffixes never make the rows ragged.
fn spawn_quest_entry(
    commands: &mut Commands,
    entry: &QuestEntry,
    font_size: f32,
    card_width: f32,
    bar_height: f32,
) -> Entity {
    let ratio = if entry.threshold == 0 {
        1.0
    } else {
        (entry.progress.value() as f32 / entry.threshold as f32).clamp(0.0, 1.0)
    };
    // Completion is conveyed by green title + full green bar; the counter
    // reads "N/N". No glyphs beyond ASCII (font-safe).
    let (fill_color, title_color) = if entry.completed {
        (QUEST_BAR_COMPLETE_COLOR, QUEST_BAR_COMPLETE_COLOR)
    } else {
        (QUEST_BAR_FILL_COLOR, Color::WHITE)
    };
    let text_font = TextFont {
        font_size: FontSize::Px(font_size),
        ..default()
    };

    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Px(card_width),
                row_gap: Val::Px(3.0),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(QUEST_ENTRY_BG_COLOR),
        ))
        .with_children(|card| {
            card.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                width: Val::Percent(100.0),
                ..default()
            })
            .with_children(|line| {
                line.spawn((
                    Text::new(entry.title.clone()),
                    text_font.clone(),
                    TextColor(title_color),
                ));
                line.spawn((
                    Text::new(quest_counter(entry)),
                    text_font.clone(),
                    TextColor(title_color),
                ));
            });
            card.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(bar_height),
                    ..default()
                },
                BackgroundColor(QUEST_BAR_TRACK_COLOR),
            ))
            .with_children(|track| {
                track.spawn((
                    Node {
                        width: Val::Percent(ratio * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(fill_color),
                ));
            });
            if let Some(note) = scope_note(entry) {
                card.spawn((
                    Text::new(note),
                    TextFont {
                        font_size: FontSize::Px(font_size * QUEST_NOTE_FONT_SCALE),
                        ..default()
                    },
                    TextColor(QUEST_NOTE_COLOR),
                ));
            }
        })
        .id()
}

fn quest_counter(entry: &QuestEntry) -> String {
    format!("{}/{}", entry.progress.value(), entry.threshold)
}

// The line under the bar that says whose progress this is; individual
// quests need none.
fn scope_note(entry: &QuestEntry) -> Option<String> {
    match entry.progress {
        QuestProgress::Own(_) => None,
        QuestProgress::Shared(_) => Some("shared progress".to_owned()),
        QuestProgress::Everyone {
            players_done,
            players_total,
            ..
        } => Some(format!("{players_done} of {players_total} players done")),
    }
}

// Walked in display order, so row order counts as content.
fn quest_panel_content_hash(quest_log: &QuestLog) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (id, entry) in quest_log.sorted() {
        id.0.hash(&mut hasher);
        entry.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::{super::test_support::*, *};
    use common::protocol::QuestScope;

    #[test]
    fn counter_and_scope_note_by_progress() {
        let own = entry("Gold Rush", QuestScope::Individual, 7, 10, 0);
        assert_eq!(quest_counter(&own), "7/10");
        assert_eq!(scope_note(&own), None);

        let shared = entry("Hunt", QuestScope::Shared, 2, 4, 0);
        assert_eq!(quest_counter(&shared), "2/4");
        assert_eq!(scope_note(&shared).as_deref(), Some("shared progress"));

        let mut everyone = entry("Gold Rush", QuestScope::Everyone, 7, 10, 0);
        everyone.progress = QuestProgress::Everyone {
            own: 7,
            players_done: 2,
            players_total: 3,
        };
        assert_eq!(quest_counter(&everyone), "7/10");
        assert_eq!(scope_note(&everyone).as_deref(), Some("2 of 3 players done"));
    }

    #[test]
    fn content_hash_is_independent_of_insertion_order() {
        let forward = log(vec![
            ("a", entry("Gold", QuestScope::Individual, 3, 10, 0)),
            ("b", entry("Hunt", QuestScope::Individual, 1, 4, 0)),
        ]);
        let reverse = log(vec![
            ("b", entry("Hunt", QuestScope::Individual, 1, 4, 0)),
            ("a", entry("Gold", QuestScope::Individual, 3, 10, 0)),
        ]);

        assert_eq!(quest_panel_content_hash(&forward), quest_panel_content_hash(&reverse));
    }

    #[test]
    fn content_hash_changes_on_rendered_fields() {
        let base = log(vec![("a", entry("Gold", QuestScope::Individual, 3, 10, 0))]);
        let base_hash = quest_panel_content_hash(&base);

        let advanced = log(vec![("a", entry("Gold", QuestScope::Individual, 4, 10, 0))]);
        assert_ne!(quest_panel_content_hash(&advanced), base_hash);

        let mut done = log(vec![("a", entry("Gold", QuestScope::Individual, 3, 10, 0))]);
        done.record_completion(common::protocol::QuestId("a".to_owned()));
        assert_ne!(quest_panel_content_hash(&done), base_hash);

        let retitled = log(vec![("a", entry("Gold Rush", QuestScope::Individual, 3, 10, 0))]);
        assert_ne!(quest_panel_content_hash(&retitled), base_hash);

        let rethreshold = log(vec![("a", entry("Gold", QuestScope::Individual, 3, 12, 0))]);
        assert_ne!(quest_panel_content_hash(&rethreshold), base_hash);

        let joined = log(vec![
            ("a", entry("Gold", QuestScope::Individual, 3, 10, 0)),
            ("b", entry("Hunt", QuestScope::Individual, 0, 4, 0)),
        ]);
        assert_ne!(quest_panel_content_hash(&joined), base_hash);

        let everyone = log(vec![("a", entry("Gold", QuestScope::Everyone, 3, 10, 0))]);
        let everyone_hash = quest_panel_content_hash(&everyone);
        assert_ne!(everyone_hash, base_hash);
        let mut counted = log(vec![("a", entry("Gold", QuestScope::Everyone, 3, 10, 0))]);
        counted.apply_group_status(&[common::protocol::QuestGroupStatus {
            id: common::protocol::QuestId("a".to_owned()),
            completed: false,
            progress: common::protocol::QuestGroupProgress::Everyone {
                players_done: 1,
                players_total: 3,
            },
        }]);
        assert_ne!(quest_panel_content_hash(&counted), everyone_hash);
    }
}
