use std::collections::{BTreeSet, HashMap};

use bevy::prelude::Resource;

use crate::{config::Quest, players::PlayerMap};
use common::protocol::{PlatePurpose, QuestGroupProgress, QuestGroupStatus, QuestId, QuestScope};

use super::QuestCatalog;

#[derive(Debug, Clone)]
enum QuestRuntimeState {
    Individual {
        unlocked: bool,
    },
    Shared {
        unlocked: bool,
        completed: bool,
        progress: u32,
    },
    Everyone {
        unlocked: bool,
        completed: bool,
    },
}

impl QuestRuntimeState {
    const fn new(scope: QuestScope, unlocked: bool) -> Self {
        match scope {
            QuestScope::Individual => Self::Individual { unlocked },
            QuestScope::Shared => Self::Shared {
                unlocked,
                completed: false,
                progress: 0,
            },
            QuestScope::Everyone => Self::Everyone {
                unlocked,
                completed: false,
            },
        }
    }

    const fn is_unlocked(&self) -> bool {
        match self {
            Self::Individual { unlocked } | Self::Shared { unlocked, .. } | Self::Everyone { unlocked, .. } => {
                *unlocked
            }
        }
    }

    fn unlock(&mut self) {
        match self {
            Self::Individual { unlocked } | Self::Shared { unlocked, .. } | Self::Everyone { unlocked, .. } => {
                *unlocked = true;
            }
        }
    }

    const fn is_completed(&self) -> bool {
        match self {
            Self::Shared { completed, .. } | Self::Everyone { completed, .. } => *completed,
            Self::Individual { .. } => false,
        }
    }
}

// Session-wide quest state: which quests are unlocked, which group quests
// completed, and the pooled progress of `shared` quests. Own progress per
// player stays on `PlayerInfo.session.quest_states`.
#[derive(Resource, Debug)]
pub struct QuestBoard {
    states: HashMap<QuestId, QuestRuntimeState>,
    // Quests whose plates are only live once they unlock.
    claims: Vec<(QuestId, PlatePurpose)>,
    // Cached from `claims` + `unlocked`; refreshed on every unlock.
    locked_plate_purposes: Vec<PlatePurpose>,
}

impl QuestBoard {
    #[must_use]
    pub fn from_catalog(catalog: &QuestCatalog) -> Self {
        let states = catalog
            .iter()
            .map(|quest| {
                (
                    quest.id.clone(),
                    QuestRuntimeState::new(quest.scope, quest.requires.is_none()),
                )
            })
            .collect();
        let claims = catalog
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

    fn state(&self, id: &QuestId) -> &QuestRuntimeState {
        self.states.get(id).expect("quest id missing from QuestBoard")
    }

    fn state_mut(&mut self, id: &QuestId) -> &mut QuestRuntimeState {
        self.states.get_mut(id).expect("quest id missing from QuestBoard")
    }

    pub fn unlock(&mut self, id: &QuestId) {
        self.state_mut(id).unlock();
        self.refresh_locked_plate_purposes();
    }

    pub fn finish_group(&mut self, quest: &Quest) -> bool {
        match self.state_mut(&quest.id) {
            QuestRuntimeState::Shared {
                completed, progress, ..
            } => {
                if *completed {
                    return false;
                }
                *progress = quest.threshold;
                *completed = true;
            }
            QuestRuntimeState::Everyone { completed, .. } => {
                if *completed {
                    return false;
                }
                *completed = true;
            }
            QuestRuntimeState::Individual { .. } => panic!("individual quest passed to finish_group"),
        }
        true
    }

    pub fn add_shared_progress(&mut self, quest: &Quest) -> u32 {
        let QuestRuntimeState::Shared { progress, .. } = self.state_mut(&quest.id) else {
            panic!("non-shared quest passed to add_shared_progress");
        };
        *progress = progress.saturating_add(1).min(quest.threshold);
        *progress
    }

    #[must_use]
    pub fn is_unlocked(&self, id: &QuestId) -> bool {
        self.state(id).is_unlocked()
    }

    #[must_use]
    pub fn is_completed(&self, id: &QuestId) -> bool {
        self.state(id).is_completed()
    }

    #[must_use]
    pub fn shared_progress(&self, id: &QuestId) -> u32 {
        match self.state(id) {
            QuestRuntimeState::Shared { progress, .. } => *progress,
            QuestRuntimeState::Individual { .. } | QuestRuntimeState::Everyone { .. } => 0,
        }
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
    pub fn group_statuses(&self, catalog: &QuestCatalog, players: &PlayerMap) -> Vec<QuestGroupStatus> {
        catalog
            .iter()
            .filter_map(|quest| {
                if !self.is_unlocked(&quest.id) {
                    return None;
                }
                let progress = match self.state(&quest.id) {
                    QuestRuntimeState::Everyone { .. } => {
                        let count = everyone_count(players, quest);
                        QuestGroupProgress::Everyone {
                            players_done: count.players_done,
                            players_total: count.players_total,
                        }
                    }
                    QuestRuntimeState::Shared { progress, .. } => QuestGroupProgress::Shared { progress: *progress },
                    QuestRuntimeState::Individual { .. } => return None,
                };
                Some(QuestGroupStatus {
                    id: quest.id.clone(),
                    completed: self.is_completed(&quest.id),
                    progress,
                })
            })
            .collect()
    }

    // Everything a snapshot carries about quests.
    #[must_use]
    pub fn snapshot_fields(
        &self,
        catalog: &QuestCatalog,
        players: &PlayerMap,
    ) -> (Vec<QuestGroupStatus>, Vec<PlatePurpose>) {
        (
            self.group_statuses(catalog, players),
            self.locked_plate_purposes.clone(),
        )
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

// Active players at the threshold of an `everyone` quest, and the active total.
pub(super) fn everyone_count(players: &PlayerMap, quest: &Quest) -> EveryoneCount {
    let mut count = EveryoneCount {
        players_done: 0,
        players_total: 0,
    };
    for (_, info) in players.iter() {
        if !info.connection.is_active() {
            continue;
        }
        count.players_total += 1;
        if info
            .session
            .quest_states
            .get(&quest.id)
            .and_then(|state| state.own_progress())
            .unwrap_or(0)
            >= quest.threshold
        {
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
        players::PlayerQuestState,
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
        let quest_catalog = QuestCatalog::from_config(&config);
        let board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);
        let _bob = join(&mut players, 2, &quest_catalog, &board);
        players
            .get_mut(&PlayerId(1))
            .expect("alice")
            .session
            .quest_states
            .insert(QuestId("gold".to_owned()), PlayerQuestState::Everyone { progress: 2 });

        let statuses = board.group_statuses(&quest_catalog, &players);

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
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        assert_eq!(board.locked_plate_purposes(), [PlatePurpose::Firework]);
        board.unlock(&QuestId("show".to_owned()));
        assert!(board.locked_plate_purposes().is_empty());

        let unclaimed_config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let unclaimed_catalog = QuestCatalog::from_config(&unclaimed_config);
        let unclaimed = QuestBoard::from_catalog(&unclaimed_catalog);
        assert!(unclaimed.locked_plate_purposes().is_empty());
    }

    #[test]
    fn shipped_catalog_locks_firework_plates_until_the_fireworks_quest_unlocks() {
        let mut config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        config.default_map = "hotel".to_owned();
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        assert_eq!(board.locked_plate_purposes(), [PlatePurpose::Firework]);
        board.unlock(&QuestId("start_fireworks".to_owned()));
        assert!(board.locked_plate_purposes().is_empty());
    }
}
