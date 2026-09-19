use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

use super::validation::validate_covers_actor_kinds;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoringConfig {
    pub player_kill: i32,
    pub player_death: i32,
    pub gold: i32,
    pub actor_hit: HashMap<String, i32>,
    pub actor_kill: HashMap<String, i32>,
}

impl ScoringConfig {
    pub(super) fn validate<T>(&self, actors: &HashMap<String, T>, path: &str) -> Result<()> {
        for (map, name) in [(&self.actor_hit, "actor_hit"), (&self.actor_kill, "actor_kill")] {
            validate_covers_actor_kinds(map.keys(), actors, &format!("{path}.{name}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/scoring.rs"]
mod tests;
