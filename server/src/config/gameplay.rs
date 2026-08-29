use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::actors::{ActorKindServerConfig, ActorSettingsConfig, ExplosionDamageConfig};
use super::cycles::{LightingCycleConfig, WeatherCycleConfig};
use super::feed::FeedConfig;
use super::maps::{MapServerConfig, validate_maps};
use super::quests::{Quest, validate_quests};
use super::validation::{
    validate_covers_actor_kinds, validate_non_negative_finite, validate_positive_finite, validate_probability,
};
use common::{config::resolve_actor_inheritance, protocol::ItemType};

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ServerGameplayConfig {
    // Named-map registry: each entry's map geometry lives at
    // `config/server/maps/<name>.json`.
    pub maps: HashMap<String, MapServerConfig>,
    pub default_map: String,
    // Global cycle definitions; each map's `weather`/`lighting` mode picks
    // whether it runs them.
    pub weather_cycle: WeatherCycleConfig,
    pub lighting_cycle: LightingCycleConfig,
    pub scoring: ScoringConfig,
    pub feed: FeedConfig,
    pub player: PlayerServerConfig,
    pub projectile: ProjectileConfig,
    pub missiles: MissilesServerConfig,
    pub power_ups: PowerUpsConfig,
    pub placed_items: PlacedItemsConfig,
    pub quests: Vec<Quest>,
    pub actor_settings: ActorSettingsConfig,
    pub actors: HashMap<String, ActorKindServerConfig>,
}

impl ServerGameplayConfig {
    pub fn load_default() -> Result<Self> {
        let config = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/server/gameplay.json"
        )))?;
        config.validate()?;
        Ok(config)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let mut value: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        resolve_actor_inheritance(&mut value, "actors")
            .with_context(|| format!("resolving actor inheritance in {}", path.display()))?;
        serde_json::from_value(value).with_context(|| format!("failed to deserialize {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        validate_maps(&self.maps, &self.default_map)?;
        self.weather_cycle.validate("weather_cycle")?;
        self.lighting_cycle.validate("lighting_cycle")?;
        self.player.validate("player")?;
        self.projectile.validate("projectile")?;
        self.missiles.validate("missiles")?;
        self.scoring.validate(&self.actors, &self.quests)?;
        self.feed.validate(&self.actors)?;
        self.power_ups.validate("power_ups")?;
        self.placed_items.validate("placed_items")?;
        self.actor_settings.validate("actor_settings")?;
        validate_quests(&self.quests, &self.actors)?;
        if self.actors.is_empty() {
            bail!("actors must define at least one kind");
        }
        for (kind, actor) in &self.actors {
            if kind.is_empty() {
                bail!("actor kind must not be empty");
            }
            actor.validate(&format!("actors.{kind}"))?;
        }
        Ok(())
    }

    #[must_use]
    pub fn actor(&self, kind: &str) -> Option<&ActorKindServerConfig> {
        self.actors.get(kind)
    }

    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &ActorKindServerConfig {
        self.actor(kind)
            .expect("actor kind missing from server gameplay config")
    }
}

// One projectile, one raw damage number. Players receive `damage * (1 -
// armor)`; actors take it raw — their toughness is health alone.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectileConfig {
    pub damage: f32,
}

impl ProjectileConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.damage, &format!("{path}.damage"))
    }
}

// Server-only missile tuning: guidance/flight parameters and blast damage.
// The client-visible half (lock range, max ammo, cooldown, blast radius)
// lives in `config/common/gameplay.json`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MissilesServerConfig {
    pub speed: f32,
    // Max steering rate, rad/s. Turn radius = speed / turn_rate; keep it
    // under half a grid cell (1.7 m) so missiles can corner in corridors.
    // A turn circle wider than `proximity_fuse_distance` can orbit after an
    // overshoot; approach passes still cross the fuse, so this is a feel
    // trade-off, not a hard invariant.
    pub turn_rate: f32,
    pub lifetime_secs: f32,
    // Max random deviation of the launch direction from the aim (degrees).
    // Missiles leave visibly off-axis and let the steering curve them in;
    // 0 = launch straight at the aim.
    pub launch_spread_degrees: f32,
    // Serpentine weave while homing: max angular deviation as a fraction of
    // the flight direction (~0.35 = up to ±20°). Purely cosmetic — it fades
    // out on final approach. 0 = fly straight at the target.
    pub weave_strength: f32,
    // Detonate when passing within this distance of the locked target —
    // a near miss on a small, moving collider still kills via the blast
    // core instead of looping for another pass. 0 = contact only.
    pub proximity_fuse_distance: f32,
    // Self-detonate after this long without 1 m of progress.
    pub stall_secs: f32,
    pub max_damage: f32,
    pub missiles_per_pack: u32,
}

