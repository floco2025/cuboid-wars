use std::collections::VecDeque;

use bevy::{ecs::hierarchy::ChildSpawnerCommands, prelude::*};
use common::protocol::{BarrierKindId, BarrierKindTable, DeathCause, FeedEvent};

use super::timed_lines::{TimedLine, TimedLines};
use crate::{
    barriers::BarrierAssets,
    config::ClientSettings,
    constants::{CONSOLE_TEXT_COLOR, FEED_CHAT_TEXT_COLOR, FEED_DIM_TEXT_COLOR, FEED_TEXT_COLOR, HUD_ROW_GAP_PX},
};

// Feed lines received from the server, drained each frame into rows. The
// server resolves names and kinds at emit time, so rendering never consults
// the live `PlayerMap` / `ActorMap`.
#[derive(Resource, Default)]
pub struct MessageFeed {
    pending: VecDeque<FeedEvent>,
}

impl MessageFeed {
    pub fn push(&mut self, event: FeedEvent) {
        self.pending.push_back(event);
    }
}

#[derive(Component)]
pub struct MessageFeedMarker;

// One styled run of text within a feed line; multiple runs are laid out in
// a row so a single line can mix colors (e.g. the colored "key" in a
// `KeyFound` line surrounded by default-color text).
struct TextRun {
    text: String,
    color: Color,
}

