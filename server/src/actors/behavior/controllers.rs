use common::physics::CharacterSupport;
use common::protocol::PlayerId;
use rand::Rng;

use crate::{
    actors::{ActorInfo, ActorMode, BeamState, resources::AwarePlayer},
    config::ActorBeamAttackConfig,
};

use super::tick::{BehaviorContext, enter_evade, enter_roam_or_return, keep_or_install_engagement_route};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BeamStarted {
    pub(super) target: PlayerId,
    pub(super) duration_secs: f32,
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
) -> Option<BeamStarted> {
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
) -> Option<BeamStarted> {
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
    let fire = beam_attack(context);
    info.awareness
        .iter()
        .find(|aware| aware.visible && context.world_pos.distance_sq(&aware.pos) <= fire.range * fire.range)
        .copied()
}

fn start_beam(info: &mut ActorInfo, context: &BehaviorContext<'_>, target: AwarePlayer) -> BeamStarted {
    let fire = beam_attack(context);
    info.beam = BeamState::Firing {
        target: target.id,
        remaining_secs: fire.duration_secs,
    };
    BeamStarted {
        target: target.id,
        duration_secs: fire.duration_secs,
    }
}

fn beam_attack(context: &BehaviorContext<'_>) -> ActorBeamAttackConfig {
    context
        .kind_config
        .attack
        .beam()
        .expect("beam controller requires beam attack config")
}

fn try_engage_attackable_player(info: &mut ActorInfo, context: &BehaviorContext<'_>) -> bool {
    let current_target = match info.mode {
        ActorMode::Engage { target, .. } => Some(target),
        ActorMode::Roam | ActorMode::Evade | ActorMode::ReturnHome => None,
    };
    let mut candidates = info.awareness.clone();
    candidates.sort_by_key(|aware| Some(aware.id) != current_target);

    for aware in candidates {
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

fn enter_passive_or_evade(info: &mut ActorInfo, context: &BehaviorContext<'_>, rng: &mut impl Rng) {
    if info.awareness.is_empty() {
        enter_roam_or_return(info, context, rng);
    } else {
        enter_evade(info, context);
    }
}
