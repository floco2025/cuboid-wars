use std::collections::{BTreeSet, HashMap};

use bevy::prelude::Resource;

use crate::{config::Quest, players::PlayerMap};
use common::protocol::{PlatePurpose, QuestGroupProgress, QuestGroupStatus, QuestId, QuestScope};

#[derive(Debug, Clone, Default)]
struct GroupQuestState {
    unlocked: bool,
    // Latched for the session once set.
    completed: bool,
    // `Shared` quests only.
    shared_progress: u32,
}

// Session-wide quest state: which quests are unlocked, which group quests
// completed, and the pooled progress of `shared` quests. Own progress per
// player stays on `PlayerInfo.quest_states`.
#[derive(Resource, Debug)]
pub struct QuestBoard {
    states: HashMap<QuestId, GroupQuestState>,
    // Quests whose plates are only live once they unlock.
    claims: Vec<(QuestId, PlatePurpose)>,
    // Cached from `claims` + `unlocked`; refreshed on every unlock.
    locked_plate_purposes: Vec<PlatePurpose>,
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
        let claims = quests
            .iter()
            .filter_map(|quest| quest.kind.plate_purpose().map(|purpose| (quest.id.clone(), purpose)))
            .collect();
        let mut board = Self {
            states,
            claims,
            locked_plate_purposes: Vec::new(),
        };
        board.refresh_locked_plate_purposes();
        board
    }

    fn state(&self, id: &QuestId) -> &GroupQuestState {
        self.states.get(id).expect("quest id missing from QuestBoard")
    }

    fn state_mut(&mut self, id: &QuestId) -> &mut GroupQuestState {
        self.states.get_mut(id).expect("quest id missing from QuestBoard")
    }

    pub fn unlock(&mut self, id: &QuestId) {
        self.state_mut(id).unlocked = true;
        self.refresh_locked_plate_purposes();
    }

    pub fn latch_completed(&mut self, id: &QuestId) {
        self.state_mut(id).completed = true;
    }

    pub fn add_shared_progress(&mut self, id: &QuestId) -> u32 {
        let state = self.state_mut(id);
        state.shared_progress = state.shared_progress.saturating_add(1);
        state.shared_progress
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

    // Plate purposes still locked: a claimed purpose waits for one of its
    // claiming quests to unlock; unclaimed purposes are never locked.
    #[must_use]
    pub fn locked_plate_purposes(&self) -> &[PlatePurpose] {
        &self.locked_plate_purposes
    }

    fn refresh_locked_plate_purposes(&mut self) {
        let claimed: BTreeSet<PlatePurpose> = self.claims.iter().map(|(_, purpose)| *purpose).collect();
        self.locked_plate_purposes = claimed
            .into_iter()
            .filter(|purpose| {
                !self
                    .claims
                    .iter()
                    .any(|(id, claimed)| claimed == purpose && self.is_unlocked(id))
            })
            .collect();
    }

    // Group status of every unlocked group-scoped quest.
    #[must_use]
    pub fn group_statuses(&self, quests: &[Quest], players: &PlayerMap) -> Vec<QuestGroupStatus> {
        quests
            .iter()
            .filter(|quest| quest.scope.is_group() && self.is_unlocked(&quest.id))
            .map(|quest| {
                let progress = match quest.scope {
                    QuestScope::Everyone => {
                        let count = everyone_count(players, quest);
                        QuestGroupProgress::Everyone {
                            players_done: count.players_done,
                            players_total: count.players_total,
                        }
                    }
                    QuestScope::Individual | QuestScope::Shared => QuestGroupProgress::Shared {
                        progress: self.shared_progress(&quest.id),
                    },
                };
                QuestGroupStatus {
                    id: quest.id.clone(),
                    completed: self.is_completed(&quest.id),
                    progress,
                }
            })
            .collect()
    }

    // Everything a snapshot carries about quests.
    #[must_use]
    pub fn snapshot_fields(&self, quests: &[Quest], players: &PlayerMap) -> (Vec<QuestGroupStatus>, Vec<PlatePurpose>) {
        (self.group_statuses(quests, players), self.locked_plate_purposes.clone())
    }
}

pub(super) struct EveryoneCount {
    pub(super) players_done: u32,
    pub(super) players_total: u32,
}

impl EveryoneCount {
    pub(super) fn all_done(&self) -> bool {
        self.players_total > 0 && self.players_done >= self.players_total
    }
}

// Logged-in players at the threshold of an `everyone` quest, and how many
// are logged in at all.
pub(super) fn everyone_count(players: &PlayerMap, quest: &Quest) -> EveryoneCount {
    let mut count = EveryoneCount {
        players_done: 0,
        players_total: 0,
    };
    for (_, info) in players.iter() {
        if !info.logged_in {
            continue;
        }
        count.players_total += 1;
        if info.quest_states.get(&quest.id).copied().unwrap_or(0) >= quest.threshold {
            count.players_done += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{QuestKind, ServerGameplayConfig},
        quests::test_support::{catalog, join, quest},
    };
    use common::protocol::PlayerId;

    #[test]
    fn group_statuses_list_only_unlocked_group_quests() {
        let config = catalog(vec![
            quest("solo", QuestKind::Cookies, QuestScope::Individual, 3, None),
            quest("pool", QuestKind::ActorKills, QuestScope::Shared, 4, None),
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 2, None),
            quest("later", QuestKind::Cookies, QuestScope::Shared, 1, Some("gold")),
        ]);
        let board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);
        let _bob = join(&mut players, 2, &config, &board);
        players
            .get_mut(&PlayerId(1))
            .expect("alice")
            .quest_states
            .insert(QuestId("gold".to_owned()), 2);

        let statuses = board.group_statuses(&config.quests, &players);

        let ids: Vec<&str> = statuses.iter().map(|status| status.id.0.as_str()).collect();
        assert_eq!(ids, ["pool", "gold"]);
        assert_eq!(statuses[0].progress, QuestGroupProgress::Shared { progress: 0 });
        assert_eq!(
            statuses[1].progress,
            QuestGroupProgress::Everyone {
                players_done: 1,
                players_total: 2
            }
        );
        assert!(!statuses[1].completed);
    }

    #[test]
    fn locked_plate_purposes_follow_the_claiming_quests() {
        let config = catalog(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None),
            quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
        ]);
        let mut board = QuestBoard::from_quests(&config.quests);
        assert_eq!(board.locked_plate_purposes(), [PlatePurpose::Firework]);
        board.unlock(&QuestId("show".to_owned()));
        assert!(board.locked_plate_purposes().is_empty());

        let unclaimed = QuestBoard::from_quests(
            &catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]).quests,
        );
        assert!(unclaimed.locked_plate_purposes().is_empty());
    }

    #[test]
    fn shipped_catalog_locks_firework_plates_until_the_fireworks_quest_unlocks() {
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let mut board = QuestBoard::from_quests(&config.quests);
        assert_eq!(board.locked_plate_purposes(), [PlatePurpose::Firework]);
        board.unlock(&QuestId("start_fireworks".to_owned()));
        assert!(board.locked_plate_purposes().is_empty());
    }
}
