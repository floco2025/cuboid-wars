use crate::config::fixtures;
// Actor kinds for tests, one per attack shape, with the health, damage,
// scoring, feed, and movement entries the config keys by kind, so no test
// leans on a shipped kind's name or tuning.
use std::collections::HashMap;

use common::config::{
    ActorGameplayConfig, ActorMovementConfig, CharacterGameplayConfig, CharacterPhysicsConfig, HitboxConfig,
    MovementColliderConfig,
};

use crate::config::{
    ActorAttackConfig, ActorBeamAttackConfig, ActorDamageConfig, ActorHealthConfig, ActorKindServerConfig, BlastConfig,
    ContactAttackConfig, ContactBeamAttackConfig, ServerGameplayConfig,
};

pub(crate) const CONTACT: &str = "contact";
pub(crate) const BEAM: &str = "beam";
pub(crate) const CONTACT_BEAM: &str = "contact_beam";
pub(crate) const IMMOVABLE: &str = "immovable";
pub(crate) const KINDS: [&str; 4] = [CONTACT, BEAM, CONTACT_BEAM, IMMOVABLE];

struct TestKind {
    server: ActorKindServerConfig,
    max_health: f32,
    damage: ActorDamageConfig,
    hit_score: i32,
    kill_score: i32,
    movement: Option<ActorMovementConfig>,
}

fn body(
    diameter: f32,
    height: f32,
    [width, hitbox_height, depth, bottom_offset]: [f32; 4],
    eye_height: f32,
) -> CharacterGameplayConfig {
    CharacterGameplayConfig {
        movement_collider: MovementColliderConfig { diameter, height },
        hitbox: HitboxConfig {
            width,
            height: hitbox_height,
            depth,
            bottom_offset,
        },
        eye_height,
    }
}

fn beam(range: f32, duration_secs: f32, cooldown_secs: f32) -> ActorBeamAttackConfig {
    ActorBeamAttackConfig {
        range,
        duration_secs,
        cooldown_secs,
    }
}

fn damage(beam_dps: Option<f32>, radius: f32, max_damage: f32) -> ActorDamageConfig {
    ActorDamageConfig {
        beam_dps,
        death_blast: BlastConfig { radius, max_damage },
    }
}

fn movement(roam_speed: f32, active_speed: f32) -> Option<ActorMovementConfig> {
    Some(ActorMovementConfig {
        roam_speed,
        active_speed,
    })
}

fn test_kind(name: &str) -> TestKind {
    match name {
        CONTACT => TestKind {
            server: ActorKindServerConfig {
                character: ActorGameplayConfig {
                    character: body(1.05, 1.1, [1.1, 0.88, 1.02, 0.0], 0.79),
                    can_use_ladders: false,
                    immovable: false,
                    beam_origin_height: None,
                },
                vision_range: 60.0,
                roam_steps: 3,
                attack: ActorAttackConfig::Contact(ContactAttackConfig { trigger_gap: 0.4 }),
            },
            max_health: 150.0,
            damage: damage(None, 10.0, 150.0),
            hit_score: 5,
            kill_score: 50,
            movement: movement(3.0, 5.0),
        },
        BEAM => TestKind {
            server: ActorKindServerConfig {
                character: ActorGameplayConfig {
                    character: body(0.88, 1.75, [0.94, 0.5, 0.76, 1.23], 1.59),
                    can_use_ladders: false,
                    immovable: false,
                    beam_origin_height: Some(1.35),
                },
                vision_range: 40.0,
                roam_steps: 2,
                attack: ActorAttackConfig::Beam(beam(25.0, 2.0, 8.0)),
            },
            max_health: 50.0,
            damage: damage(Some(40.0), 6.0, 75.0),
            hit_score: 50,
            kill_score: 150,
            movement: movement(2.0, 4.0),
        },
        CONTACT_BEAM => TestKind {
            server: ActorKindServerConfig {
                character: ActorGameplayConfig {
                    character: body(1.6, 1.7, [1.8, 1.5, 2.0, 0.0], 1.4),
                    can_use_ladders: false,
                    immovable: false,
                    beam_origin_height: None,
                },
                vision_range: 60.0,
                roam_steps: 7,
                attack: ActorAttackConfig::ContactBeam(ContactBeamAttackConfig {
                    contact: ContactAttackConfig { trigger_gap: 0.8 },
                    beam: beam(25.0, 2.0, 5.0),
                }),
            },
            max_health: 1000.0,
            damage: damage(Some(40.0), 6.0, 75.0),
            hit_score: 10,
            kill_score: 200,
            movement: movement(5.0, 8.0),
        },
        IMMOVABLE => TestKind {
            server: ActorKindServerConfig {
                character: ActorGameplayConfig {
                    character: body(0.5, 1.62, [0.5, 1.61, 0.5, 0.01], 1.6),
                    can_use_ladders: false,
                    immovable: true,
                    beam_origin_height: Some(1.45),
                },
                vision_range: 40.0,
                roam_steps: 0,
                attack: ActorAttackConfig::Beam(beam(25.0, 15.0, 0.5)),
            },
            max_health: 50.0,
            damage: damage(Some(500.0), 6.0, 75.0),
            hit_score: 50,
            kill_score: 150,
            movement: None,
        },
        _ => panic!("{name:?} is not a test actor kind"),
    }
}

pub(crate) fn kind(name: &str) -> ActorKindServerConfig {
    test_kind(name).server
}

pub(crate) fn physics(name: &str) -> CharacterPhysicsConfig {
    kind(name).character.physics()
}

pub(crate) fn server_config() -> ServerGameplayConfig {
    let mut config = fixtures::server_config();
    let kinds: Vec<(String, TestKind)> = KINDS.iter().map(|name| ((*name).to_owned(), test_kind(name))).collect();
    config.actors.kinds = table(&kinds, |kind| kind.server.clone());
    config.combat.health.actors = table(&kinds, |kind| ActorHealthConfig {
        max: kind.max_health,
        regen_rate: 0.01,
    });
    config.combat.damage.actors = table(&kinds, |kind| kind.damage);
    config.scoring.actor_hit = table(&kinds, |kind| kind.hit_score);
    config.scoring.actor_kill = table(&kinds, |kind| kind.kill_score);
    config.feed.actor_destroyed = table(&kinds, |_| false);
    let movement: HashMap<String, ActorMovementConfig> = kinds
        .iter()
        .filter_map(|(name, kind)| Some((name.clone(), kind.movement?)))
        .collect();
    for map in config.maps.values_mut() {
        map.settings.movement.actors = movement.clone();
    }
    config
}

fn table<T>(kinds: &[(String, TestKind)], value: impl Fn(&TestKind) -> T) -> HashMap<String, T> {
    kinds.iter().map(|(name, kind)| (name.clone(), value(kind))).collect()
}
