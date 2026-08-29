use crate::{
    config::{Quest, QuestKind, ScoringConfig},
    players::PlayerInfo,
};
use common::protocol::{NewQuest, SQuestCompleted, SQuestProgress, SQuestsAssigned, ServerMessage};

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

#[derive(Debug, Default)]
pub struct QuestOutcome {
    // Progress/completion events for the player who earned them.
    pub unicast: Vec<ServerMessage>,
    // Titles of quests completed by this event, for the everyone-sees-it feed line.
    pub completed_titles: Vec<String>,
}

pub fn record_quest_event(
    player_info: &mut PlayerInfo,
    quests: &[Quest],
    scoring: &ScoringConfig,
    event: QuestEvent,
) -> QuestOutcome {
    let mut outcome = QuestOutcome::default();
    for quest in quests {
        if !event.matches(quest) {
            continue;
        }
        let Some(state) = player_info.quest_states.get_mut(&quest.id) else {
            continue;
        };
        if state.completed {
            continue;
        }
        state.progress = state.progress.saturating_add(1);
        if state.progress >= quest.threshold {
            state.completed = true;
            player_info.score += scoring
                .quest_completed
                .get(&quest.id.0)
                .copied()
                .expect("quest id missing from scoring.quest_completed");
            outcome.unicast.push(ServerMessage::QuestCompleted(SQuestCompleted {
                id: quest.id.clone(),
                completed_text: quest.completed_text.clone(),
            }));
            outcome.completed_titles.push(quest.title.clone());
        } else {
            outcome.unicast.push(ServerMessage::QuestProgress(SQuestProgress {
                id: quest.id.clone(),
                progress: state.progress,
            }));
        }
    }
    outcome
}

