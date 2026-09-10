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
    Gold,
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
            Self::Gold | Self::ActorKills => None,
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
#[path = "tests/quests.rs"]
mod tests;
