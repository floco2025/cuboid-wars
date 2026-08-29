use crate::{
    config::{Quest, QuestKind, ServerGameplayConfig},
    network::{ServerToClient, announce, broadcast_to_all},
    players::{PlayerInfo, PlayerMap},
};
use common::protocol::{
    FeedEvent, NewQuest, PlayerId, QuestId, QuestScope, SQuestCompleted, SQuestProgress, SQuestsAssigned, ServerMessage,
};

use super::{QuestBoard, everyone_counts};

// Per-player quest state. For `everyone` quests `completed` means the GROUP
// completed; a player at the threshold stays `completed: false` until the
// last player gets there.
#[derive(Debug, Clone)]
pub struct QuestState {
    pub progress: u32,
    pub completed: bool,
}

pub enum QuestEvent<'a> {
    CookieCollected,
    ActorKilled { kind: &'a str },
    FireworksStarted,
}

impl QuestEvent<'_> {
    fn matches(&self, quest: &Quest) -> bool {
        match (quest.kind, self) {
            (QuestKind::Cookies, Self::CookieCollected) => true,
            (QuestKind::ActorKills, Self::ActorKilled { kind }) => {
                quest.actor_kind.as_deref().is_none_or(|want| want == *kind)
            }
            (QuestKind::Fireworks, Self::FireworksStarted) => true,
            _ => false,
        }
    }
}

// Every quest event goes through here: progress, completion, scoring, the
// per-player messages, feed lines, and unlocking dependent quests. `actor` is
// the player who caused the event — `None` for world events such as the
// firework launch, which only `shared` quests consume (config validation).
pub fn record_quest_event(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    config: &ServerGameplayConfig,
    actor: Option<PlayerId>,
    event: QuestEvent,
) {
    // Decide the affected quests up front so a quest this event unlocks can't
    // also consume it.
    let matching: Vec<&Quest> = config
        .quests
        .iter()
        .filter(|quest| event.matches(quest) && board.is_unlocked(&quest.id) && !board.is_completed(&quest.id))
        .collect();
    for quest in matching {
        match quest.scope {
            QuestScope::Individual => {
                let actor = actor.expect("player-less quest event on an individual quest");
                advance_individual(players, config, actor, quest);
            }
            QuestScope::Everyone => {
                let actor = actor.expect("player-less quest event on an everyone quest");
                advance_everyone(players, board, config, actor, quest);
            }
            QuestScope::Shared => advance_shared(players, board, config, quest),
        }
    }
}

fn advance_individual(players: &mut PlayerMap, config: &ServerGameplayConfig, actor: PlayerId, quest: &Quest) {
    let Some(info) = players.get_mut(&actor) else {
        return;
    };
    let Some(state) = info.quest_states.get_mut(&quest.id) else {
        return;
    };
    if state.completed {
        return;
    }
    state.progress = state.progress.saturating_add(1);
    let progress = state.progress;
    let completed = progress >= quest.threshold;
    state.completed = completed;
    if !completed {
        send(info, progress_message(quest, progress));
        return;
    }
    info.score += quest_points(config, quest);
    send(info, completed_message(quest));
    announce(
        players,
        &config.feed,
        FeedEvent::QuestCompleted {
            name: players.display_name(&actor),
            title: quest.title.clone(),
        },
    );
}

fn advance_everyone(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    config: &ServerGameplayConfig,
    actor: PlayerId,
    quest: &Quest,
) {
    let reached = {
        let Some(info) = players.get_mut(&actor) else {
            return;
        };
        let Some(state) = info.quest_states.get_mut(&quest.id) else {
            return;
        };
        if state.completed || state.progress >= quest.threshold {
            return;
        }
        state.progress = state.progress.saturating_add(1);
        let progress = state.progress;
        send(info, progress_message(quest, progress));
        progress >= quest.threshold
    };
    if !reached {
        return;
    }
    let (players_done, logged_in) = everyone_counts(players, quest);
    if logged_in > 0 && players_done >= logged_in {
        complete_group(players, board, config, quest);
    } else {
        announce(
            players,
            &config.feed,
            FeedEvent::QuestPartDone {
                name: players.display_name(&actor),
                title: quest.title.clone(),
                players_done,
                players: logged_in,
            },
        );
    }
}

