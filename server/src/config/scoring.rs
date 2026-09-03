use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

use super::{ActorKindServerConfig, validation::validate_covers_actor_kinds};

#[derive(Debug, Clone, Deserialize)]
pub struct ScoringConfig {
    pub player_kill: i32,
    pub player_death: i32,
    pub cookie: i32,
    pub actor_hit: HashMap<String, i32>,
    pub actor_kill: HashMap<String, i32>,
}

impl ScoringConfig {
    pub(super) fn validate(&self, actors: &HashMap<String, ActorKindServerConfig>) -> Result<()> {
        for (map, name) in [(&self.actor_hit, "actor_hit"), (&self.actor_kill, "actor_kill")] {
            validate_covers_actor_kinds(map.keys(), actors, &format!("scoring.{name}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ScoringConfig {
        ScoringConfig {
            player_kill: 200,
            player_death: -200,
            cookie: 1000,
            actor_hit: HashMap::from([("zapper".to_owned(), 5)]),
            actor_kill: HashMap::from([("zapper".to_owned(), 150)]),
        }
    }

    fn one_actor_kind(kind: &str) -> HashMap<String, ActorKindServerConfig> {
        let json = serde_json::json!({
            "collider": {
                "width": 1.0,
                "height": 1.0,
                "depth": 1.0,
                "y_offset": 0.1,
                "y_offset_anchor": "bottom"
            },
            "support_probe": { "width": 0.2, "depth": 0.2 },
            "eye_height": 1.0,
            "respawn_secs": 60.0,
            "vision_range": 40.0,
            "roam_steps": 2,
            "attack": { "type": "contact", "trigger_gap": 0.4 }
        });
        let actor: ActorKindServerConfig = serde_json::from_value(json).expect("actor fixture should deserialize");
        HashMap::from([(kind.to_owned(), actor)])
    }

    #[test]
    fn accepts_matching_maps() {
        fixture()
            .validate(&one_actor_kind("zapper"))
            .expect("matching maps should pass");
    }

    #[test]
    fn rejects_missing_actor_kind() {
        let mut scoring = fixture();
        scoring.actor_hit.clear();
        let err = scoring
            .validate(&one_actor_kind("zapper"))
            .expect_err("missing actor_hit kind must be rejected");
        assert!(err.to_string().contains("scoring.actor_hit"));
    }

    #[test]
    fn rejects_unknown_actor_kind() {
        let mut scoring = fixture();
        scoring.actor_kill.insert("banana".to_owned(), 1);
        let err = scoring
            .validate(&one_actor_kind("zapper"))
            .expect_err("unknown actor_kill kind must be rejected");
        assert!(err.to_string().contains("scoring.actor_kill"));
    }
}