impl MissilesServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.speed, &format!("{path}.speed"))?;
        validate_positive_finite(self.turn_rate, &format!("{path}.turn_rate"))?;
        validate_positive_finite(self.lifetime_secs, &format!("{path}.lifetime_secs"))?;
        if !(self.launch_spread_degrees.is_finite() && (0.0..=90.0).contains(&self.launch_spread_degrees)) {
            bail!(
                "{path}.launch_spread_degrees must be in [0, 90], got {}",
                self.launch_spread_degrees
            );
        }
        validate_non_negative_finite(self.weave_strength, &format!("{path}.weave_strength"))?;
        validate_non_negative_finite(self.proximity_fuse_distance, &format!("{path}.proximity_fuse_distance"))?;
        validate_positive_finite(self.stall_secs, &format!("{path}.stall_secs"))?;
        validate_non_negative_finite(self.max_damage, &format!("{path}.max_damage"))?;
        if self.missiles_per_pack == 0 {
            bail!("{path}.missiles_per_pack must be at least 1");
        }
        Ok(())
    }
}

// Every point value in the game, consolidated for balancing. The block is
// server-only state — clients read the resulting `score` field via
// `SSnapshot` and never need the per-event point values.
#[derive(Debug, Clone, Deserialize)]
pub struct ScoringConfig {
    pub player_kill: i32,
    pub player_death: i32,
    pub cookie: i32,
    // Per actor kind: points per projectile hit, and the kill bonus.
    pub actor_hit: HashMap<String, i32>,
    pub actor_kill: HashMap<String, i32>,
    // Per quest id: points on completion.
    pub quest_completed: HashMap<String, i32>,
}

impl ScoringConfig {
    // Point values themselves are unvalidated — negative deltas are legal
    // (e.g., a death penalty), and so is zero. Only the map keys are
    // checked: every actor kind and quest needs an explicit entry (a
    // missing one silently scoring 0 is the footgun), and an unknown key
    // is a typo.
    fn validate(&self, actors: &HashMap<String, ActorKindServerConfig>, quests: &[Quest]) -> Result<()> {
        for (map, name) in [(&self.actor_hit, "actor_hit"), (&self.actor_kill, "actor_kill")] {
            validate_covers_actor_kinds(map.keys(), actors, &format!("scoring.{name}"))?;
        }
        for quest in quests {
            if !self.quest_completed.contains_key(&quest.id.0) {
                bail!("scoring.quest_completed is missing quest {:?}", quest.id.0);
            }
        }
        for id in self.quest_completed.keys() {
            if !quests.iter().any(|quest| &quest.id.0 == id) {
                bail!("scoring.quest_completed contains unknown quest {id:?}");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerServerConfig {
    // Fraction of incoming damage players block (0.0 = unarmored, 0.9 =
    // takes 10%). Applies to projectile hits and explosion blasts; fall
    // damage ignores armor (it's defined as a fraction of max health by
    // fall distance).
    pub armor: f32,
    pub fall_damage: FallDamageConfig,
    // Blast dealt by a dying player, same shape as the per-actor-kind
    // `combat.death_explosion` — standing next to your victim is now a mistake.
    pub explosion: ExplosionDamageConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FallDamageConfig {
    // Below this fall distance (meters), landing does no damage.
    pub safe_fall_distance: f32,
    // At this fall distance, landing deals `max_health` damage (lethal).
    // Damage lerps linearly between the two endpoints and clamps past
    // `lethal_fall_distance`.
    pub lethal_fall_distance: f32,
}

impl PlayerServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_probability(self.armor, &format!("{path}.armor"))?;
        self.fall_damage.validate(&format!("{path}.fall_damage"))?;
        self.explosion.validate(&format!("{path}.explosion"))
    }
}

impl FallDamageConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.safe_fall_distance, &format!("{path}.safe_fall_distance"))?;
        validate_non_negative_finite(self.lethal_fall_distance, &format!("{path}.lethal_fall_distance"))?;
        if self.safe_fall_distance >= self.lethal_fall_distance {
            bail!(
                "{path}.safe_fall_distance ({}) must be < lethal_fall_distance ({})",
                self.safe_fall_distance,
                self.lethal_fall_distance
            );
        }
        Ok(())
    }
}

