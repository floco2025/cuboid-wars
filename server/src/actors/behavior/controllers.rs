use super::beam::{find_beam_target, start_beam};
use common::{physics::CharacterSupport, protocol::ActorBeam};
use rand::Rng;

use crate::actors::{ActorInfo, ActorMode, BeamState};

use super::transitions::{
    BehaviorContext, enter_evade, enter_roam_or_return, install_ladder_engagement, keep_or_install_engagement_route,
};

pub(super) fn retarget_beam(info: &mut ActorInfo, context: &BehaviorContext<'_>) {
    super::beam::retarget_beam(info, &context.into());
}

pub(super) fn decide_stationary_actor(info: &mut ActorInfo, context: &BehaviorContext<'_>) -> Option<ActorBeam> {
    context.kind_config.attack.beam()?;
    if info.beam.target().is_none() {
        info.mode = ActorMode::Roam;
    }
    find_beam_target(info, &context.into()).map(|target| {
        info.mode = ActorMode::Engage {
            target: target.id,
            target_pos: target.pos,
        };
        start_beam(info, &context.into(), target)
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
    if let Some(target) = find_beam_target(info, &context.into()) {
        info.mode = ActorMode::Engage {
            target: target.id,
            target_pos: target.pos,
        };
        info.set_route(None);
        return Some(start_beam(info, &context.into(), target));
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
        let target_pos = info
            .awareness
            .iter()
            .find(|aware| aware.id == target)
            .expect("firing beam target missing from awareness")
            .pos;
        info.mode = ActorMode::Engage { target, target_pos };
        info.set_route(None);
        return None;
    }
    decide_contact_actor(info, context, rng);
    find_beam_target(info, &context.into()).map(|target| start_beam(info, &context.into(), target))
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
        if !keep_or_install_engagement_route(info, context, aware.id, aware.pos) {
            continue;
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
