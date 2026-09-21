use std::collections::VecDeque;

use bevy::math::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_MAX_SLOPE, CHARACTER_STEP_HEIGHT},
    map::Carriers,
    physics::{CharacterMovementResult, CharacterSupport, CollisionWorld, grounding_diagnostics},
    protocol::{ActorMoveIntent, CarrierId, FieldId, MapSettings, Position},
};

pub use crate::actors::navigation::surface::TraversalAction;
use crate::actors::navigation::{
    ActorTerritory,
    surface::{CarrierDock, SurfaceRoute},
};
use crate::actors::{ActorMovementStep, step_actor_movement};

const ARRIVAL_DISTANCE: f32 = 0.15;
const STALL_SECONDS: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalStatus {
    Moving,
    Waiting,
    Reached,
    Blocked,
    LostSupport,
    OutsideTerritory,
    Crushed,
}

pub struct TraversalEnvironment<'a> {
    pub world: &'a CollisionWorld,
    pub carriers: &'a Carriers,
    pub settings: &'a MapSettings,
    pub open: &'a [FieldId],
    pub delta: f32,
}

#[derive(Debug, Clone)]
pub struct TraversalExecutor {
    pub actions: VecDeque<TraversalAction>,
    pub movement: CharacterMovementResult,
    pub status: TraversalStatus,
    pub intent: ActorMoveIntent,
    pub physics: CharacterPhysicsConfig,
    pub speed: f32,
    pub(crate) facing: f32,
    pub(crate) start: Position,
    stalled_secs: f32,
}

impl TraversalExecutor {
    pub fn new(position: Position, physics: CharacterPhysicsConfig, speed: f32, env: &TraversalEnvironment) -> Self {
        let movement = step_actor_movement(ActorMovementStep {
            start: position,
            vertical_velocity: 0.0,
            intent: ActorMoveIntent::Idle,
            external_displacement: Vec3::ZERO,
            delta: env.delta,
            can_use_ladders: false,
            physics,
            open_fields: env.open,
            collision_world: env.world,
            map_settings: env.settings,
            carriers: env.carriers,
        });
        Self {
            actions: VecDeque::new(),
            movement,
            status: TraversalStatus::Reached,
            intent: ActorMoveIntent::Idle,
            physics,
            speed,
            facing: 0.0,
            start: position,
            stalled_secs: 0.0,
        }
    }

    pub fn set_route(&mut self, route: SurfaceRoute) {
        self.actions = route.actions;
        self.status = TraversalStatus::Moving;
        self.stalled_secs = 0.0;
    }

    // A ladder, a boarding, or a ride runs to its end through ordinary goal
    // changes; only a failed one is open to replanning.
    pub(crate) fn committed(&self) -> bool {
        self.actions.front().is_some_and(|action| action.committed())
            && !matches!(
                self.status,
                TraversalStatus::Blocked | TraversalStatus::LostSupport | TraversalStatus::OutsideTerritory
            )
    }