fn advance_shared(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig, quest: &Quest) {
    let state = board.state_mut(&quest.id);
    state.shared_progress = state.shared_progress.saturating_add(1);
    if state.shared_progress >= quest.threshold {
        complete_group(players, board, config, quest);
    }
}

// Latch the completion, credit every logged-in player, tell everyone, and
// open whatever this quest was gating.
fn complete_group(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig, quest: &Quest) {
    board.state_mut(&quest.id).completed = true;
    let points = quest_points(config, quest);
    for (_, info) in players.iter_mut() {
        if !info.logged_in {
            continue;
        }
        if let Some(state) = info.quest_states.get_mut(&quest.id) {
            state.progress = quest.threshold;
            state.completed = true;
        }
        info.score += points;
    }
    broadcast_to_all(players, completed_message(quest));
    announce(
        players,
        &config.feed,
        FeedEvent::GroupQuestCompleted {
            title: quest.title.clone(),
        },
    );
    unlock_dependents(players, board, config, &quest.id);
}

fn unlock_dependents(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    config: &ServerGameplayConfig,
    completed: &QuestId,
) {
    for (index, quest) in config.quests.iter().enumerate() {
        if quest.requires.as_ref() != Some(completed) || board.is_unlocked(&quest.id) {
            continue;
        }
        board.unlock(&quest.id);
        for (_, info) in players.iter_mut() {
            if !info.logged_in {
                continue;
            }
            if let Some(new_quest) = assign_quest(info, quest, index as u32, board) {
                send(
                    info,
                    ServerMessage::QuestsAssigned(SQuestsAssigned {
                        quests: vec![new_quest],
                    }),
                );
            }
        }
    }
}

// Every unlocked quest the player doesn't have yet. Completed group quests
// arrive completed (latched; no points for late joiners); shared quests
// arrive at the pooled progress.
pub fn assign_quests(player_info: &mut PlayerInfo, quests: &[Quest], board: &QuestBoard) -> Option<SQuestsAssigned> {
    let new_quests: Vec<NewQuest> = quests
        .iter()
        .enumerate()
        .filter(|(_, quest)| board.is_unlocked(&quest.id))
        .filter_map(|(index, quest)| assign_quest(player_info, quest, index as u32, board))
        .collect();
    (!new_quests.is_empty()).then_some(SQuestsAssigned { quests: new_quests })
}

fn assign_quest(player_info: &mut PlayerInfo, quest: &Quest, order: u32, board: &QuestBoard) -> Option<NewQuest> {
    if player_info.quest_states.contains_key(&quest.id) {
        return None;
    }
    let completed = board.is_completed(&quest.id);
    let progress = if completed {
        quest.threshold
    } else if quest.scope == QuestScope::Shared {
        board.shared_progress(&quest.id)
    } else {
        0
    };
    player_info
        .quest_states
        .insert(quest.id.clone(), QuestState { progress, completed });
    Some(NewQuest {
        id: quest.id.clone(),
        scope: quest.scope,
        title: quest.title.clone(),
        description: quest.description.clone(),
        progress,
        threshold: quest.threshold,
        order,
    })
}

// A leaver may have been the last holdout of an `everyone` quest.
pub fn player_left(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig) {
    if !players.iter().any(|(_, info)| info.logged_in) {
        return;
    }
    for quest in &config.quests {
        if quest.scope != QuestScope::Everyone || !board.is_unlocked(&quest.id) || board.is_completed(&quest.id) {
            continue;
        }
        let (players_done, logged_in) = everyone_counts(players, quest);
        if logged_in > 0 && players_done >= logged_in {
            complete_group(players, board, config, quest);
        }
    }
}

fn quest_points(config: &ServerGameplayConfig, quest: &Quest) -> i32 {
    config
        .scoring
        .quest_completed
        .get(&quest.id.0)
        .copied()
        .expect("quest id missing from scoring.quest_completed")
}

fn progress_message(quest: &Quest, progress: u32) -> ServerMessage {
    ServerMessage::QuestProgress(SQuestProgress {
        id: quest.id.clone(),
        progress,
    })
}

fn completed_message(quest: &Quest) -> ServerMessage {
    ServerMessage::QuestCompleted(SQuestCompleted {
        id: quest.id.clone(),
        completed_text: quest.completed_text.clone(),
    })
}

