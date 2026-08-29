use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::actors::ActorKindServerConfig;
use common::protocol::{QuestId, QuestScope};

// One quest the server assigns to a player. Server-only: the wire ships only
// the per-quest `title` / `description` / `completed_text` strings (plus
// progress/threshold numbers) on the quest messages; clients never see the
// kind or the `actor_kind` filter.
#[derive(Debug, Clone, Deserialize)]
pub struct Quest {
    pub id: QuestId,
    pub kind: QuestKind,
    pub scope: QuestScope,
    // Hidden until this `shared` / `everyone` quest completes for the group.
    #[serde(default)]
    pub requires: Option<QuestId>,
    // For `ActorKills`: when `Some`, only kills of that actor kind count;
    // `None` counts any actor. Ignored by other kinds.
    #[serde(default)]
    pub actor_kind: Option<String>,
    pub threshold: u32,
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

pub(super) fn validate_quests(quests: &[Quest], actors: &HashMap<String, ActorKindServerConfig>) -> Result<()> {
    if quests.is_empty() {
        bail!("quests list must contain at least one quest");
    }
    let mut seen_ids: HashSet<&QuestId> = HashSet::with_capacity(quests.len());
    for (idx, quest) in quests.iter().enumerate() {
        let path = format!("quests[{idx}]");
        if quest.id.0.is_empty() {
            bail!("{path}.id must not be empty");
        }
        if !seen_ids.insert(&quest.id) {
            bail!("{path}.id `{}` is duplicated", quest.id.0);
        }
        if quest.threshold == 0 {
            bail!("{path}.threshold must be > 0");
        }
        if quest.title.is_empty() {
            bail!("{path}.title must not be empty");
        }
        if quest.description.is_empty() {
            bail!("{path}.description must not be empty");
        }
        if quest.completed_text.is_empty() {
            bail!("{path}.completed_text must not be empty");
        }
        match (quest.kind, &quest.actor_kind) {
            (QuestKind::ActorKills, Some(kind)) if !actors.contains_key(kind) => {
                bail!("{path}.actor_kind `{kind}` is not a known actor kind");
            }
            (kind, Some(_)) if kind != QuestKind::ActorKills => {
                bail!("{path}.actor_kind is only valid on an actor_kills quest");
            }
            _ => {}
        }
        // The firework launch has no acting player, so only a pooled counter
        // can consume it.
        if quest.kind == QuestKind::Fireworks && quest.scope != QuestScope::Shared {
            bail!("{path}: a fireworks quest must have scope `shared`");
        }
        if let Some(required) = &quest.requires {
            if required == &quest.id {
                bail!("{path}.requires must not name the quest itself");
            }
            if !quests.iter().any(|other| &other.id == required) {
                bail!("{path}.requires names unknown quest `{}`", required.0);
            }
            let Some(target) = quests[..idx].iter().find(|other| &other.id == required) else {
                bail!(
                    "{path}.requires `{}` must name a quest defined earlier in the list",
                    required.0
                );
            };
            if target.scope == QuestScope::Individual {
                bail!(
                    "{path}.requires `{}` must name a shared or everyone quest (an individual quest has no group completion)",
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
    use crate::config::ServerGameplayConfig;

    fn ok_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::Cookies,
            scope: QuestScope::Individual,
            requires: None,
            actor_kind: None,
            threshold,
            title: "Go".to_owned(),
            description: "go do the thing".to_owned(),
            completed_text: "done".to_owned(),
        }
    }

    fn no_actors() -> HashMap<String, ActorKindServerConfig> {
        HashMap::new()
    }

    fn default_actors() -> HashMap<String, ActorKindServerConfig> {
        ServerGameplayConfig::load_default()
            .expect("default server gameplay config should load")
            .actors
    }

    #[test]
    fn validate_quests_accepts_single_valid_entry() {
        validate_quests(&[ok_quest("a", 10)], &no_actors()).expect("valid quest should pass");
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
        validate_quests(&quests, &no_actors()).expect("valid chain should pass");
    }

    #[test]
    fn requires_rejects_unknown_quest() {
        let err = validate_quests(&[requiring("a", "nope", QuestScope::Shared)], &no_actors())
            .expect_err("unknown prerequisite must fail");
        assert!(err.to_string().contains("unknown quest"));
    }

    #[test]
    fn requires_rejects_a_later_quest() {
        let quests = [
            requiring("a", "b", QuestScope::Shared),
            group("b", QuestScope::Everyone),
        ];
        let err = validate_quests(&quests, &no_actors()).expect_err("forward reference must fail");
        assert!(err.to_string().contains("defined earlier"));
    }

    #[test]
    fn requires_rejects_itself() {
        let err = validate_quests(&[requiring("a", "a", QuestScope::Shared)], &no_actors())
            .expect_err("self reference must fail");
        assert!(err.to_string().contains("itself"));
    }

    #[test]
    fn requires_rejects_an_individual_target() {
        let quests = [ok_quest("solo", 1), requiring("b", "solo", QuestScope::Shared)];
        let err = validate_quests(&quests, &no_actors()).expect_err("individual prerequisite must fail");
        assert!(err.to_string().contains("shared or everyone"));
    }

    #[test]
    fn fireworks_quest_must_be_shared() {
        let mut quest = ok_quest("start_fireworks", 1);
        quest.kind = QuestKind::Fireworks;
        quest.scope = QuestScope::Everyone;
        let err = validate_quests(&[quest], &no_actors()).expect_err("non-shared fireworks quest must fail");
        assert!(err.to_string().contains("must have scope `shared`"));
    }

    #[test]
    fn fireworks_quest_rejects_actor_kind() {
        let mut quest = ok_quest("start_fireworks", 1);
        quest.kind = QuestKind::Fireworks;
        quest.scope = QuestScope::Shared;
        quest.actor_kind = Some("mine".to_owned());
        let err = validate_quests(&[quest], &default_actors()).expect_err("actor_kind on a fireworks quest must fail");
        assert!(err.to_string().contains("only valid on an actor_kills quest"));
    }

    #[test]
    fn validate_quests_rejects_empty_list() {
        let err = validate_quests(&[], &no_actors()).expect_err("empty list must be rejected");
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn validate_quests_rejects_duplicate_ids() {
        let err = validate_quests(&[ok_quest("dup", 5), ok_quest("dup", 7)], &no_actors())
            .expect_err("dup ids must be rejected");
        assert!(err.to_string().contains("duplicated"));
    }

    #[test]
    fn validate_quests_rejects_zero_threshold() {
        let err = validate_quests(&[ok_quest("z", 0)], &no_actors()).expect_err("zero threshold must be rejected");
        assert!(err.to_string().contains("threshold"));
    }

    #[test]
    fn validate_quests_rejects_empty_title() {
        let mut quest = ok_quest("a", 1);
        quest.title = String::new();
        let err = validate_quests(&[quest], &no_actors()).expect_err("empty title must be rejected");
        assert!(err.to_string().contains("title"));
    }

    #[test]
    fn validate_quests_rejects_empty_description() {
        let mut quest = ok_quest("a", 1);
        quest.description = String::new();
        let err = validate_quests(&[quest], &no_actors()).expect_err("empty description must be rejected");
        assert!(err.to_string().contains("description"));
    }

    #[test]
    fn validate_quests_rejects_empty_completed_text() {
        let mut quest = ok_quest("a", 1);
        quest.completed_text = String::new();
        let err = validate_quests(&[quest], &no_actors()).expect_err("empty completed_text must be rejected");
        assert!(err.to_string().contains("completed_text"));
    }

    #[test]
    fn validate_quests_accepts_actor_kills_with_known_actor_kind() {
        let mut quest = ok_quest("hunt", 4);
        quest.kind = QuestKind::ActorKills;
        quest.actor_kind = Some("sentry".to_owned());
        validate_quests(&[quest], &default_actors()).expect("known actor kind should pass");
    }

    #[test]
    fn validate_quests_rejects_actor_kills_with_unknown_actor_kind() {
        let mut quest = ok_quest("hunt", 4);
        quest.kind = QuestKind::ActorKills;
        quest.actor_kind = Some("dragon".to_owned());
        let err = validate_quests(&[quest], &default_actors()).expect_err("unknown actor kind must be rejected");
        assert!(err.to_string().contains("actor_kind"));
    }

    #[test]
    fn validate_quests_rejects_actor_kind_on_non_actor_kills_quest() {
        let mut quest = ok_quest("oops", 4);
        quest.actor_kind = Some("sentry".to_owned());
        let err = validate_quests(&[quest], &default_actors()).expect_err("actor_kind on cookies must be rejected");
        assert!(err.to_string().contains("actor_kind"));
    }
}