// Power-up effect tuning. Spawning lives elsewhere: random pools per map in
// `maps.<name>.random_items`, placed respawns in `placed_items`.
#[derive(Debug, Clone, Deserialize)]
pub struct PowerUpsConfig {
    pub speed_duration_secs: f32,
    pub multi_shot_duration_secs: f32,
    pub low_gravity_duration_secs: f32,
    // Fraction of max health restored by a single Health Potion pickup.
    // 0.0 < value <= 1.0 (1.0 = full heal). No duration — instant effect.
    pub health_potion_heal_fraction: f32,
}

impl PowerUpsConfig {
    // Per-`PowerUpKind` duration, sourced from the named JSON fields. Lets
    // the rest of the server look up durations by enum variant without
    // each call site enumerating the four matches.
    #[must_use]
    pub const fn duration_secs(&self, kind: common::protocol::PowerUpKind) -> f32 {
        use common::protocol::PowerUpKind as K;
        match kind {
            K::Speed => self.speed_duration_secs,
            K::MultiShot => self.multi_shot_duration_secs,
            K::LowGravity => self.low_gravity_duration_secs,
        }
    }

    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.speed_duration_secs, &format!("{path}.speed_duration_secs"))?;
        validate_non_negative_finite(
            self.multi_shot_duration_secs,
            &format!("{path}.multi_shot_duration_secs"),
        )?;
        validate_non_negative_finite(
            self.low_gravity_duration_secs,
            &format!("{path}.low_gravity_duration_secs"),
        )?;
        if !(self.health_potion_heal_fraction > 0.0 && self.health_potion_heal_fraction <= 1.0) {
            bail!(
                "{path}.health_potion_heal_fraction must be in (0.0, 1.0], got {}",
                self.health_potion_heal_fraction
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlacedItemsConfig {
    pub respawn_secs: PlacedItemRespawnSecs,
}

// How long a placed item stays hidden after pickup before reappearing at
// its cell. One value per `ItemType` config id; 0.0 = instant reappear.
#[derive(Debug, Clone, Deserialize)]
pub struct PlacedItemRespawnSecs {
    pub speed: f32,
    pub multi_shot: f32,
    pub low_gravity: f32,
    pub health_potion: f32,
    pub cookie: f32,
    pub key: f32,
    pub missile_pack: f32,
}

impl PlacedItemsConfig {
    #[must_use]
    pub const fn respawn_secs_for(&self, item_type: ItemType) -> f32 {
        let secs = &self.respawn_secs;
        match item_type {
            ItemType::SpeedPowerUp => secs.speed,
            ItemType::MultiShotPowerUp => secs.multi_shot,
            ItemType::LowGravityPowerUp => secs.low_gravity,
            ItemType::HealthPotion => secs.health_potion,
            ItemType::Cookie => secs.cookie,
            ItemType::Key(_) => secs.key,
            ItemType::MissilePack => secs.missile_pack,
        }
    }

    fn validate(&self, path: &str) -> Result<()> {
        let secs = &self.respawn_secs;
        for (value, name) in [
            (secs.speed, "speed"),
            (secs.multi_shot, "multi_shot"),
            (secs.low_gravity, "low_gravity"),
            (secs.health_potion, "health_potion"),
            (secs.cookie, "cookie"),
            (secs.key, "key"),
            (secs.missile_pack, "missile_pack"),
        ] {
            validate_non_negative_finite(value, &format!("{path}.respawn_secs.{name}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scoring_fixture() -> ScoringConfig {
        ScoringConfig {
            player_kill: 200,
            player_death: -200,
            cookie: 1000,
            actor_hit: HashMap::from([("zapper".to_owned(), 5)]),
            actor_kill: HashMap::from([("zapper".to_owned(), 150)]),
            quest_completed: HashMap::new(),
        }
    }

    fn one_actor_kind(kind: &str) -> HashMap<String, ActorKindServerConfig> {
        let json = serde_json::json!({
            "respawn_delay_secs": 60.0,
            "vision_range": 40.0,
            "roam_steps": 2,
            "combat": {
                "attack": { "type": "contact", "trigger_gap": 0.4 },
                "death_explosion": { "radius": 6.0, "max_damage": 75.0 }
            }
        });
        let actor: ActorKindServerConfig = serde_json::from_value(json).expect("actor fixture should deserialize");
        HashMap::from([(kind.to_owned(), actor)])
    }

    #[test]
    fn scoring_accepts_matching_maps() {
        scoring_fixture()
            .validate(&one_actor_kind("zapper"), &[])
            .expect("matching scoring maps should pass");
    }

    #[test]
    fn scoring_rejects_missing_actor_kind() {
        let mut scoring = scoring_fixture();
        scoring.actor_hit.clear();
        let err = scoring
            .validate(&one_actor_kind("zapper"), &[])
            .expect_err("missing actor_hit kind must be rejected");
        assert!(err.to_string().contains("scoring.actor_hit"));
    }

    #[test]
    fn scoring_rejects_unknown_actor_kind() {
        let mut scoring = scoring_fixture();
        scoring.actor_kill.insert("banana".to_owned(), 1);
        let err = scoring
            .validate(&one_actor_kind("zapper"), &[])
            .expect_err("unknown actor_kill kind must be rejected");
        assert!(err.to_string().contains("scoring.actor_kill"));
    }

    #[test]
    fn scoring_rejects_missing_quest() {
        let quest: Quest = serde_json::from_value(serde_json::json!({
            "id": "collect_gold",
            "kind": "cookies",
            "scope": "individual",
            "threshold": 10,
            "title": "Gold",
            "description": "collect gold",
            "completed_text": "done"
        }))
        .expect("quest fixture should deserialize");
        let scoring = scoring_fixture();
        let err = scoring
            .validate(&one_actor_kind("zapper"), std::slice::from_ref(&quest))
            .expect_err("missing quest reward must be rejected");
        assert!(err.to_string().contains("scoring.quest_completed"));

        let mut scoring = scoring_fixture();
        scoring.quest_completed.insert("collect_gold".to_owned(), 500);
        scoring
            .validate(&one_actor_kind("zapper"), std::slice::from_ref(&quest))
            .expect("complete quest map should pass");
        scoring.quest_completed.insert("bogus".to_owned(), 1);
        let err = scoring
            .validate(&one_actor_kind("zapper"), std::slice::from_ref(&quest))
            .expect_err("unknown quest reward must be rejected");
        assert!(err.to_string().contains("unknown quest"));
    }

    #[test]
    fn placed_item_respawn_secs_matches_item_type() {
        let config = PlacedItemsConfig {
            respawn_secs: PlacedItemRespawnSecs {
                speed: 1.0,
                multi_shot: 2.0,
                low_gravity: 4.0,
                health_potion: 5.0,
                cookie: 6.0,
                key: 7.0,
                missile_pack: 8.0,
            },
        };
        assert_eq!(config.respawn_secs_for(ItemType::SpeedPowerUp), 1.0);
        assert_eq!(config.respawn_secs_for(ItemType::MultiShotPowerUp), 2.0);
        assert_eq!(config.respawn_secs_for(ItemType::LowGravityPowerUp), 4.0);
        assert_eq!(config.respawn_secs_for(ItemType::HealthPotion), 5.0);
        assert_eq!(config.respawn_secs_for(ItemType::Cookie), 6.0);
        assert_eq!(
            config.respawn_secs_for(ItemType::Key(common::protocol::BarrierKindId(0))),
            7.0
        );
        assert_eq!(config.respawn_secs_for(ItemType::MissilePack), 8.0);
    }
}