fn send(info: &PlayerInfo, message: ServerMessage) {
    let _ = info.channel.send(ServerToClient::Send(message));
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::*;
    use crate::{config::QuestKind, network::ServerToClient};
    use common::protocol::SFeed;

    fn quest(id: &str, kind: QuestKind, scope: QuestScope, threshold: u32, requires: Option<&str>) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind,
            scope,
            requires: requires.map(|required| QuestId(required.to_owned())),
            actor_kind: None,
            threshold,
            title: id.to_owned(),
            description: format!("do {id}"),
            completed_text: format!("{id} done"),
        }
    }

    fn config(quests: Vec<Quest>) -> ServerGameplayConfig {
        let mut config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        config.scoring.quest_completed = quests.iter().map(|quest| (quest.id.0.clone(), 100)).collect();
        config.quests = quests;
        config
    }

    fn join(
        players: &mut PlayerMap,
        id: u32,
        config: &ServerGameplayConfig,
        board: &QuestBoard,
    ) -> UnboundedReceiver<ServerToClient> {
        let (tx, rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.logged_in = true;
        info.name = format!("P{id}");
        assign_quests(&mut info, &config.quests, board);
        players.insert(PlayerId(id), info);
        rx
    }

    fn drain(receiver: &mut UnboundedReceiver<ServerToClient>) -> Vec<ServerMessage> {
        let mut messages = Vec::new();
        while let Ok(envelope) = receiver.try_recv() {
            if let ServerToClient::Send(msg) = envelope {
                messages.push(msg);
            }
        }
        messages
    }

    fn completed(messages: &[ServerMessage], id: &str) -> bool {
        messages
            .iter()
            .any(|msg| matches!(msg, ServerMessage::QuestCompleted(c) if c.id.0 == id))
    }

    fn progress_values(messages: &[ServerMessage], id: &str) -> Vec<u32> {
        messages
            .iter()
            .filter_map(|msg| match msg {
                ServerMessage::QuestProgress(p) if p.id.0 == id => Some(p.progress),
                _ => None,
            })
            .collect()
    }

    fn feed_events(messages: &[ServerMessage]) -> Vec<FeedEvent> {
        messages
            .iter()
            .filter_map(|msg| match msg {
                ServerMessage::Feed(SFeed { event }) => Some(event.clone()),
                _ => None,
            })
            .collect()
    }

    fn assigned_ids(messages: &[ServerMessage]) -> Vec<String> {
        messages
            .iter()
            .filter_map(|msg| match msg {
                ServerMessage::QuestsAssigned(assigned) => Some(assigned.quests.iter().map(|q| q.id.0.clone())),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn cookie(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig, id: u32) {
        record_quest_event(players, board, config, Some(PlayerId(id)), QuestEvent::CookieCollected);
    }

    fn score(players: &PlayerMap, id: u32) -> i32 {
        players.get(&PlayerId(id)).expect("player tracked").score
    }

    fn state(players: &PlayerMap, id: u32, quest: &str) -> QuestState {
        players.get(&PlayerId(id)).expect("player tracked").quest_states[&QuestId(quest.to_owned())].clone()
    }

    #[test]
    fn individual_progress_and_completion_stay_per_player() {
        let config = config(vec![quest("gold", QuestKind::Cookies, QuestScope::Individual, 2, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut bob = join(&mut players, 2, &config, &board);

        cookie(&mut players, &mut board, &config, 1);
        cookie(&mut players, &mut board, &config, 1);

        let alice_messages = drain(&mut alice);
        assert_eq!(progress_values(&alice_messages, "gold"), [1]);
        assert!(completed(&alice_messages, "gold"));
        assert_eq!(score(&players, 1), 100);
        let bob_messages = drain(&mut bob);
        assert!(!completed(&bob_messages, "gold"));
        assert!(
            matches!(feed_events(&bob_messages).as_slice(), [FeedEvent::QuestCompleted { name, .. }] if name == "P1")
        );
        assert_eq!(state(&players, 2, "gold").progress, 0);
        assert_eq!(score(&players, 2), 0);
    }

    #[test]
    fn shared_quest_pools_progress_and_scores_everyone_once() {
        let config = config(vec![quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 2, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut bob = join(&mut players, 2, &config, &board);
        let kill = |players: &mut PlayerMap, board: &mut QuestBoard, id: u32| {
            record_quest_event(
                players,
                board,
                &config,
                Some(PlayerId(id)),
                QuestEvent::ActorKilled { kind: "sentry" },
            );
        };

        kill(&mut players, &mut board, 1);
        assert_eq!(board.shared_progress(&QuestId("hunt".to_owned())), 1);
        assert!(!board.is_completed(&QuestId("hunt".to_owned())));

        kill(&mut players, &mut board, 2);
        assert!(board.is_completed(&QuestId("hunt".to_owned())));
        for rx in [&mut alice, &mut bob] {
            let messages = drain(rx);
            assert!(completed(&messages, "hunt"));
            assert!(
                progress_values(&messages, "hunt").is_empty(),
                "shared progress rides the snapshot only"
            );
            assert!(matches!(
                feed_events(&messages).as_slice(),
                [FeedEvent::GroupQuestCompleted { .. }]
            ));
        }
        assert_eq!((score(&players, 1), score(&players, 2)), (100, 100));

        kill(&mut players, &mut board, 1);
        assert!(drain(&mut alice).is_empty(), "latched: nothing after completion");
        assert_eq!(score(&players, 1), 100);
    }

    #[test]
    fn everyone_quest_completes_when_the_last_player_reaches_the_threshold() {
        let config = config(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut bob = join(&mut players, 2, &config, &board);

        cookie(&mut players, &mut board, &config, 1);
        let alice_messages = drain(&mut alice);
        assert_eq!(progress_values(&alice_messages, "gold"), [1]);
        assert!(!completed(&alice_messages, "gold"));
        assert!(!state(&players, 1, "gold").completed, "own part done, group not");
        assert!(matches!(
            feed_events(&drain(&mut bob)).as_slice(),
            [FeedEvent::QuestPartDone {
                players_done: 1,
                players: 2,
                ..
            }]
        ));

        cookie(&mut players, &mut board, &config, 2);
        for rx in [&mut alice, &mut bob] {
            let messages = drain(rx);
            assert!(completed(&messages, "gold"));
            let lines = feed_events(&messages);
            assert!(
                matches!(lines.as_slice(), [FeedEvent::GroupQuestCompleted { .. }]),
                "{lines:?}"
            );
        }
        assert!(state(&players, 1, "gold").completed);
        assert_eq!((score(&players, 1), score(&players, 2)), (100, 100));
    }

    #[test]
    fn late_joiner_raises_the_denominator() {
        let config = config(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let _bob = join(&mut players, 2, &config, &board);
        cookie(&mut players, &mut board, &config, 1);
        drain(&mut alice);

        let _carol = join(&mut players, 3, &config, &board);
        cookie(&mut players, &mut board, &config, 2);
        assert!(!board.is_completed(&QuestId("gold".to_owned())));
        assert!(matches!(
            feed_events(&drain(&mut alice)).as_slice(),
            [FeedEvent::QuestPartDone {
                players_done: 2,
                players: 3,
                ..
            }]
        ));

        cookie(&mut players, &mut board, &config, 3);
        assert!(board.is_completed(&QuestId("gold".to_owned())));
    }

    #[test]
    fn player_left_completes_when_the_leaver_was_the_holdout() {
        let config = config(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let _bob = join(&mut players, 2, &config, &board);
        cookie(&mut players, &mut board, &config, 1);
        drain(&mut alice);

        players.remove(&PlayerId(2));
        player_left(&mut players, &mut board, &config);

        assert!(board.is_completed(&QuestId("gold".to_owned())));
        assert!(completed(&drain(&mut alice), "gold"));
        assert_eq!(score(&players, 1), 100);
    }

    #[test]
    fn player_left_on_an_empty_server_completes_nothing() {
        let config = config(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);
        players.remove(&PlayerId(1));

        player_left(&mut players, &mut board, &config);

        assert!(!board.is_completed(&QuestId("gold".to_owned())));
    }

    #[test]
    fn group_completion_unlocks_dependents_and_assigns_them_to_everyone() {
        let config = config(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None),
            quest("fireworks", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
        ]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut bob = join(&mut players, 2, &config, &board);
        assert!(
            !players
                .get(&PlayerId(1))
                .expect("alice")
                .quest_states
                .contains_key(&QuestId("fireworks".to_owned()))
        );

        cookie(&mut players, &mut board, &config, 1);
        cookie(&mut players, &mut board, &config, 2);

        assert!(board.is_unlocked(&QuestId("fireworks".to_owned())));
        for rx in [&mut alice, &mut bob] {
            assert_eq!(assigned_ids(&drain(rx)), ["fireworks"]);
        }
        assert!(
            players
                .get(&PlayerId(1))
                .expect("alice")
                .quest_states
                .contains_key(&QuestId("fireworks".to_owned()))
        );

        // A late joiner gets the completed prerequisite as completed and the unlocked quest fresh.
        let mut carol = join(&mut players, 3, &config, &board);
        let messages = drain(&mut carol);
        assert!(
            messages.is_empty(),
            "assignment happens through the login path, not here"
        );
        let gold = state(&players, 3, "gold");
        assert!(gold.completed);
        assert_eq!(gold.progress, 1);
        assert_eq!(state(&players, 3, "fireworks").progress, 0);
        assert_eq!(score(&players, 3), 0, "no points for late joiners");
    }

    #[test]
    fn the_unlocking_event_does_not_feed_the_quest_it_unlocked() {
        let config = config(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None),
            quest("bonus", QuestKind::Cookies, QuestScope::Shared, 1, Some("gold")),
        ]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);

        cookie(&mut players, &mut board, &config, 1);

        assert!(board.is_completed(&QuestId("gold".to_owned())));
        assert!(board.is_unlocked(&QuestId("bonus".to_owned())));
        assert_eq!(board.shared_progress(&QuestId("bonus".to_owned())), 0);
    }

    #[test]
    fn actor_kill_respects_kind_filter() {
        let mut sentries = quest("sentries", QuestKind::ActorKills, QuestScope::Individual, 2, None);
        sentries.actor_kind = Some("sentry".to_owned());
        let config = config(vec![sentries]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);

        record_quest_event(
            &mut players,
            &mut board,
            &config,
            Some(PlayerId(1)),
            QuestEvent::ActorKilled { kind: "zapper" },
        );
        assert_eq!(state(&players, 1, "sentries").progress, 0);
        record_quest_event(
            &mut players,
            &mut board,
            &config,
            Some(PlayerId(1)),
            QuestEvent::ActorKilled { kind: "sentry" },
        );
        assert_eq!(state(&players, 1, "sentries").progress, 1);
    }

    #[test]
    fn fireworks_event_only_hits_fireworks_quests() {
        let config = config(vec![
            quest("fireworks", QuestKind::Fireworks, QuestScope::Shared, 1, None),
            quest("gold", QuestKind::Cookies, QuestScope::Individual, 5, None),
        ]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);

        record_quest_event(&mut players, &mut board, &config, None, QuestEvent::FireworksStarted);

        assert!(board.is_completed(&QuestId("fireworks".to_owned())));
        assert_eq!(state(&players, 1, "gold").progress, 0);
    }

    #[test]
    fn assign_quests_skips_locked_and_seeds_completed_group_quests() {
        let config = config(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 3, None),
            quest("fireworks", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
            quest("later", QuestKind::Cookies, QuestScope::Shared, 1, Some("fireworks")),
        ]);
        let mut board = QuestBoard::from_quests(&config.quests);
        board.state_mut(&QuestId("gold".to_owned())).completed = true;
        board.unlock(&QuestId("fireworks".to_owned()));
        let (tx, _rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);

        let assigned = assign_quests(&mut info, &config.quests, &board).expect("something to assign");

        let ids: Vec<&str> = assigned.quests.iter().map(|q| q.id.0.as_str()).collect();
        assert_eq!(ids, ["gold", "fireworks"]);
        assert_eq!(
            assigned.quests[0].progress, 3,
            "completed group quests arrive at the threshold"
        );
        assert_eq!(assigned.quests[1].order, 1, "order is the catalog index");
        assert!(info.quest_states[&QuestId("gold".to_owned())].completed);
        assert!(assign_quests(&mut info, &config.quests, &board).is_none(), "idempotent");
    }
}
