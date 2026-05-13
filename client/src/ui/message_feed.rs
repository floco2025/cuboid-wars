use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;
use common::protocol::{BarrierKindId, PlayerId};

use crate::{barriers::BarrierAssets, config::RenderSettings, constants::MESSAGE_FEED_MAX_ENTRIES};

// One entry in the bottom-right game-message feed. Names and kind ids are
// captured at emit time so the formatter doesn't depend on the live
// `PlayerMap` / `ActorMap` (handy for `PlayerLeft`, where the player is
// already gone by render time).
#[derive(Debug, Clone)]
pub enum GameMessage {
    Kill { killer_name: String, victim: KillVictim },
    SoloDeath { player_name: String },
    KeyFound { player_name: String, kind: BarrierKindId },
    PlayerJoined { name: String },
    PlayerLeft { name: String },
}

#[derive(Debug, Clone)]
pub enum KillVictim {
    Player(String),
    Actor(String),
}

// Pending messages produced by gameplay handlers, drained each frame by
// `render_pending_messages_system` into spawned text entries.
#[derive(Resource, Default)]
pub struct GameMessageFeed {
    pending: VecDeque<GameMessage>,
}

impl GameMessageFeed {
    pub fn push(&mut self, msg: GameMessage) {
        self.pending.push_back(msg);
    }
}

// Player IDs we've already announced as "joined" at some point in this
// session. Used to suppress duplicate "joined" lines on respawn — a player
// who dies leaves `PlayerMap` and re-enters via the next snapshot, which
// from `sync_players`' perspective looks identical to a fresh join.
#[derive(Resource, Default)]
pub struct SeenPlayerIds(HashSet<PlayerId>);

impl SeenPlayerIds {
    // Returns true the first time `id` is seen; false on subsequent calls.
    pub fn insert_if_new(&mut self, id: PlayerId) -> bool {
        self.0.insert(id)
    }
}

#[derive(Component)]
pub struct MessageFeedRoot;

#[derive(Component)]
pub struct MessageFeedEntry {
    timer: Timer,
}

const DEFAULT_TEXT_COLOR: Color = Color::srgba(0.85, 0.85, 0.85, 1.0);
const DIM_TEXT_COLOR: Color = Color::srgba(0.6, 0.6, 0.6, 1.0);
const ENTRY_FONT_SIZE: f32 = 18.0;
const ENTRY_ROW_GAP: f32 = 4.0;

// One styled run of text within a feed line; multiple runs are laid out in
// a row so a single line can mix colors (e.g. the colored "key" in a
// `KeyFound` line surrounded by default-color text).
struct TextRun {
    text: String,
    color: Color,
}

pub fn spawn_message_feed_root(commands: &mut Commands) {
    commands.spawn((
        MessageFeedRoot,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(10.0),
            bottom: Val::Px(10.0),
            // ColumnReverse so the newest child (pushed last) renders at the
            // bottom, with older entries stacking upward toward the top.
            flex_direction: FlexDirection::ColumnReverse,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(ENTRY_ROW_GAP),
            ..default()
        },
    ));
}

pub fn render_pending_messages_system(
    mut commands: Commands,
    mut feed: ResMut<GameMessageFeed>,
    render_settings: Res<RenderSettings>,
    barrier_assets: Option<Res<BarrierAssets>>,
    root_query: Query<(Entity, Option<&Children>), With<MessageFeedRoot>>,
) {
    if feed.pending.is_empty() {
        return;
    }
    let Ok((root, children)) = root_query.single() else {
        return;
    };

    let pending: Vec<_> = feed.pending.drain(..).collect();
    let mut live: Vec<Entity> = children.map(|c| c.iter().collect()).unwrap_or_default();
    let duration = render_settings.message_feed_entry_duration_secs;

    for msg in pending {
        let runs = build_runs(&msg, barrier_assets.as_deref());
        let entry = commands
            .spawn((
                MessageFeedEntry {
                    timer: Timer::from_seconds(duration, TimerMode::Once),
                },
                Node {
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
            ))
            .with_children(|line| {
                for run in &runs {
                    line.spawn((
                        Text::new(run.text.clone()),
                        TextFont {
                            font_size: ENTRY_FONT_SIZE,
                            ..default()
                        },
                        TextColor(run.color),
                    ));
                }
            })
            .id();
        commands.entity(root).add_children(&[entry]);
        live.push(entry);

        // Cap visible entries; the oldest (head) gets despawned first.
        while live.len() > MESSAGE_FEED_MAX_ENTRIES {
            let oldest = live.remove(0);
            commands.entity(oldest).despawn();
        }
    }
}

pub fn update_message_feed_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut MessageFeedEntry)>,
) {
    for (entity, mut entry) in &mut query {
        entry.timer.tick(time.delta());
        if entry.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

fn build_runs(msg: &GameMessage, barrier_assets: Option<&BarrierAssets>) -> Vec<TextRun> {
    match msg {
        GameMessage::Kill { killer_name, victim } => {
            let victim_label = match victim {
                KillVictim::Player(name) | KillVictim::Actor(name) => name.as_str(),
            };
            vec![TextRun {
                text: format!("{killer_name} killed {victim_label}"),
                color: DEFAULT_TEXT_COLOR,
            }]
        }
        GameMessage::SoloDeath { player_name } => vec![TextRun {
            text: format!("{player_name} died"),
            color: DIM_TEXT_COLOR,
        }],
        GameMessage::KeyFound { player_name, kind } => {
            // The key kind id is internal; surface the meaning by coloring
            // the word "key" with the kind's barrier color and leaving the
            // rest of the line in the default text color.
            let key_color = barrier_assets
                .map(|assets| color_with_full_alpha(assets.base_color(*kind)))
                .unwrap_or(DEFAULT_TEXT_COLOR);
            vec![
                TextRun {
                    text: format!("{player_name} found a "),
                    color: DEFAULT_TEXT_COLOR,
                },
                TextRun {
                    text: "key".to_string(),
                    color: key_color,
                },
            ]
        }
        GameMessage::PlayerJoined { name } => vec![TextRun {
            text: format!("{name} joined"),
            color: DIM_TEXT_COLOR,
        }],
        GameMessage::PlayerLeft { name } => vec![TextRun {
            text: format!("{name} left"),
            color: DIM_TEXT_COLOR,
        }],
    }
}

// Strip the alpha from a barrier base color so the text reads as fully
// opaque; the barrier mesh deliberately uses translucent alpha for the
// in-world pulse, but text needs full opacity to be legible.
fn color_with_full_alpha(color: Color) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, 1.0)
}