    #[cfg(test)]
    pub fn step(&mut self, env: &TraversalEnvironment) {
        self.step_with_avoidance(env, Vec3::ZERO, Vec3::ZERO, false, None, |_| None);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn step_with_avoidance(
        &mut self,
        env: &TraversalEnvironment,
        external_displacement: Vec3,
        avoidance: Vec3,
        waiting: bool,
        home: Option<&ActorTerritory>,
        blocking_actor: impl Fn(&CharacterMovementResult) -> Option<Vec3>,
    ) {
        let start = self.movement.position;
        self.start = start;
        let previous_facing = self.facing;
        let mut target = None;
        let mut ladder_intent = None;
        let mut mounting = false;
        let mut exiting = false;
        let mut walking = false;
        self.status = TraversalStatus::Reached;
        while let Some(action) = self.actions.front().copied().filter(|_| !waiting) {
            match action {
                TraversalAction::Walk { carrier, target: local } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    if start.horizontal_distance_sq(&destination) <= ARRIVAL_DISTANCE.powi(2)
                        && (start.y - destination.y).abs() <= 0.25 + slope_clearance(self.physics)
                    {
                        self.actions.pop_front();
                        continue;
                    }
                    target = Some(destination);
                    walking = true;
                }
                TraversalAction::MountLadder {
                    carrier, target: local, ..
                } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    if start.horizontal_distance_sq(&destination) <= ARRIVAL_DISTANCE.powi(2) {
                        self.actions.pop_front();
                        continue;
                    }
                    target = Some(destination);
                    mounting = true;
                }
                TraversalAction::Climb {
                    carrier,
                    target: local,
                    normal,
                    ascending,
                    ..
                } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    if (ascending && start.y >= destination.y) || (!ascending && start.y <= destination.y) {
                        self.actions.pop_front();
                        continue;
                    }
                    let sign = if ascending { -1.0 } else { 1.0 };
                    ladder_intent = Some(ActorMoveIntent::Climbing {
                        direction: (normal[0] * sign).atan2(normal[1] * sign),
                        speed: self.speed,
                    });
                }
                TraversalAction::ExitLadder {
                    carrier, target: local, ..
                } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    if start.horizontal_distance_sq(&destination) <= ARRIVAL_DISTANCE.powi(2)
                        && (start.y - destination.y).abs() <= 0.25
                    {
                        self.actions.pop_front();
                        continue;
                    }
                    target = Some(destination);
                    exiting = true;
                }
                TraversalAction::WaitForDock { carrier, dock } => {
                    if at_dock(env.carriers, carrier, dock) {
                        self.actions.pop_front();
                        continue;
                    }
                    self.status = TraversalStatus::Waiting;
                }
                TraversalAction::Board {
                    carrier,
                    target: local,
                    dock,
                } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    let support = grounding_diagnostics(env.world, &start, self.physics, env.open, &[]);
                    let on_board = support.supported && support.hit.is_some_and(|hit| hit.carrier == carrier);
                    if on_board && start.horizontal_distance_sq(&destination) < ARRIVAL_DISTANCE.powi(2) {
                        self.actions.pop_front();
                        continue;
                    }
                    if !on_board && !at_dock(env.carriers, carrier, dock) {
                        self.status = TraversalStatus::Waiting;
                    } else {
                        target = Some(destination);
                    }
                }
                TraversalAction::Ride { carrier, dock } => {
                    let shifted = Position::from(Vec3::from(start) + env.carriers.displacement(carrier));
                    let support = grounding_diagnostics(env.world, &shifted, self.physics, env.open, &[]);
                    if !support.supported || support.hit.is_none_or(|hit| hit.carrier != carrier) {
                        self.status = TraversalStatus::LostSupport;
                    } else if at_dock(env.carriers, carrier, dock) {
                        self.actions.pop_front();
                        continue;
                    } else {
                        self.status = TraversalStatus::Waiting;
                    }
                }
            }
            break;
        }
        let mut intent = ladder_intent.unwrap_or(ActorMoveIntent::Idle);
        if ladder_intent.is_some() {
            self.status = TraversalStatus::Moving;
        }
        if let Some(target) = target {
            let offset = Vec3::from(target) - Vec3::from(start);
            let distance = offset.x.hypot(offset.z);
            let speed = self.speed.min(distance / env.delta);
            let direction = offset.x.atan2(offset.z);
            let mut movement = if mounting {
                ActorMoveIntent::Climbing { direction, speed }
            } else if exiting {
                ActorMoveIntent::ExitingLadder { direction, speed }
            } else {
                ActorMoveIntent::Moving { direction, speed }
            };
            if walking && avoidance.length_squared() > 1e-6 {
                let steered = (movement.to_horizontal_velocity() + avoidance).clamp_length_max(self.speed);
                if supported_at(env, Vec3::from(start) + steered * env.delta, self.physics) {
                    movement = ActorMoveIntent::Moving {
                        direction: steered.x.atan2(steered.z),
                        speed: steered.length(),
                    };
                }
            }
            if walking {
                movement = super::steering::steer(movement, self.facing, distance, env.delta);
            }
            let next = Vec3::from(start) + movement.to_horizontal_velocity() * env.delta;
            let supported = supported_at(env, next, self.physics);
            if !mounting && !exiting && self.movement.support == CharacterSupport::Ground && !supported {
                self.status = TraversalStatus::LostSupport;
                // Keep turning toward safe ground even when the current
                // heading would step off a ledge.
                intent = ActorMoveIntent::Moving {
                    direction: movement.direction().expect("direction missing from walking intent"),
                    speed: 0.0,
                };
            } else {
                intent = movement;
                self.status = TraversalStatus::Moving;
            }
        }
        if waiting {
            intent = self.intent.holding_ladder();
            self.status = TraversalStatus::Waiting;
        }
        self.intent = intent;
        let step = |intent: ActorMoveIntent, external_displacement: Vec3| {
            step_actor_movement(ActorMovementStep {
                start,
                vertical_velocity: self.movement.vertical_velocity,
                intent,
                external_displacement,
                delta: env.delta,
                can_use_ladders: intent.uses_ladders(),
                physics: self.physics,
                open_fields: env.open,
                collision_world: env.world,
                map_settings: env.settings,
                carriers: env.carriers,
            })
        };
        let mut movement = step(intent, external_displacement);
        if home.is_some_and(|home| {
            !home.contains_position(
                env.carriers
                    .pose(home.carrier)
                    .inverse_transform_point(movement.position.into()),
            )
        }) {
            // Steering can leave a valid route's territory. Rerun the complete
            // motor without voluntary travel, retaining gravity, impulses and
            // carrier motion; clamping the result would corrupt its support.
            self.intent = stopped_intent(intent);
            movement = step(self.intent, external_displacement);
            self.actions.clear();
            self.status = TraversalStatus::OutsideTerritory;
        }
        let mut blocker = blocking_actor(&movement);
        // A pivot has no sweep. Probe the intended travel too, or turning
        // back toward the route every tick cancels the turn around a body.
        if blocker.is_none() && walking && self.status == TraversalStatus::Moving && self.intent.speed() == Some(0.0) {
            let offset = Vec3::from(target.expect("walking target")) - Vec3::from(start);
            let probe = step(
                ActorMoveIntent::Moving {
                    direction: offset.x.atan2(offset.z),
                    speed: self.speed.min(offset.length() / env.delta),
                },
                external_displacement,
            );
            blocker = blocking_actor(&probe);
        }
        if let Some(offset) = blocker {
            let mut sidestep = None;
            if walking && self.status == TraversalStatus::Moving && self.movement.support == CharacterSupport::Ground {
                let target = target.expect("walking target");
                let toward = offset.with_y(0.0).try_normalize().unwrap_or(Vec3::X);
                let tangent = Vec3::new(toward.z, 0.0, -toward.x) - toward * 0.25;
                let intent = super::steering::steer(
                    ActorMoveIntent::Moving {
                        direction: tangent.x.atan2(tangent.z),
                        speed: self.speed,
                    },
                    previous_facing,
                    start.horizontal_distance_sq(&target).sqrt(),
                    env.delta,
                );
                self.intent = intent;
                let next = Vec3::from(start) + intent.to_horizontal_velocity() * env.delta;
                if supported_at(env, next, self.physics) {
                    let alternative = step(intent, external_displacement);
                    if blocking_actor(&alternative).is_none()
                        && home.is_none_or(|home| {
                            home.contains_position(
                                env.carriers
                                    .pose(home.carrier)
                                    .inverse_transform_point(alternative.position.into()),
                            )
                        })
                    {
                        sidestep = Some(alternative);
                    }
                }
            }
            // Soft separation cannot stop crossing paths or blast impulses.
            // Retry the motor without horizontal travel so gravity, support,
            // landing and carrier state all describe the accepted position.
            movement = sidestep.unwrap_or_else(|| {
                self.intent = stopped_intent(self.intent);
                step(self.intent, external_displacement * Vec3::Y)
            });
        }
        if let Some(direction) = self.intent.direction() {
            self.facing = direction;
        }
        self.movement = movement;
        if self.movement.crushed {
            self.status = TraversalStatus::Crushed;
        }
        if self.status == TraversalStatus::Moving {
            let carried = env.carriers.displacement(self.movement.carrier);
            let progressed = (Vec3::from(self.movement.position) - Vec3::from(start) - carried).length_squared() > 1e-6;
            self.stalled_secs = if progressed { 0.0 } else { self.stalled_secs + env.delta };
            if self.stalled_secs >= STALL_SECONDS {
                self.status = TraversalStatus::Blocked;
            }
        } else {
            self.stalled_secs = 0.0;
        }
    }
}

