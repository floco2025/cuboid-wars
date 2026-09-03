use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::validation::deserialize_required_option;
use common::protocol::{PlatePurpose, QuestId, QuestScope};

// One map's server-side quest definition. Quest updates project the display
// fields, scope, and threshold; advancement rules, filters, and points stay server-only.
#[derive(Debug, Clone, Deserialize)]
pub struct Quest {
    pub id: QuestId,
    pub kind: QuestKind,
    pub scope: QuestScope,
    // Hidden until this `shared` / `everyone` quest completes for the group.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub requires: Option<QuestId>,
    // For `ActorKills`: when `Some`, only kills of that actor kind count;
    // `None` counts any actor. Ignored by other kinds.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub actor_kind: Option<String>,
    pub threshold: u32,
    pub points: i32,
    // Short label for the quest panel.
    pub title: String,
    // Longer body shown (with the title) in the announcement banner.
    pub description: String,
    pub completed_text: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QuestKind {
    Cookies,
    ActorKills,
    // Completed when the firework plates launch the show (`/firework` doesn't count).
    Fireworks,
}

impl QuestKind {
    // The plate purpose whose plates solve this kind of quest, if any. Those
    // plates stay locked until such a quest unlocks.
    #[must_use]
    pub fn plate_purpose(self) -> Option<PlatePurpose> {
        match self {
            Self::Fireworks => Some(PlatePurpose::Firework),
            Self::Cookies | Self::ActorKills => None,
        }
    }

