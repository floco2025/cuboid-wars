use super::transitions::BehaviorContext;
use crate::{
    actors::{ActorInfo, ActorMode, BeamState, resources::AwarePlayer},
    config::{ActorBeamAttackConfig, ActorKindServerConfig},
};
use bevy::prelude::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    physics::{CollisionWorld, character_hitbox_center},
    protocol::{ActorBeam, BarrierId, Position},
};

pub(super) struct BeamContext<'a> {
    pub tick: u32,
    pub world_pos: Position,
    pub kind_config: &'a ActorKindServerConfig,
    pub player_physics: CharacterPhysicsConfig,
    pub collision_world: &'a CollisionWorld,
    pub open_barriers: &'a [BarrierId],
}
impl<'a> From<&BehaviorContext<'a>> for BeamContext<'a> {
    fn from(context: &BehaviorContext<'a>) -> Self {
        Self {
            tick: context.tick,
            world_pos: context.world_pos,
            kind_config: context.kind_config,
            player_physics: context.player_physics,
            collision_world: context.collision_world,
            open_barriers: context.open_barriers,
        }
    }
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
            context.open_barriers,
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