fn stopped_intent(intent: ActorMoveIntent) -> ActorMoveIntent {
    match intent {
        ActorMoveIntent::Moving { direction, .. } => ActorMoveIntent::Moving { direction, speed: 0.0 },
        _ => intent.holding_ladder(),
    }
}

fn slope_clearance(physics: CharacterPhysicsConfig) -> f32 {
    // A capsule's bottom rides above the surface below its center on a
    // slope or rounded landing transition. A point probe must include that
    // clearance or it reports missing support while the motor is grounded.
    physics.movement_collider.radius() * (1.0 / CHARACTER_MAX_SLOPE.cos() - 1.0)
}

fn supported_at(env: &TraversalEnvironment, point: Vec3, physics: CharacterPhysicsConfig) -> bool {
    let rise = CHARACTER_STEP_HEIGHT + 0.05;
    env.world
        .support_surface(
            point + Vec3::Y * rise,
            rise + CHARACTER_GROUND_SNAP_DISTANCE + slope_clearance(physics),
            env.open,
        )
        .is_some_and(|hit| hit.normal.y >= CHARACTER_MAX_SLOPE.cos())
}

fn at_dock(carriers: &Carriers, carrier: CarrierId, dock: CarrierDock) -> bool {
    carriers
        .pose(carrier)
        .transform_position(&Position::default())
        .distance_sq(&carriers.pose(dock.parent).transform_position(&dock.position))
        < 0.01
        && (carriers.displacement(carrier) - carriers.displacement(dock.parent)).length_squared() < 1e-8
}

#[cfg(test)]
#[path = "tests/traversal.rs"]
mod tests;
