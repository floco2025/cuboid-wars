use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;
use common::protocol::{BarrierKindId, PlayerId};

use crate::{
    barriers::BarrierAssets,
    config::ClientSettings,
    constants::{CONSOLE_LINE_RESERVED_PX, HUD_EDGE_MARGIN_PX},
};

// One entry in the bottom-right game-message feed. Names and kind ids are
// captured at emit time so the formatter doesn't depend on the live
// `PlayerMap` / `ActorMap` (handy for `PlayerLeft`, where the player is
// already gone by render time).
#[derive(Debug, Clone)]
pub enum GameMessage {
    Kill { killer_name: String, victim_name: String },
    SoloDeath { player_name: String },
    KeyFound { player_name: String, kind: BarrierKindId },
    PlayerJoined { name: String },
    PlayerLeft { name: String },
    // Server reply to an admin console command; text is server-authored.
    Admin { text: String },
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
// Matches the console input line, so replies read as part of the console.
const ADMIN_TEXT_COLOR: Color = Color::srgba(1.0, 0.85, 0.4, 1.0);
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
            right: Val::Px(HUD_EDGE_MARGIN_PX),
            // Starts above the console's reserved strip so the input line and
            // the feed stack instead of overlapping.
            bottom: Val::Px(HUD_EDGE_MARGIN_PX + CONSOLE_LINE_RESERVED_PX),
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
    client_settings: Res<ClientSettings>,
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
    let duration = client_settings.hud.message_feed.entry_duration_secs;
    let font_size = client_settings.hud.font_sizes.message_feed;
    let max_entries = client_settings.hud.message_feed.max_entries;

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
                            font_size: FontSize::Px(font_size),
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
        while live.len() > max_entries {
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
        GameMessage::Kill {
            killer_name,
            victim_name,
        } => vec![TextRun {
            text: format!("{killer_name} killed {victim_name}"),
            color: DEFAULT_TEXT_COLOR,
        }],
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
        GameMessage::Admin { text } => vec![TextRun {
            text: text.clone(),
            color: ADMIN_TEXT_COLOR,
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