pub fn assign_quests(player_info: &mut PlayerInfo, quests: &[Quest]) -> Option<SQuestsAssigned> {
    let mut new_quests = Vec::new();
    for (index, quest) in quests.iter().enumerate() {
        if player_info.quest_states.contains_key(&quest.id) {
            continue;
        }
        player_info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 0,
                completed: false,
            },
        );
        new_quests.push(NewQuest {
            id: quest.id.clone(),
            title: quest.title.clone(),
            description: quest.description.clone(),
            progress: 0,
            threshold: quest.threshold,
            order: index as u32,
        });
    }
    (!new_quests.is_empty()).then_some(SQuestsAssigned { quests: new_quests })
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;
    use common::protocol::QuestId;

    fn player_info() -> PlayerInfo {
        let (tx, _rx) = unbounded_channel();
        PlayerInfo::new(Entity::PLACEHOLDER, tx)
    }

    fn cookies_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::Cookies,
            actor_kind: None,
            threshold,
            title: "Gold".to_owned(),
            description: "collect gold".to_owned(),
            completed_text: "done".to_owned(),
        }
    }

    fn sentry_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::ActorKills,
            actor_kind: Some("sentry".to_owned()),
            threshold,
            title: "Hunt".to_owned(),
            description: "destroy sentries".to_owned(),
            completed_text: "hunted".to_owned(),
        }
    }

    fn fireworks_quest(id: &str) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::Fireworks,
            actor_kind: None,
            threshold: 1,
            title: "Sky".to_owned(),
            description: "light it up".to_owned(),
            completed_text: "boom".to_owned(),
        }
    }

    fn scoring(quest_ids: &[&str]) -> ScoringConfig {
        ScoringConfig {
            player_kill: 0,
            player_death: 0,
            cookie: 0,
            actor_hit: std::collections::HashMap::new(),
            actor_kill: std::collections::HashMap::new(),
            quest_completed: quest_ids.iter().map(|id| ((*id).to_owned(), 100)).collect(),
        }
    }

    fn seed_quest(info: &mut PlayerInfo, quest: &Quest) {
        info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 0,
                completed: false,
            },
        );
    }

    #[test]
    fn record_quest_event_increments_progress() {
        let quest = cookies_quest("collect_gold", 3);
        let mut info = player_info();
        seed_quest(&mut info, &quest);

        let messages = record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            &scoring(&[&quest.id.0]),
            QuestEvent::CookieCollected,
        );

        assert!(
            matches!(messages.unicast.as_slice(), [ServerMessage::QuestProgress(progress)] if progress.progress == 1 && progress.id == quest.id)
        );
        assert_eq!(info.quest_states[&quest.id].progress, 1);
        assert!(!info.quest_states[&quest.id].completed);
    }

    #[test]
    fn record_quest_event_completes_at_threshold_once() {
        let quest = cookies_quest("collect_gold", 2);
        let mut info = player_info();
        seed_quest(&mut info, &quest);

        let first = record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            &scoring(&[&quest.id.0]),
            QuestEvent::CookieCollected,
        );
        let second = record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            &scoring(&[&quest.id.0]),
            QuestEvent::CookieCollected,
        );
        let third = record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            &scoring(&[&quest.id.0]),
            QuestEvent::CookieCollected,
        );

        assert!(matches!(first.unicast.as_slice(), [ServerMessage::QuestProgress(_)]));
        assert!(
            matches!(second.unicast.as_slice(), [ServerMessage::QuestCompleted(completed)] if completed.id == quest.id)
        );
        assert!(third.unicast.is_empty());
        assert!(first.completed_titles.is_empty());
        assert_eq!(second.completed_titles, ["Gold"]);
        assert!(third.completed_titles.is_empty());
        assert!(info.quest_states[&quest.id].completed);
    }

    #[test]
    fn completion_awards_the_mapped_score() {
        let quest = cookies_quest("collect_gold", 2);
        let mut info = player_info();
        seed_quest(&mut info, &quest);
        let scoring = scoring(&[&quest.id.0]);

        record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            &scoring,
            QuestEvent::CookieCollected,
        );
        assert_eq!(info.score, 0, "progress ticks award nothing");

        record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            &scoring,
            QuestEvent::CookieCollected,
        );
        assert_eq!(info.score, 100, "completion awards the mapped value");
    }

    #[test]
    fn actor_kill_respects_kind_filter() {
        let quest = sentry_quest("destroy_sentries", 2);
        let mut info = player_info();
        seed_quest(&mut info, &quest);

        let wrong = record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            &scoring(&[&quest.id.0]),
            QuestEvent::ActorKilled { kind: "zapper" },
        );
        let matching = record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            &scoring(&[&quest.id.0]),
            QuestEvent::ActorKilled { kind: "sentry" },
        );

        assert!(wrong.unicast.is_empty());
        assert!(
            matches!(matching.unicast.as_slice(), [ServerMessage::QuestProgress(progress)] if progress.progress == 1)
        );
    }

    #[test]
    fn event_only_advances_matching_quest_kind() {
        let cookie = cookies_quest("collect_gold", 5);
        let sentry = sentry_quest("destroy_sentries", 5);
        let mut info = player_info();
        seed_quest(&mut info, &cookie);
        seed_quest(&mut info, &sentry);
        let quests = [cookie.clone(), sentry.clone()];

        let messages = record_quest_event(
            &mut info,
            &quests,
            &scoring(&[&cookie.id.0, &sentry.id.0]),
            QuestEvent::CookieCollected,
        );

        assert_eq!(messages.unicast.len(), 1);
        assert_eq!(info.quest_states[&cookie.id].progress, 1);
        assert_eq!(info.quest_states[&sentry.id].progress, 0);
    }

    #[test]
    fn fireworks_event_only_advances_fireworks_quests() {
        let fireworks = fireworks_quest("start_fireworks");
        let cookie = cookies_quest("collect_gold", 5);
        let mut info = player_info();
        seed_quest(&mut info, &fireworks);
        seed_quest(&mut info, &cookie);
        let quests = [fireworks.clone(), cookie.clone()];

        let outcome = record_quest_event(
            &mut info,
            &quests,
            &scoring(&[&fireworks.id.0, &cookie.id.0]),
            QuestEvent::FireworksStarted,
        );

        assert_eq!(outcome.completed_titles, ["Sky"]);
        assert!(info.quest_states[&fireworks.id].completed);
        assert_eq!(info.quest_states[&cookie.id].progress, 0);
    }

    #[test]
    fn assign_quests_seeds_state_and_is_idempotent() {
        let quests = [cookies_quest("collect_gold", 10), sentry_quest("destroy_sentries", 4)];
        let mut info = player_info();

        let batch = assign_quests(&mut info, &quests).expect("quests assigned");
        record_quest_event(
            &mut info,
            &quests,
            &scoring(&[&quests[0].id.0, &quests[1].id.0]),
            QuestEvent::CookieCollected,
        );
        let second = assign_quests(&mut info, &quests);

        assert_eq!(batch.quests.len(), 2);
        assert!(batch.quests.iter().all(|quest| quest.progress == 0));
        assert_eq!(info.quest_states.len(), 2);
        assert!(second.is_none());
        assert_eq!(info.quest_states[&quests[0].id].progress, 1);
    }
}