// Rows stack in push order top-to-bottom, so the newest sits directly above
// the console prompt below and older rows are pushed upward.
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
    barrier_kinds: Res<BarrierKindTable>,
    root: Single<Entity, With<MessageFeedMarker>>,
) {
    if feed.pending.is_empty() {
        return;
    }
    let duration = client_settings.hud.message_feed.entry_duration_secs;
    let font_size = client_settings.hud.font_sizes.message_feed;
    for event in feed.pending.drain(..) {
        let runs = build_runs(&event, Some(&barrier_assets), &barrier_kinds);
        commands
            .spawn((
                TimedLine {
                    remaining_secs: duration,
                },
                ChildOf(*root),
                Node {
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
            ))
            .with_children(|row| {
                for run in runs {
                    row.spawn((
                        Text::new(run.text),
                        TextFont {
                            font_size: FontSize::Px(font_size),
                            ..default()
                        },
                        TextColor(run.color),
                    ));
                }
            });
    }
}

fn build_runs(
    event: &FeedEvent,
    barrier_assets: Option<&BarrierAssets>,
    barrier_kinds: &BarrierKindTable,
) -> Vec<TextRun> {
    match event {
        FeedEvent::PlayerJoined { name } => dim(format!("{name} joined")),
        FeedEvent::PlayerLeft { name } => dim(format!("{name} left")),
        FeedEvent::PlayerDied { name, cause } => death_runs(name, cause),
        FeedEvent::ActorDestroyed { name, kind } => plain(format!("{name} destroyed a {kind}")),
        FeedEvent::KeyFound { name, kind } => vec![
            TextRun {
                text: format!("{name} found a "),
                color: FEED_TEXT_COLOR,
            },
            kind_run(*kind, "key", barrier_assets),
        ],
        FeedEvent::QuestCompleted { name, title } => plain(format!("{name} completed {title}")),
        FeedEvent::EveryoneQuestPartDone {
            name,
            title,
            players_done,
            players_total,
        } => plain(format!(
            "{name} finished {title} ({players_done}/{players_total} players)"
        )),
        FeedEvent::GroupQuestCompleted { title } => plain(format!("Everyone completed {title}")),
        FeedEvent::BarrierOpened { name, kind } => vec![
            TextRun {
                text: format!("{name} opened the "),
                color: FEED_TEXT_COLOR,
            },
            kind_run(*kind, kind_name(*kind, barrier_kinds), barrier_assets),
            TextRun {
                text: " barriers".to_owned(),
                color: FEED_TEXT_COLOR,
            },
        ],
        FeedEvent::BarrierClosed { kind } => vec![
            TextRun {
                text: "The ".to_owned(),
                color: FEED_DIM_TEXT_COLOR,
            },
            kind_run(*kind, kind_name(*kind, barrier_kinds), barrier_assets),
            TextRun {
                text: " barriers closed".to_owned(),
                color: FEED_DIM_TEXT_COLOR,
            },
        ],
        FeedEvent::AdminReply { text } => vec![TextRun {
            text: text.clone(),
            color: CONSOLE_TEXT_COLOR,
        }],
        FeedEvent::AdminAction { name, text } => vec![TextRun {
            text: format!("{name}: {text}"),
            color: CONSOLE_TEXT_COLOR,
        }],
        FeedEvent::Chat { name, text } => vec![TextRun {
            text: format!("{name}: {text}"),
            color: FEED_CHAT_TEXT_COLOR,
        }],
    }
}

fn death_runs(name: &str, cause: &DeathCause) -> Vec<TextRun> {
    match cause {
        DeathCause::Shot { by } => plain(format!("{by} shot {name}")),
        DeathCause::SelfShot => plain(format!("{name} shot themselves")),
        DeathCause::Missile { by } => plain(format!("{by} blew up {name}")),
        DeathCause::SelfMissile => plain(format!("{name} blew themselves up")),
        DeathCause::Beam { kind } => plain(format!("{name} was zapped by a {kind}")),
        DeathCause::ActorBlast { kind } => plain(format!("{name} was blown up by a {kind}")),
        DeathCause::PlayerBlast { by } => plain(format!("{name} was caught in {by}'s explosion")),
        DeathCause::Fall => dim(format!("{name} fell")),
        DeathCause::Admin => plain(format!("{name} was killed by an admin")),
    }
}

fn plain(text: String) -> Vec<TextRun> {
    vec![TextRun {
        text,
        color: FEED_TEXT_COLOR,
    }]
}

fn dim(text: String) -> Vec<TextRun> {
    vec![TextRun {
        text,
        color: FEED_DIM_TEXT_COLOR,
    }]
}

// A word in the barrier kind's own color, so "key" / "lobby" read as the
// thing the player sees in the world.
fn kind_run(kind: BarrierKindId, word: &str, barrier_assets: Option<&BarrierAssets>) -> TextRun {
    let color = barrier_assets
        .map(|assets| color_with_full_alpha(assets.base_color(kind)))
        .unwrap_or(FEED_TEXT_COLOR);
    TextRun {
        text: word.to_owned(),
        color,
    }
}

fn kind_name(kind: BarrierKindId, barrier_kinds: &BarrierKindTable) -> &str {
    barrier_kinds.id(kind).unwrap_or("barrier")
}

// The barrier mesh deliberately uses translucent alpha for the in-world
// pulse, but text needs full opacity to be legible.
fn color_with_full_alpha(color: Color) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds() -> BarrierKindTable {
        BarrierKindTable::from_ids(vec!["treasure".to_owned()]).expect("one barrier kind")
    }

    fn runs(event: FeedEvent) -> Vec<TextRun> {
        build_runs(&event, None, &kinds())
    }

    fn line(event: FeedEvent) -> String {
        runs(event).iter().map(|run| run.text.as_str()).collect()
    }

    fn died(cause: DeathCause) -> FeedEvent {
        FeedEvent::PlayerDied {
            name: "Marc".to_owned(),
            cause,
        }
    }

    #[test]
    fn shot_line_names_shooter_and_victim() {
        let cause = DeathCause::Shot { by: "Bob".to_owned() };
        assert_eq!(line(died(cause)), "Bob shot Marc");
    }

    #[test]
    fn self_shot_line_has_no_second_name() {
        assert_eq!(line(died(DeathCause::SelfShot)), "Marc shot themselves");
        assert_eq!(line(died(DeathCause::SelfMissile)), "Marc blew themselves up");
    }

    #[test]
    fn actor_causes_name_the_kind() {
        let beam = DeathCause::Beam {
            kind: "zapper".to_owned(),
        };
        let blast = DeathCause::ActorBlast {
            kind: "mine".to_owned(),
        };
        assert_eq!(line(died(beam)), "Marc was zapped by a zapper");
        assert_eq!(line(died(blast)), "Marc was blown up by a mine");
    }

    #[test]
    fn fall_line_is_dim() {
        let runs = runs(died(DeathCause::Fall));
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Marc fell");
        assert_eq!(runs[0].color, FEED_DIM_TEXT_COLOR);
    }

    #[test]
    fn key_found_colors_only_the_key_word() {
        let runs = runs(FeedEvent::KeyFound {
            name: "Marc".to_owned(),
            kind: BarrierKindId(0),
        });
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "Marc found a ");
        assert_eq!(runs[1].text, "key");
    }

    #[test]
    fn barrier_opened_names_kind_from_table() {
        let opened = FeedEvent::BarrierOpened {
            name: "Marc".to_owned(),
            kind: BarrierKindId(0),
        };
        let closed = FeedEvent::BarrierClosed { kind: BarrierKindId(0) };
        assert_eq!(line(opened), "Marc opened the treasure barriers");
        assert_eq!(line(closed), "The treasure barriers closed");
    }

    #[test]
    fn group_quest_lines() {
        let part = FeedEvent::EveryoneQuestPartDone {
            name: "Marc".to_owned(),
            title: "Gold Rush".to_owned(),
            players_done: 2,
            players_total: 3,
        };
        let group = FeedEvent::GroupQuestCompleted {
            title: "Gold Rush".to_owned(),
        };
        assert_eq!(line(part), "Marc finished Gold Rush (2/3 players)");
        assert_eq!(line(group), "Everyone completed Gold Rush");
    }

    #[test]
    fn unknown_kind_falls_back_to_barrier() {
        let opened = FeedEvent::BarrierOpened {
            name: "Marc".to_owned(),
            kind: BarrierKindId(9),
        };
        assert_eq!(line(opened), "Marc opened the barrier barriers");
    }

    #[test]
    fn admin_lines_share_the_console_color_and_chat_is_white() {
        let action = runs(FeedEvent::AdminAction {
            name: "Marc".to_owned(),
            text: "weather set to rain".to_owned(),
        });
        let chat = runs(FeedEvent::Chat {
            name: "Marc".to_owned(),
            text: "hi".to_owned(),
        });
        assert_eq!(action[0].text, "Marc: weather set to rain");
        assert_eq!(action[0].color, CONSOLE_TEXT_COLOR);
        assert_eq!(chat[0].text, "Marc: hi");
        assert_eq!(chat[0].color, FEED_CHAT_TEXT_COLOR);
    }
}
