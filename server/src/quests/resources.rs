use std::collections::{BTreeSet, HashMap};

use bevy::prelude::Resource;

use crate::{config::Quest, players::PlayerMap};
use common::protocol::{PlatePurpose, QuestGroupStatus, QuestId, QuestScope};

#[derive(Debug, Clone, Default)]
pub struct GroupQuestState {
    pub unlocked: bool,
    // Latched for the session once set.
    pub completed: bool,
    // `Shared` quests only.
    pub shared_progress: u32,
}

// Session-wide quest state: which quests are unlocked, which group quests
// completed, and the pooled progress of `shared` quests. Per-player progress
// stays on `PlayerInfo.quest_states`.
#[derive(Resource, Debug)]
pub struct QuestBoard {
    states: HashMap<QuestId, GroupQuestState>,
}

impl QuestBoard {
    #[must_use]
    pub fn from_quests(quests: &[Quest]) -> Self {
        let states = quests
            .iter()
            .map(|quest| {
                let state = GroupQuestState {
                    unlocked: quest.requires.is_none(),
                    ..GroupQuestState::default()
                };
                (quest.id.clone(), state)
            })
            .collect();
        Self { states }
    }

    fn state(&self, id: &QuestId) -> &GroupQuestState {
        self.states.get(id).expect("quest id missing from QuestBoard")
    }

    pub(super) fn state_mut(&mut self, id: &QuestId) -> &mut GroupQuestState {
        self.states.get_mut(id).expect("quest id missing from QuestBoard")
    }

    pub fn unlock(&mut self, id: &QuestId) {
        self.state_mut(id).unlocked = true;
    }

    #[must_use]
    pub fn is_unlocked(&self, id: &QuestId) -> bool {
        self.state(id).unlocked
    }

    #[must_use]
    pub fn is_completed(&self, id: &QuestId) -> bool {
        self.state(id).completed
    }

    #[must_use]
    pub fn shared_progress(&self, id: &QuestId) -> u32 {
        self.state(id).shared_progress
    }

    // Plate purposes still locked: a purpose claimed by a quest (the plates
    // that solve it) waits for one of those quests to unlock; unclaimed
    // purposes are never locked. Sorted so snapshots diff stably.
    #[must_use]
    pub fn locked_plate_purposes(&self, quests: &[Quest]) -> Vec<PlatePurpose> {
        let claimed: BTreeSet<PlatePurpose> = quests.iter().filter_map(|quest| quest.kind.plate_purpose()).collect();
        claimed
            .into_iter()
            .filter(|purpose| {
                !quests
                    .iter()
                    .any(|quest| quest.kind.plate_purpose() == Some(*purpose) && self.is_unlocked(&quest.id))
            })
            .collect()
    }

    // Group status of every unlocked group-scoped quest, for the snapshot.
    #[must_use]
    pub fn snapshot(&self, quests: &[Quest], players: &PlayerMap) -> Vec<QuestGroupStatus> {
        quests
            .iter()
            .filter(|quest| quest.scope != QuestScope::Individual && self.is_unlocked(&quest.id))
            .map(|quest| {
                let (players_done, players) = match quest.scope {
                    QuestScope::Everyone => everyone_counts(players, quest),
                    QuestScope::Individual | QuestScope::Shared => (0, 0),
                };
                QuestGroupStatus {
                    id: quest.id.clone(),
                    completed: self.is_completed(&quest.id),
                    shared_progress: self.shared_progress(&quest.id),
                    players_done,
                    players,
                }
            })
            .collect()
    }
}

// (players at the threshold, players logged in) for an `everyone` quest.
#[must_use]
pub fn everyone_counts(players: &PlayerMap, quest: &Quest) -> (u32, u32) {
    let mut done = 0;
    let mut logged_in = 0;
    for (_, info) in players.iter() {
        if !info.logged_in {
            continue;
        }
        logged_in += 1;
        let reached = info
            .quest_states
            .get(&quest.id)
            .is_some_and(|state| state.completed || state.progress >= quest.threshold);
        if reached {
            done += 1;
        }
    }
    (done, logged_in)
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;
    use crate::{
        config::ServerGameplayConfig,
        players::PlayerInfo,
        quests::{QuestState, assign_quests},
    };

    fn logged_in(players: &mut PlayerMap, id: u32, config: &ServerGameplayConfig, board: &QuestBoard) {
        let (tx, _rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.logged_in = true;
        assign_quests(&mut info, &config.quests, board);
        players.insert(common::protocol::PlayerId(id), info);
    }

    #[test]
    fn snapshot_lists_only_unlocked_group_quests_with_counts() {
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        logged_in(&mut players, 1, &config, &board);
        logged_in(&mut players, 2, &config, &board);
        let gold = QuestId("collect_gold".to_owned());
        players
            .get_mut(&common::protocol::PlayerId(1))
            .expect("player 1")
            .quest_states
            .insert(
                gold.clone(),
                QuestState {
                    progress: 10,
                    completed: false,
                },
            );

        let statuses = board.snapshot(&config.quests, &players);

        // Exactly the unlocked group-scoped quests, in catalog order.
        let expected: Vec<&str> = config
            .quests
            .iter()
            .filter(|quest| quest.scope != QuestScope::Individual && quest.requires.is_none())
            .map(|quest| quest.id.0.as_str())
            .collect();
        let listed: Vec<&str> = statuses.iter().map(|status| status.id.0.as_str()).collect();
        assert_eq!(listed, expected);
        let gold_status = statuses.iter().find(|s| s.id == gold).expect("gold listed");
        assert_eq!((gold_status.players_done, gold_status.players), (1, 2));
        assert!(!gold_status.completed);
        assert!(!statuses.iter().any(|s| s.id.0 == "start_fireworks"));
        assert!(
            !statuses.iter().any(|s| s.id.0 == "destroy_mines"),
            "individual quests have no group status"
        );
    }

    #[test]
    fn locked_plate_purposes_follow_the_claiming_quests() {
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let mut board = QuestBoard::from_quests(&config.quests);
        assert_eq!(board.locked_plate_purposes(&config.quests), [PlatePurpose::Firework]);
        board.unlock(&QuestId("start_fireworks".to_owned()));
        assert!(board.locked_plate_purposes(&config.quests).is_empty());
        assert!(
            board.locked_plate_purposes(&[]).is_empty(),
            "unclaimed purposes are never locked"
        );
    }
}
