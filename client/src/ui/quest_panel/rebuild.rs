use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use bevy::prelude::*;
use common::protocol::QuestScope;

use super::{
    components::{QuestEntryMarker, QuestPanelMarker},
    state::{QuestEntry, QuestLog},
};
use crate::{
    config::ClientSettings,
    constants::{
        QUEST_BAR_COMPLETE_COLOR, QUEST_BAR_FILL_COLOR, QUEST_BAR_TRACK_COLOR, QUEST_ENTRY_BG_COLOR, QUEST_NOTE_COLOR,
        QUEST_NOTE_FONT_SCALE,
    },
};

// Rebuild the quest panel's rows when the quest log's rendered content
// changes. `QuestLog` is a `ResMut` touched on assignment/progress/completion,
// so `is_changed()` gates the common case; the content hash additionally
// suppresses a rebuild when a no-op update (e.g. a stale progress message the
// max-guard discarded) marked it changed without changing what we render.
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
        panel.bar_width,
        panel.bar_height,
        &children_query,
    );
}

fn rebuild_quest_panel(
    commands: &mut Commands,
    panel_entity: Entity,
    quest_log: &QuestLog,
    font_size: f32,
    bar_width: f32,
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
        .map(|(_, entry)| spawn_quest_entry(commands, entry, font_size, bar_width, bar_height))
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
    bar_width: f32,
    bar_height: f32,
) -> Entity {
    let ratio = if entry.threshold == 0 {
        1.0
    } else {
        (entry.progress as f32 / entry.threshold as f32).clamp(0.0, 1.0)
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
            QuestEntryMarker,
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Px(bar_width),
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
            // Progress bar: full-width track with a percent-width fill.
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
    format!("{}/{}", entry.progress, entry.threshold)
}

// The line under the bar that says whose progress this is; individual
// quests need none.
fn scope_note(entry: &QuestEntry) -> Option<String> {
    match entry.scope {
        QuestScope::Individual => None,
        QuestScope::Shared => Some("shared progress".to_owned()),
        QuestScope::Everyone => Some(format!("{} of {} players done", entry.players_done, entry.players)),
    }
}

// Hash of everything the panel renders, walked in display order so the result
// captures both content and row order. Any visible change (title, progress,
// threshold, counts, completed, or the order itself) forces a rebuild.
fn quest_panel_content_hash(quest_log: &QuestLog) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (id, entry) in quest_log.sorted() {
        id.0.hash(&mut hasher);
        entry.title.hash(&mut hasher);
        entry.scope.hash(&mut hasher);
        entry.progress.hash(&mut hasher);
        entry.threshold.hash(&mut hasher);
        entry.completed.hash(&mut hasher);
        entry.players_done.hash(&mut hasher);
        entry.players.hash(&mut hasher);
        entry.order.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::QuestId;

    fn entry(title: &str, progress: u32, threshold: u32, completed: bool) -> QuestEntry {
        ordered_entry(title, progress, threshold, completed, 0)
    }

    fn ordered_entry(title: &str, progress: u32, threshold: u32, completed: bool, order: u32) -> QuestEntry {
        QuestEntry {
            title: title.to_owned(),
            description: format!("{title} description"),
            scope: QuestScope::Individual,
            progress,
            threshold,
            completed,
            players_done: 0,
            players: 0,
            order,
        }
    }

    #[test]
    fn counter_and_scope_note_by_scope() {
        let mut entry = entry("Gold Rush", 7, 10, false);
        assert_eq!(quest_counter(&entry), "7/10");
        assert_eq!(scope_note(&entry), None);
        entry.scope = QuestScope::Shared;
        assert_eq!(scope_note(&entry).as_deref(), Some("shared progress"));
        entry.scope = QuestScope::Everyone;
        entry.players_done = 2;
        entry.players = 3;
        assert_eq!(scope_note(&entry).as_deref(), Some("2 of 3 players done"));
    }

    #[test]
    fn content_hash_changes_on_player_counts() {
        let mut everyone = entry("Gold Rush", 7, 10, false);
        everyone.scope = QuestScope::Everyone;
        everyone.players = 3;
        let before = quest_panel_content_hash(&log(vec![("gold", everyone.clone())]));
        everyone.players_done = 1;
        let after = quest_panel_content_hash(&log(vec![("gold", everyone)]));
        assert_ne!(before, after);
    }

    fn log(items: Vec<(&str, QuestEntry)>) -> QuestLog {
        let mut log = QuestLog::default();
        for (id, e) in items {
            log.entries.insert(QuestId(id.to_owned()), e);
        }
        log
    }

    #[test]
    fn content_hash_is_independent_of_insertion_order() {
        let forward = log(vec![
            ("a", entry("Gold", 3, 10, false)),
            ("b", entry("Hunt", 1, 4, false)),
        ]);
        let reverse = log(vec![
            ("b", entry("Hunt", 1, 4, false)),
            ("a", entry("Gold", 3, 10, false)),
        ]);

        assert_eq!(quest_panel_content_hash(&forward), quest_panel_content_hash(&reverse));
    }

    #[test]
    fn content_hash_changes_on_rendered_fields() {
        let base = log(vec![("a", entry("Gold", 3, 10, false))]);
        let base_hash = quest_panel_content_hash(&base);

        let advanced = log(vec![("a", entry("Gold", 4, 10, false))]);
        assert_ne!(quest_panel_content_hash(&advanced), base_hash);

        let done = log(vec![("a", entry("Gold", 10, 10, true))]);
        assert_ne!(quest_panel_content_hash(&done), base_hash);

        let retitled = log(vec![("a", entry("Gold Rush", 3, 10, false))]);
        assert_ne!(quest_panel_content_hash(&retitled), base_hash);

        let rethreshold = log(vec![("a", entry("Gold", 3, 12, false))]);
        assert_ne!(quest_panel_content_hash(&rethreshold), base_hash);

        let joined = log(vec![
            ("a", entry("Gold", 3, 10, false)),
            ("b", entry("Hunt", 0, 4, false)),
        ]);
        assert_ne!(quest_panel_content_hash(&joined), base_hash);
    }

    #[test]
    fn sorted_ranks_by_catalog_order_not_id() {
        // Ids sort "a" < "z", but catalog order puts "z" first.
        let log = log(vec![
            ("a_quest", ordered_entry("Second", 0, 1, false, 1)),
            ("z_quest", ordered_entry("First", 0, 1, false, 0)),
        ]);

        let ids: Vec<&str> = log.sorted().into_iter().map(|(id, _)| id.0.as_str()).collect();

        assert_eq!(ids, vec!["z_quest", "a_quest"], "lower catalog order wins over id");
    }

    #[test]
    fn sorted_breaks_order_ties_by_id() {
        let log = log(vec![
            ("b", ordered_entry("B", 0, 1, false, 5)),
            ("a", ordered_entry("A", 0, 1, false, 5)),
        ]);

        let ids: Vec<&str> = log.sorted().into_iter().map(|(id, _)| id.0.as_str()).collect();

        assert_eq!(ids, vec!["a", "b"], "equal order falls back to id");
    }
}
