use bevy::prelude::Vec3;
use common::{
    physics::{CharacterSupport, character_hitbox_center},
    protocol::ActorBeam,
};
use rand::Rng;

use crate::{
    actors::{ActorInfo, ActorMode, BeamState, resources::AwarePlayer},
    config::ActorBeamAttackConfig,
};

use super::tick::{
    BehaviorContext, enter_evade, enter_roam_or_return, install_ladder_engagement, keep_or_install_engagement_route,
};

pub(super) fn retarget_beam(info: &mut ActorInfo, context: &BehaviorContext<'_>) {
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

pub(super) fn decide_stationary_actor(info: &mut ActorInfo, context: &BehaviorContext<'_>) -> Option<ActorBeam> {
    context.kind_config.attack.beam()?;
    if info.beam.target().is_none() {
        info.mode = ActorMode::Roam;
    }
    find_beam_target(info, context).map(|target| {
        info.mode = ActorMode::Engage {
            target: target.id,
            target_pos: target.pos,
        };
        start_beam(info, context, target)
    })
}

pub(super) fn decide_contact_actor(info: &mut ActorInfo, context: &BehaviorContext<'_>, rng: &mut impl Rng) {
    if try_engage_attackable_player(info, context) {
        return;
    }
    enter_passive_or_evade(info, context, rng);
}

pub(super) fn decide_beam_actor(
    info: &mut ActorInfo,
    context: &BehaviorContext<'_>,
    rng: &mut impl Rng,
) -> Option<ActorBeam> {
    if matches!(info.beam, BeamState::Firing { .. }) {
        return None;
    }
    if let Some(target) = find_beam_target(info, context) {
        info.mode = ActorMode::Engage {
            target: target.id,
            target_pos: target.pos,
        };
        info.set_route(None);
        return Some(start_beam(info, context, target));
    }

    if matches!(info.beam, BeamState::Cooldown { .. }) {
        enter_passive_or_evade(info, context, rng);
        return None;
    }
    if try_engage_attackable_player(info, context) {
        return None;
    }
    enter_passive_or_evade(info, context, rng);
    None
}

// A contact attacker that also carries a beam: movement follows the contact
// rules in every beam state, and the beam fires opportunistically on top.
// Mid-burst it never runs for cover — cover means breaking the line of
// sight the beam needs — so an unreachable target is held in view instead.
pub(super) fn decide_contact_beam_actor(
    info: &mut ActorInfo,
    context: &BehaviorContext<'_>,
    rng: &mut impl Rng,
) -> Option<ActorBeam> {
    if let BeamState::Firing { target, .. } = info.beam {
        if try_engage_attackable_player(info, context) {
            return None;
        }
        match info.awareness.iter().find(|aware| aware.id == target) {
            Some(aware) => {
                info.mode = ActorMode::Engage {
                    target,
                    target_pos: aware.pos,
                };
                info.set_route(None);
            }
            None => enter_passive_or_evade(info, context, rng),
        }
        return None;
    }
    decide_contact_actor(info, context, rng);
    find_beam_target(info, context).map(|target| start_beam(info, context, target))
}

fn find_beam_target(info: &ActorInfo, context: &BehaviorContext<'_>) -> Option<AwarePlayer> {
    if !matches!(info.beam, BeamState::Ready) {
        return None;
    }
    info.awareness
        .iter()
        .find(|aware| beam_target_attackable(aware, context))
        .copied()
}

fn beam_target_attackable(aware: &AwarePlayer, context: &BehaviorContext<'_>) -> bool {
    let range = context
        .kind_config
        .attack
        .beam_range()
        .expect("beam range missing from beam actor");
    aware.visible
        && context.world_pos.distance_sq(&aware.pos) <= range * range
        && context.collision_world.attack_path_clear(
            Vec3::from(context.world_pos) + Vec3::Y * context.kind_config.character.beam_origin_height(),
            character_hitbox_center(aware.pos, context.player_physics),
            context.open_barriers,
        )
}

fn start_beam(info: &mut ActorInfo, context: &BehaviorContext<'_>, target: AwarePlayer) -> ActorBeam {
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

fn beam_attack(context: &BehaviorContext<'_>) -> ActorBeamAttackConfig {
    context
        .kind_config
        .attack
        .beam()
        .expect("beam attack config missing from beam controller")
}

fn try_engage_attackable_player(info: &mut ActorInfo, context: &BehaviorContext<'_>) -> bool {
    let current_target = match info.mode {
        ActorMode::Engage { target, .. } => Some(target),
        ActorMode::Roam | ActorMode::Evade { .. } | ActorMode::ReturnHome => None,
    };
    let mut candidates = info.awareness.clone();
    candidates.sort_by_key(|aware| Some(aware.id) != current_target);

    for aware in candidates {
        if aware.support == CharacterSupport::Ladder && install_ladder_engagement(info, context, aware.id, aware.pos) {
            return true;
        }
        let attack_anchor = match aware.support {
            CharacterSupport::Ground => Some(aware.pos),
            CharacterSupport::Airborne if current_target == Some(aware.id) => aware.attack_anchor,
            CharacterSupport::Airborne | CharacterSupport::Ladder => None,
        };
        let Some(anchor) = attack_anchor else {
            continue;
        };
        if !keep_or_install_engagement_route(info, context, aware.id, anchor) {
            continue;
        }
        if aware.support == CharacterSupport::Ground
            && let Some(memory) = info.awareness.iter_mut().find(|memory| memory.id == aware.id)
        {
            memory.attack_anchor = Some(aware.pos);
        }
        return true;
    }
    false
}

// Nothing to fear from a player who cannot shoot: on a map without
// projectiles or missiles, an unreachable player is simply ignored.
fn enter_passive_or_evade(info: &mut ActorInfo, context: &BehaviorContext<'_>, rng: &mut impl Rng) {
    if info.awareness.is_empty() || !context.players_armed {
        enter_roam_or_return(info, context, rng);
    } else {
        enter_evade(info, context, rng);
    }
}
