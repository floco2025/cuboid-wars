use bevy::prelude::*;
use common::{
    config::{ActorMovementConfig, CharacterPhysicsConfig},
    map::Carriers,
    physics::{CharacterSupport, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity},
    protocol::{ActorId, ActorMoveIntent, CarrierId, FaceYaw, MapSettings, Position, ServerTick, SwitchState},
};

use super::traversal::{TraversalEnvironment, TraversalExecutor, TraversalStatus};
use crate::actors::{
    ActorCharacter, ActorCrushed, ActorLanding, ActorMap, ActorMode,
    navigation::surface::{RouteFailure, SurfaceNavigation},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SurfaceGoal {
    pub carrier: CarrierId,
    pub position: Position,
}

#[derive(Component, Default)]
pub struct SurfaceAgent {
    pub(crate) goal: Option<SurfaceGoal>,
    pub(crate) executor: Option<TraversalExecutor>,
    pub(crate) failure: Option<RouteFailure>,
    pub(crate) decision_secs: f32,
    pub(crate) tactic_secs: f32,
    pub(crate) roam_index: usize,
    pub(crate) route_revision: Option<(CarrierId, u64)>,
    route_goal: Option<SurfaceGoal>,
    retry_secs: f32,
    pending: bool,
}

impl SurfaceAgent {
    pub fn reached(&self) -> bool {
        self.executor
            .as_ref()
            .is_some_and(|e| e.status == TraversalStatus::Reached)
            && self.failure.is_none()
            && !self.pending
    }
}

pub(crate) fn surface_actors_movement_system(
    time: Res<Time>,
    tick: Res<ServerTick>,
    collision: Res<CollisionWorld>,
    carriers: Res<Carriers>,
    settings: Res<MapSettings>,
    switches: Res<SwitchState>,
    navigation: Res<SurfaceNavigation>,
    mut actors: ResMut<ActorMap>,
    mut query: Query<(
        &ActorId,
        &ActorCharacter,
        &ActorMovementConfig,
        &mut SurfaceAgent,
        &mut Position,
        &mut CharacterVerticalVelocity,
        &mut CharacterSupport,
        &mut ActorMoveIntent,
        &mut FaceYaw,
        &mut ActorCrushed,
        &mut ActorLanding,
        Option<&KnockbackVelocity>,
    )>,
) {
    let delta = time.delta_secs();
    if delta <= 0.0 {
        return;
    }
    let env = TraversalEnvironment {
        world: &collision,
        carriers: &carriers,
        settings: &settings,
        open: &switches.open_fields,
        delta,
    };
    let neighbors: Vec<_> = query
        .iter()
        .map(|(id, character, _, _, position, _, support, ..)| (*id, *position, character.0.physics(), *support))
        .collect();
    let mut occupancy = std::collections::BTreeMap::new();
    for (id, _, _, agent, _, _, support, ..) in &query {
        if *support == CharacterSupport::Ladder
            && let Some(ladder) = agent
                .executor
                .as_ref()
                .and_then(|executor| executor.actions.front())
                .and_then(|action| action.ladder())
        {
            occupancy.entry(ladder).or_insert(*id);
        }
    }
    let mut ordered: Vec<_> = query.iter_mut().collect();
    ordered.sort_by_key(|(id, ..)| id.0);
    if !ordered.is_empty() {
        let rotation = tick.0 as usize % ordered.len();
        ordered.rotate_left(rotation);
    }
    let mut budget = 8192;
    for (
        id,
        character,
        speeds,
        mut agent,
        mut position,
        mut velocity,
        mut support,
        mut intent,
        mut facing,
        mut crushed,
        mut landing,
        knockback,
    ) in ordered
    {
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        let physics = character.0.physics();
        let speed = if matches!(info.mode, ActorMode::Engage { .. } | ActorMode::Evade { .. }) {
            speeds.active_speed
        } else {
            speeds.roam_speed
        };
        let mut executor = agent
            .executor
            .take()
            .unwrap_or_else(|| TraversalExecutor::new(*position, physics, speed, &env));
        executor.movement.position = *position;
        executor.movement.vertical_velocity = velocity.0;
        executor.movement.support = *support;
        executor.speed = speed;
        executor.facing = facing.0;
        agent.retry_secs = (agent.retry_secs - delta).max(0.0);
        let committed = executor.actions.front().is_some_and(|action| action.committed())
            && !matches!(executor.status, TraversalStatus::Blocked | TraversalStatus::LostSupport);
        if !committed {
            if let Some(goal) = agent.goal {
                if navigation
                    .mesh_at(
                        SurfaceGoal {
                            carrier: info.carrier,
                            position: carriers
                                .previous_pose(info.carrier)
                                .inverse_transform_position(&position),
                        },
                        physics,
                    )
                    .is_some()
                {
                    let revision = navigation.revision();
                    let changed = agent.route_revision != Some((info.carrier, revision))
                        || agent.route_goal.is_none_or(|old| {
                            old.carrier != goal.carrier || old.position.distance_sq(&goal.position) > 0.25
                        });
                    let interrupted =
                        matches!(executor.status, TraversalStatus::Blocked | TraversalStatus::LostSupport);
                    if changed
                        || (agent.retry_secs <= 0.0
                            && (agent.failure.is_some()
                                || interrupted
                                || (executor.actions.is_empty()
                                    && position
                                        .distance_sq(&carriers.pose(goal.carrier).transform_position(&goal.position))
                                        > 0.25)))
                    {
                        if budget == 0 {
                            agent.pending = true;
                            if agent.route_revision != Some((info.carrier, revision)) {
                                executor.actions.clear();
                            }
                        } else {
                            agent.pending = false;
                            agent.retry_secs = 0.5;
                            let start = carriers
                                .previous_pose(info.carrier)
                                .inverse_transform_position(&position);
                            let limit = budget.min(4096);
                            let route = navigation.route(
                                SurfaceGoal {
                                    carrier: info.carrier,
                                    position: start,
                                },
                                goal,
                                physics,
                                character.0.can_use_ladders,
                                limit,
                            );
                            budget -= match &route {
                                Ok(route) => route.expanded,
                                Err(RouteFailure::NavigationUnavailable) => 0,
                                Err(_) => limit,
                            };
                            if !matches!(route, Err(RouteFailure::NavigationUnavailable)) {
                                agent.route_revision = Some((info.carrier, revision));
                                agent.route_goal = Some(goal);
                            }
                            match route {
                                Ok(route) => {
                                    executor.set_route(route);
                                    agent.failure = None;
                                }
                                Err(RouteFailure::NavigationUnavailable) => {
                                    agent.pending = true;
                                    agent.failure = None;
                                    if agent.route_revision != Some((info.carrier, revision)) {
                                        executor.actions.clear();
                                    }
                                }
                                Err(reason) => {
                                    executor.actions.clear();
                                    agent.failure = Some(reason);
                                }
                            }
                        }
                    }
                } else {
                    executor.actions.clear();
                    agent.pending = navigation.rebuilding();
                    agent.failure = (!agent.pending).then_some(RouteFailure::NavigationUnavailable);
                    agent.route_revision = None;
                }
            } else {
                executor.actions.clear();
                agent.route_goal = None;
                agent.failure = None;
                agent.pending = false;
            }
        }
        let ladder = executor.actions.front().and_then(|action| action.ladder()).or_else(|| {
            executor.actions.iter().find_map(|action| {
                let super::traversal::TraversalAction::MountLadder {
                    carrier,
                    ladder,
                    target,
                } = *action
                else {
                    return None;
                };
                let mount = carriers.pose(carrier).transform_position(&target);
                (position.horizontal_distance_sq(&mount) < (physics.movement_collider.diameter + 1.0).powi(2)
                    && (position.y - mount.y).abs() < 0.5)
                    .then_some(ladder)
            })
        });
        let waiting = ladder.is_some_and(|ladder| *occupancy.entry(ladder).or_insert(*id) != *id);
        let avoidance = if *support == CharacterSupport::Ground && !committed {
            crowd_velocity(*id, *position, physics, &neighbors).clamp_length_max(speed * 0.5)
        } else {
            Vec3::ZERO
        };
        executor.step_with_avoidance(
            &env,
            knockback.map_or(Vec3::ZERO, |v| v.step(delta)),
            avoidance,
            waiting,
        );
        let movement = executor.movement;
        *position = movement.position;
        velocity.0 = movement.vertical_velocity;
        *support = movement.support;
        *intent = executor.intent;
        if let Some(direction) = intent.direction() {
            facing.0 = direction;
        }
        crushed.0 = movement.crushed;
        landing.0 = movement.impact_speed;
        info.carrier = movement
            .grounding
            .hit
            .filter(|_| movement.grounding.supported)
            .map_or(movement.carrier, |hit| hit.carrier);
        agent.executor = Some(executor);
    }
}

fn crowd_velocity(
    id: ActorId,
    position: Position,
    physics: CharacterPhysicsConfig,
    neighbors: &[(ActorId, Position, CharacterPhysicsConfig, CharacterSupport)],
) -> Vec3 {
    let mut velocity = Vec3::ZERO;
    let radius = physics.movement_collider.radius();
    for &(other, point, body, support) in neighbors {
        if other == id || support != CharacterSupport::Ground {
            continue;
        }
        if position.y + physics.movement_collider.height <= point.y
            || point.y + body.movement_collider.height <= position.y
        {
            continue;
        }
        let offset = (Vec3::from(position) - Vec3::from(point)).with_y(0.0);
        let distance = offset.length();
        let clearance = radius + body.movement_collider.radius() + 0.2;
        if distance >= clearance {
            continue;
        }
        let direction = offset
            .try_normalize()
            .unwrap_or(if id.0 < other.0 { Vec3::X } else { Vec3::NEG_X });
        velocity += direction * (clearance - distance) * 2.0;
    }
    velocity
}