    // Kinds advanced by something that happens to the world rather than by
    // a player's action.
    #[must_use]
    pub fn is_world_event(self) -> bool {
        matches!(self, Self::Fireworks)
    }
}

pub(super) fn validate_quests<T>(quests: &[Quest], actors: &HashMap<String, T>, path: &str) -> Result<()> {
    let mut seen_ids: HashSet<&QuestId> = HashSet::with_capacity(quests.len());
    for (idx, quest) in quests.iter().enumerate() {
        let quest_path = format!("{path}[{idx}]");
        if quest.id.0.is_empty() {
            bail!("{quest_path}.id must not be empty");
        }
        if !seen_ids.insert(&quest.id) {
            bail!("{quest_path}.id `{}` is duplicated", quest.id.0);
        }
        if quest.threshold == 0 {
            bail!("{quest_path}.threshold must be > 0");
        }
        if quest.title.is_empty() {
            bail!("{quest_path}.title must not be empty");
        }
        if quest.description.is_empty() {
            bail!("{quest_path}.description must not be empty");
        }
        if quest.completed_text.is_empty() {
            bail!("{quest_path}.completed_text must not be empty");
        }
        match (quest.kind, &quest.actor_kind) {
            (QuestKind::ActorKills, Some(kind)) if !actors.contains_key(kind) => {
                bail!("{quest_path}.actor_kind `{kind}` is not a known actor kind");
            }
            (kind, Some(_)) if kind != QuestKind::ActorKills => {
                bail!("{quest_path}.actor_kind is only valid on an actor_kills quest");
            }
            _ => {}
        }
        // A world event has no acting player, so only a pooled counter can
        // consume it.
        if quest.kind.is_world_event() && quest.scope != QuestScope::Shared {
            bail!(
                "{quest_path}: a {:?} quest is advanced by a world event and must have scope `shared`",
                quest.kind
            );
        }
        if let Some(required) = &quest.requires {
            if required == &quest.id {
                bail!("{quest_path}.requires must not name the quest itself");
            }
            if !quests.iter().any(|other| &other.id == required) {
                bail!("{quest_path}.requires names unknown quest `{}`", required.0);
            }
            let Some(target) = quests[..idx].iter().find(|other| &other.id == required) else {
                bail!(
                    "{quest_path}.requires `{}` must name a quest defined earlier in the list",
                    required.0
                );
            };
            if !target.scope.is_group() {
                bail!(
                    "{quest_path}.requires `{}` must name a shared or everyone quest (an individual quest has no group completion)",
                    required.0
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActorKindServerConfig, ServerGameplayConfig};

    fn ok_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::Cookies,
            scope: QuestScope::Individual,
            requires: None,
            actor_kind: None,
            threshold,
            points: 100,
            title: "Go".to_owned(),
            description: "go do the thing".to_owned(),
            completed_text: "done".to_owned(),
        }
    }

    fn no_actors() -> HashMap<String, ()> {
        HashMap::new()
    }

    fn default_actors() -> HashMap<String, ActorKindServerConfig> {
        ServerGameplayConfig::load_default()
            .expect("default server gameplay config should load")
            .actors
    }

    fn validate<T>(quests: &[Quest], actors: &HashMap<String, T>) -> Result<()> {
        validate_quests(quests, actors, "quests")
    }

    #[test]
    fn validate_quests_accepts_single_valid_entry() {
        validate(&[ok_quest("a", 10)], &no_actors()).expect("valid quest should pass");
    }

    fn requiring(id: &str, requires: &str, scope: QuestScope) -> Quest {
        let mut quest = ok_quest(id, 1);
        quest.scope = scope;
        quest.requires = Some(QuestId(requires.to_owned()));
        quest
    }

    fn group(id: &str, scope: QuestScope) -> Quest {
        let mut quest = ok_quest(id, 1);
        quest.scope = scope;
        quest
    }

    #[test]
    fn requires_accepts_an_earlier_group_quest() {
        let quests = [
            group("gold", QuestScope::Everyone),
            requiring("fireworks", "gold", QuestScope::Shared),
        ];
        validate(&quests, &no_actors()).expect("valid chain should pass");
    }

    #[test]
    fn requires_rejects_unknown_quest() {
        let err = validate(&[requiring("a", "nope", QuestScope::Shared)], &no_actors())
            .expect_err("unknown prerequisite must fail");
        assert!(err.to_string().contains("unknown quest"));
    }

    #[test]
    fn requires_rejects_a_later_quest() {
        let quests = [
            requiring("a", "b", QuestScope::Shared),
            group("b", QuestScope::Everyone),
        ];
        let err = validate(&quests, &no_actors()).expect_err("forward reference must fail");
        assert!(err.to_string().contains("defined earlier"));
    }

    #[test]
    fn requires_rejects_itself() {
        let err =
            validate(&[requiring("a", "a", QuestScope::Shared)], &no_actors()).expect_err("self reference must fail");
        assert!(err.to_string().contains("itself"));
    }

    #[test]
    fn requires_rejects_an_individual_target() {
        let quests = [ok_quest("solo", 1), requiring("b", "solo", QuestScope::Shared)];
        let err = validate(&quests, &no_actors()).expect_err("individual prerequisite must fail");
        assert!(err.to_string().contains("shared or everyone"));
    }

    #[test]
    fn fireworks_quest_must_be_shared() {
        let mut quest = ok_quest("start_fireworks", 1);
        quest.kind = QuestKind::Fireworks;
        quest.scope = QuestScope::Everyone;
        let err = validate(&[quest], &no_actors()).expect_err("non-shared fireworks quest must fail");
        assert!(err.to_string().contains("must have scope `shared`"));
    }

    #[test]
    fn fireworks_quest_rejects_actor_kind() {
        let mut quest = ok_quest("start_fireworks", 1);
        quest.kind = QuestKind::Fireworks;
        quest.scope = QuestScope::Shared;
        quest.actor_kind = Some("mine".to_owned());
        let err = validate(&[quest], &default_actors()).expect_err("actor_kind on a fireworks quest must fail");
        assert!(err.to_string().contains("only valid on an actor_kills quest"));
    }

    #[test]
    fn validate_quests_accepts_empty_list() {
        validate(&[], &no_actors()).expect("maps may have no quests");
    }

    #[test]
    fn validate_quests_rejects_duplicate_ids() {
        let err =
            validate(&[ok_quest("dup", 5), ok_quest("dup", 7)], &no_actors()).expect_err("dup ids must be rejected");
        assert!(err.to_string().contains("duplicated"));
    }

    #[test]
    fn validate_quests_rejects_zero_threshold() {
        let err = validate(&[ok_quest("z", 0)], &no_actors()).expect_err("zero threshold must be rejected");
        assert!(err.to_string().contains("threshold"));
    }

    #[test]
    fn validate_quests_rejects_empty_title() {
        let mut quest = ok_quest("a", 1);
        quest.title = String::new();
        let err = validate(&[quest], &no_actors()).expect_err("empty title must be rejected");
        assert!(err.to_string().contains("title"));
    }

    #[test]
    fn validate_quests_rejects_empty_description() {
        let mut quest = ok_quest("a", 1);
        quest.description = String::new();
        let err = validate(&[quest], &no_actors()).expect_err("empty description must be rejected");
        assert!(err.to_string().contains("description"));
    }

    #[test]
    fn validate_quests_rejects_empty_completed_text() {
        let mut quest = ok_quest("a", 1);
        quest.completed_text = String::new();
        let err = validate(&[quest], &no_actors()).expect_err("empty completed_text must be rejected");
        assert!(err.to_string().contains("completed_text"));
    }

    #[test]
    fn validate_quests_accepts_actor_kills_with_known_actor_kind() {
        let mut quest = ok_quest("hunt", 4);
        quest.kind = QuestKind::ActorKills;
        quest.actor_kind = Some("sentry".to_owned());
        validate(&[quest], &default_actors()).expect("known actor kind should pass");
    }

    #[test]
    fn validate_quests_rejects_actor_kills_with_unknown_actor_kind() {
        let mut quest = ok_quest("hunt", 4);
        quest.kind = QuestKind::ActorKills;
        quest.actor_kind = Some("dragon".to_owned());
        let err = validate(&[quest], &default_actors()).expect_err("unknown actor kind must be rejected");
        assert!(err.to_string().contains("actor_kind"));
    }

    #[test]
    fn validate_quests_rejects_actor_kind_on_non_actor_kills_quest() {
        let mut quest = ok_quest("oops", 4);
        quest.actor_kind = Some("sentry".to_owned());
        let err = validate(&[quest], &default_actors()).expect_err("actor_kind on cookies must be rejected");
        assert!(err.to_string().contains("actor_kind"));
    }
}
