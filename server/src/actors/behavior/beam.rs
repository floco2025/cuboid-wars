use super::perception::PlayerState;
use crate::{
    actors::{ActorInfo, ActorMode, BeamState, resources::AwarePlayer},
    config::{ActorBeamAttackConfig, ActorKindServerConfig},
};
use bevy::prelude::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    physics::{CollisionWorld, character_hitbox_center},
    protocol::{ActorBeam, FieldId, Position},
};

pub(super) struct BeamContext<'a> {
    pub tick: u32,
    pub world_pos: Position,
    pub kind_config: &'a ActorKindServerConfig,
    pub player_physics: CharacterPhysicsConfig,
    pub collision_world: &'a CollisionWorld,
    pub open_fields: &'a [FieldId],
}
pub(super) fn retarget_beam(info: &mut ActorInfo, context: &BeamContext<'_>) {
    let BeamState::Firing { target: current, .. } = info.beam else {
        return;
    };
    let target = info
        .awareness
        .iter()
        .filter(|aware| beam_target_attackable(aware, context))
        .min_by_key(|aware| aware.id != current)
        .copied();
    if let Some(aware) = target {
        if let BeamState::Firing { target, .. } = &mut info.beam {
            *target = aware.id;
        }
        if matches!(info.mode, ActorMode::Engage { target, .. } if target == current) {
            info.mode = ActorMode::Engage {
                target: aware.id,
                target_pos: aware.pos,
            };
        }
    } else {
        info.beam = BeamState::Cooldown {
            remaining_secs: beam_attack(context).cooldown_secs,
        };
        info.decision_timer = 0.0;
    }
}

pub(super) fn find_beam_target(info: &ActorInfo, context: &BeamContext<'_>) -> Option<AwarePlayer> {
    context.kind_config.attack.beam()?;
    if !matches!(info.beam, BeamState::Ready) {
        return None;
    }
    info.awareness
        .iter()
        .find(|aware| beam_target_attackable(aware, context))
        .copied()
}

fn beam_target_attackable(aware: &AwarePlayer, context: &BeamContext<'_>) -> bool {
    let range = context
        .kind_config
        .attack
        .beam_range()
        .expect("beam range missing from beam actor");
    aware.visible
        && context.world_pos.distance_sq(&aware.pos) <= range * range
        && context.collision_world.attack_path_clear(
            Vec3::from(context.world_pos) + Vec3::Y * context.kind_config.character.beam_origin_y_offset(),
            character_hitbox_center(aware.pos, context.player_physics),
            context.open_fields,
        )
}

pub(super) fn start_beam(info: &mut ActorInfo, context: &BeamContext<'_>, target: AwarePlayer) -> ActorBeam {
    let fire = beam_attack(context);
    info.beam = BeamState::Firing {
        target: target.id,
        started_tick: context.tick,
        remaining_secs: fire.duration_secs,
    };
    ActorBeam {
        target: target.id,
        started_tick: context.tick,
        remaining_secs: fire.duration_secs,
    }
}

fn beam_attack(context: &BeamContext<'_>) -> ActorBeamAttackConfig {
    context
        .kind_config
        .attack
        .beam()
        .expect("beam attack config missing from beam controller")
}

pub(super) fn tick_beam_state(
    info: &mut ActorInfo,
    delta: f32,
    kind_config: &ActorKindServerConfig,
    players: &[PlayerState],
) {
    let mut ended = false;
    match &mut info.beam {
        BeamState::Ready => {}
        BeamState::Cooldown { remaining_secs } => {
            *remaining_secs = (*remaining_secs - delta).max(0.0);
            if *remaining_secs <= 0.0 {
                info.beam = BeamState::Ready;
            }
        }
        BeamState::Firing {
            target, remaining_secs, ..
        } => {
            *remaining_secs -= delta;
            if let Some(player) = players.iter().find(|player| player.id == *target)
                && matches!(info.mode, ActorMode::Engage { target: engaged, .. } if engaged == *target)
            {
                info.mode = ActorMode::Engage {
                    target: *target,
                    target_pos: player.pos,
                };
            }
            ended |= *remaining_secs <= 0.0;
        }
    }
    if ended {
        let cooldown_secs = kind_config
            .attack
            .beam()
            .expect("beam attack config missing from firing actor")
            .cooldown_secs;
        info.beam = BeamState::Cooldown {
            remaining_secs: cooldown_secs,
        };
        // The controller decides what the cooldown looks like (zappers run
        // for cover, contact-beam kinds keep attacking) on this same tick.
        info.decision_timer = 0.0;
    }
}
