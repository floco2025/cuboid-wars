use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{
        ActorId, ActorMarker, PlayerId, PlayerMarker, Position, SActorBeam, ServerMessage, ServerTick, SwitchState,
    },
};

use super::{
    beam::{BeamContext, find_beam_target, retarget_beam, start_beam, tick_beam_state},
    perception::{decay_awareness, player_states, update_awareness},
};
use crate::{
    actors::{ActorCharacter, ActorInfo, ActorMap, ActorMode},
    config::ServerGameplayConfig,
    network::broadcast_to_all,
    players::PlayerMap,
};

// An anchored actor has no navigation state. Its only decision is whether
// to fire, so it checks exposure every tick, including during a burst.
pub fn stationary_actors_behavior_system(
    time: Res<Time>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    collision: Res<CollisionWorld>,
    switches: Res<SwitchState>,
    gameplay: Res<GameplayConfig>,
    config: Res<ServerGameplayConfig>,
    mut actors: ResMut<ActorMap>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
    query: Query<(&ActorId, &Position, &ActorCharacter), With<ActorMarker>>,
) {
    let delta = time.delta_secs();
    let states = player_states(&players, actors.peaceful, player_query.iter());
    for (id, position, character) in query.iter().filter(|(_, _, character)| character.0.immovable) {
        let Some(info) = actors.get_mut(id) else { continue };
        let kind = config.expect_actor(&info.spawn_kind);
        let previous = info.beam.snapshot().map(|beam| (beam.started_tick, beam.target));
        tick_beam_state(info, delta, kind, &states);
        decay_awareness(info, delta);
        update_awareness(
            info,
            *position,
            character.0.eye_height(),
            kind.vision_range,
            kind.threat_memory_secs,
            gameplay.player.physics(),
            &states,
            &collision,
        );
        let context = BeamContext {
            tick: tick.0,
            world_pos: *position,
            kind_config: kind,
            player_physics: gameplay.player.physics(),
            collision_world: &collision,
            open_fields: &switches.open_fields,
        };
        retarget_beam(info, &context);
        decide_stationary_actor(info, &context);
        let beam = info.beam.snapshot();
        if beam.map(|beam| (beam.started_tick, beam.target)) != previous {
            broadcast_to_all(
                &players,
                ServerMessage::ActorBeam(SActorBeam {
                    id: *id,
                    tick: tick.0,
                    beam,
                }),
            );
        }
    }
}

pub(super) fn decide_stationary_actor(
    info: &mut ActorInfo,
    context: &BeamContext<'_>,
) -> Option<common::protocol::ActorBeam> {
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
