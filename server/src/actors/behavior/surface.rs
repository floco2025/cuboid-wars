use std::f32::consts::TAU;

use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld},
    protocol::{
        ActorId, MapItems, PlayerId, PlayerMarker, Position, SActorBeam, ServerMessage, ServerTick, SwitchState,
    },
};

use super::{
    beam::{BeamContext, find_beam_target, retarget_beam, start_beam, tick_beam_state},
    geometry::{covered, threat_distance_sq},
    home::return_goal,
    perception::{AI_DECISION_INTERVAL_SECS, decay_awareness, player_states, update_awareness},
    pursuit::pursuit_surface,
};
use crate::{
    actors::{
        ActorCharacter, ActorMap, ActorMode, BeamState, SurfaceAgent, SurfaceGoal, TraversalExecutor,
        navigation::{ActorTerritories, radical_inverse, surface::SurfaceNavigation},
    },
    config::{ActorAttackConfig, ServerGameplayConfig},
    network::broadcast_to_all,
    players::PlayerMap,
};

pub(crate) fn surface_actors_behavior_system(
    time: Res<Time>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    items: Res<MapItems>,
    collision: Res<CollisionWorld>,
    switches: Res<SwitchState>,
    gameplay: Res<GameplayConfig>,
    config: Res<ServerGameplayConfig>,
    mut navigation: ResMut<SurfaceNavigation>,
    territories: Res<ActorTerritories>,
    carriers: Res<Carriers>,
    mut actors: ResMut<ActorMap>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
    mut query: Query<(&ActorId, &Position, &ActorCharacter, &mut SurfaceAgent)>,
) {
    let delta = time.delta_secs();
    let states = player_states(&players, actors.peaceful, player_query.iter());
    for (id, position, character, mut agent) in &mut query {
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        let kind = config.expect_actor(&info.spawn_kind);
        let previous_beam = info.beam.snapshot().map(|beam| (beam.started_tick, beam.target));
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
        let beam = BeamContext {
            tick: tick.0,
            world_pos: *position,
            kind_config: kind,
            player_physics: gameplay.player.physics(),
            collision_world: &collision,
            open_fields: &switches.open_fields,
        };
        retarget_beam(info, &beam);
        if let Some(target) = find_beam_target(info, &beam) {
            start_beam(info, &beam, target);
        }
        let firing = info.beam.snapshot();
        if firing.map(|beam| (beam.started_tick, beam.target)) != previous_beam {
            broadcast_to_all(
                &players,
                ServerMessage::ActorBeam(SActorBeam {
                    id: *id,
                    tick: tick.0,
                    beam: firing,
                }),
            );
        }
        agent.decision_secs -= delta;
        agent.tactic_secs = (agent.tactic_secs - delta).max(0.0);
        if agent.decision_secs > 0.0 {
            continue;
        }
        agent.decision_secs = AI_DECISION_INTERVAL_SECS;
        if agent.executor.as_ref().is_some_and(TraversalExecutor::committed) {
            continue;
        }
        let pose = carriers.pose(info.carrier);
        let from = SurfaceGoal {
            carrier: info.carrier,
            position: pose.inverse_transform_position(position),
        };
        if !info.awareness.is_empty() {
            navigation.prepare_area(from, character.0.physics());
            if matches!(kind.attack, ActorAttackConfig::Beam(_)) && firing.is_some() {
                agent.goal = None;
                continue;
            }
            let beam_cooling =
                matches!(kind.attack, ActorAttackConfig::Beam(_)) && matches!(info.beam, BeamState::Cooldown { .. });
            let mut pending = None;
            let mut airborne = None;
            let reachable = if beam_cooling {
                None
            } else {
                info.awareness.iter().find_map(|target| {
                    if target.support == CharacterSupport::Airborne {
                        airborne.get_or_insert(*target);
                    }
                    let goal = pursuit_surface(target, &collision, &carriers, &switches.open_fields)?;
                    let goal = navigation.pursuit_goal(goal, character.0.physics());
                    let goal = navigation.approach_goal(
                        from,
                        goal,
                        character.0.physics(),
                        character.0.can_use_ladders,
                        &collision,
                        &carriers,
                        &switches.open_fields,
                    );
                    match navigation.prepare_route(from, goal, character.0.physics(), character.0.can_use_ladders) {
                        Some(true) => Some((*target, goal)),
                        None => {
                            pending.get_or_insert((*target, goal));
                            None
                        }
                        Some(false) => None,
                    }
                })
            };
            if let Some((target, goal)) = reachable.or(pending) {
                info.mode = ActorMode::Engage {
                    target: target.id,
                    target_pos: target.pos,
                };
                agent.goal = Some(SurfaceGoal {
                    carrier: goal.carrier,
                    position: goal.position,
                });
                continue;
            }
            if let Some(target) = airborne {
                // Being over a gap or an inaccessible roof is temporary while
                // airborne. Keep the previous pursuit destination until a
                // reachable surface appears; do not reuse a roam/flee goal.
                if !matches!(info.mode, ActorMode::Engage { target: id, .. } if id == target.id) {
                    agent.goal = None;
                }
                info.mode = ActorMode::Engage {
                    target: target.id,
                    target_pos: target.pos,
                };
                continue;
            }
            if players.players_can_be_armed(&items) && firing.is_none() {
                if matches!(info.mode, ActorMode::Evade { .. })
                    && agent.tactic_secs > 0.0
                    && !agent.reached()
                    && agent.failure.is_none()
                {
                    continue;
                }
                let threats: Vec<_> = info.awareness.iter().map(|target| target.pos).collect();
                let current_clearance = threat_distance_sq((*position).into(), &threats).sqrt();
                let mut best = None;
                let mesh = navigation.mesh_at(from, character.0.physics());
                let start = mesh.and_then(|mesh| mesh.locate(from.position, 1.0));
                if let (Some(mesh), Some(start)) = (mesh, start) {
                    for radius in [4.0, 10.0] {
                        for step in 0..12 {
                            let angle = step as f32 * TAU / 12.0;
                            let sample = Vec3::from(start.position) + Vec3::new(angle.sin(), 0.0, angle.cos()) * radius;
                            let Some(candidate) = mesh.locate(sample.into(), 1.5) else {
                                continue;
                            };
                            if !mesh.connected(start, candidate, character.0.can_use_ladders) {
                                continue;
                            }
                            let world = pose.transform_position(&candidate.position);
                            let clearance = threat_distance_sq(world.into(), &threats).sqrt();
                            let cover = covered(world.into(), &threats, &beam);
                            if !cover && clearance < current_clearance + 1.0 {
                                continue;
                            }
                            let score =
                                clearance - world.distance_sq(position).sqrt() * 0.2 + if cover { 8.0 } else { 0.0 };
                            if best.is_none_or(|(old, _, _)| score > old) {
                                best = Some((score, candidate, cover));
                            }
                        }
                    }
                }
                info.mode = ActorMode::Evade { fleeing: true };
                agent.goal = best.map(|(_, candidate, cover)| {
                    info.mode = ActorMode::Evade { fleeing: !cover };
                    SurfaceGoal {
                        carrier: candidate.carrier,
                        position: candidate.position,
                    }
                });
                agent.tactic_secs = 1.5;
                continue;
            }
        }
        let previous_mode = info.mode;
        let home = territories.get(info.spawn_zone_index);
        let home_pose = carriers.pose(home.carrier);
        let local = home_pose.inverse_transform_point((*position).into());
        let returning = (matches!(
            previous_mode,
            ActorMode::Engage { .. } | ActorMode::Evade { .. } | ActorMode::ReturnHome
        ) || !home.contains_position(local))
            && !home.contains_spawn_position(local);
        info.mode = if returning {
            ActorMode::ReturnHome
        } else {
            ActorMode::Roam
        };
        let goal_is_home = agent.goal.is_some_and(|goal| {
            let local = home_pose
                .inverse_transform_point(carriers.pose(goal.carrier).transform_position(&goal.position).into());
            if returning {
                home.contains_spawn_position(local)
            } else {
                home.contains_position(local)
            }
        });
        if previous_mode == info.mode && goal_is_home && !agent.reached() && agent.failure.is_none() {
            continue;
        }
        agent.goal = None;
        if returning {
            agent.goal = return_goal(
                home,
                from,
                &character.0,
                &mut navigation,
                &collision,
                &carriers,
                &switches.open_fields,
                &mut agent.roam_index,
            );
            // A temporarily blocked home is still home. Retry it, rather than
            // choosing another roaming destination on the wrong floor.
            continue;
        }
        let destination_carrier = info.carrier;
        let destination_pose = carriers.pose(destination_carrier);
        if let Some((mesh, _)) = navigation.mesh(destination_carrier, character.0.physics()) {
            for _ in 0..8 {
                agent.roam_index = agent.roam_index.wrapping_add(1);
                let index = agent.roam_index.wrapping_add(id.0 as usize * 17);
                let fraction = Vec3::new(
                    radical_inverse(index, 2),
                    radical_inverse(index, 5),
                    radical_inverse(index, 3),
                );
                let min = home.volume.min - Vec3::splat(home.distance);
                let max = home.volume.max + Vec3::splat(home.distance);
                let mut sample = min + (max - min) * fraction;
                sample.y -= home.center_height;
                if index.is_multiple_of(2) {
                    sample.y = home_pose.inverse_transform_point((*position).into()).y;
                }
                let local = destination_pose.inverse_transform_point(home_pose.transform_point(sample));
                let Some(candidate) = mesh.locate(local.into(), 1.0).map(|point| point.position) else {
                    continue;
                };
                let world = destination_pose.transform_position(&candidate);
                if home.contains_position(home_pose.inverse_transform_point(world.into()))
                    && position.distance_sq(&world) > 1.0
                    && navigation.can_route(
                        from,
                        SurfaceGoal {
                            carrier: destination_carrier,
                            position: candidate,
                        },
                        character.0.physics(),
                        character.0.can_use_ladders,
                    )
                {
                    agent.goal = Some(SurfaceGoal {
                        carrier: destination_carrier,
                        position: candidate,
                    });
                    break;
                }
            }
        }
    }
}
