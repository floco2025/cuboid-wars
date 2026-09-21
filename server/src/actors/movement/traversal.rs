use std::collections::VecDeque;

use bevy::prelude::{Entity, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_MAX_SLOPE, CHARACTER_STEP_HEIGHT},
    map::Carriers,
    physics::{CharacterMovementResult, CharacterSupport, CollisionWorld, grounding_diagnostics},
    protocol::{ActorMoveIntent, CarrierId, FieldId, MapSettings, Position},
};

use super::steering::steer;
pub use crate::actors::navigation::surface::TraversalAction;
use crate::actors::navigation::{
    ActorTerritory,
    surface::{CarrierDock, SurfaceRoute},
};
use crate::actors::{ActorMovementStep, step_actor_movement};

const ARRIVAL_DISTANCE: f32 = 0.15;
const STALL_SECONDS: f32 = 2.0;
// How long two movers block each other without progress before the caller
// lets them pass; shorter than a stall, so a standoff ends before its routes fail.
const STANDOFF_SECONDS: f32 = 1.0;
// How far past its own body a walker going around another still treats that
// body as in its way.
const SIDESTEP_LOOKAHEAD: f32 = 1.0;
// The approach that counts as progress. A blocked walker re-approaches its
// blocker in whole steps, so a finer measure keeps finding new records.
const PROGRESS_DISTANCE: f32 = 0.05;

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

// The body that rejected a move and where it stands from the mover.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BodyBlocker {
    pub entity: Entity,
    pub offset: Vec3,
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
    pub(crate) blocked_by: Option<Entity>,
    // The hand a blocked walker turns to, kept while the blockage lasts so an
    // opening on the other side cannot turn it back.
    sidestep_hand: Option<f32>,
    // The action being timed and the closest the actor has come to its target.
    progress: Option<(TraversalAction, f32)>,
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
            blocked_by: None,
            sidestep_hand: None,
            progress: None,
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

    // The body this actor has made no progress against for a standoff's length.
    pub(crate) fn standoff(&self) -> Option<Entity> {
        self.blocked_by.filter(|_| self.stalled_secs >= STANDOFF_SECONDS)
    }

    #[cfg(test)]
    pub fn step(&mut self, env: &TraversalEnvironment) {
        self.step_with_avoidance(env, Vec3::ZERO, Vec3::ZERO, false, None, |_| None);
    }

    pub(crate) fn step_with_avoidance(
        &mut self,
        env: &TraversalEnvironment,
        external_displacement: Vec3,
        avoidance: Vec3,
        waiting: bool,
        home: Option<&ActorTerritory>,
        blocking_actor: impl Fn(&CharacterMovementResult) -> Option<BodyBlocker>,
    ) {
        let start = self.movement.position;
        let mut target = None;
        let mut climb_target = None;
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
                    climb_target = Some(destination);
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
                movement = steer(movement, self.facing, distance, env.delta);
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
        let inside_home = |position: Position| {
            home.is_none_or(|home| {
                home.contains_position(env.carriers.pose(home.carrier).inverse_transform_point(position.into()))
            })
        };
        let mut movement = step(intent, external_displacement);
        if !inside_home(movement.position) {
            // Steering can leave a valid route's territory. Rerun the complete
            // motor without voluntary travel, retaining gravity, impulses and
            // carrier motion; clamping the result would corrupt its support.
            self.intent = stopped_intent(intent);
            movement = step(self.intent, external_displacement);
            self.actions.clear();
            self.status = TraversalStatus::OutsideTerritory;
        }
        let mut blocker = blocking_actor(&movement);
        // A pivot has no sweep, so it probes one tick of its intended travel.
        // A walker already going around a body looks further: a single step
        // reads as clear the moment it backs off, and the turn around the
        // body would start over on the next approach.
        if blocker.is_none()
            && walking
            && self.status == TraversalStatus::Moving
            && (self.intent.speed() == Some(0.0) || self.sidestep_hand.is_some())
        {
            let reach = if self.sidestep_hand.is_some() {
                self.physics.movement_collider.diameter + SIDESTEP_LOOKAHEAD
            } else {
                self.speed * env.delta
            };
            let offset = Vec3::from(target.expect("target missing from walking action")) - Vec3::from(start);
            blocker = blocking_actor(&CharacterMovementResult {
                position: (Vec3::from(start) + offset.clamp_length_max(reach)).into(),
                ..movement
            });
        }
        self.blocked_by = blocker.map(|blocker| blocker.entity);
        if let Some(blocker) = blocker {
            let mut sidestep = None;
            let mut turning = None;
            if walking && self.status == TraversalStatus::Moving && self.movement.support == CharacterSupport::Ground {
                let target = target.expect("target missing from walking action");
                let distance = start.horizontal_distance_sq(&target).sqrt();
                let toward = blocker.offset.with_y(0.0).try_normalize().unwrap_or(Vec3::X);
                let preferred = self.sidestep_hand.unwrap_or(1.0);
                for hand in [preferred, -preferred] {
                    let tangent = Vec3::new(toward.z, 0.0, -toward.x) * hand - toward * 0.25;
                    let intent = steer(
                        ActorMoveIntent::Moving {
                            direction: tangent.x.atan2(tangent.z),
                            speed: self.speed,
                        },
                        self.facing,
                        distance,
                        env.delta,
                    );
                    turning.get_or_insert(intent);
                    let travel = intent.to_horizontal_velocity() * env.delta;
                    if !supported_at(env, Vec3::from(start) + travel, self.physics) {
                        continue;
                    }
                    let alternative = step(intent, external_displacement);
                    let carried = env.carriers.displacement(alternative.carrier);
                    let travelled = (Vec3::from(alternative.position) - Vec3::from(start) - carried).with_y(0.0);
                    // A wall that swallows the sidestep leaves only its
                    // back-off, which would repeat forever: turn the other way.
                    if travelled.length_squared() < (travel * 0.5).length_squared() {
                        continue;
                    }
                    // Only the ground chooses the hand. A body in the way of
                    // this turn clears as the actor keeps turning; trying the
                    // other hand then would swing it back and forth.
                    self.sidestep_hand = Some(hand);
                    turning = Some(intent);
                    if blocking_actor(&alternative).is_none() && inside_home(alternative.position) {
                        sidestep = Some(alternative);
                    }
                    break;
                }
            }
            self.intent = turning.unwrap_or(self.intent);
            if let Some(alternative) = sidestep {
                movement = alternative;
            } else {
                // Soft separation cannot stop crossing paths or blast impulses.
                // Retry the motor without voluntary travel, and without the
                // impulse too when that alone still collides, so gravity,
                // support, landing and carrier state all describe the
                // accepted position.
                self.intent = stopped_intent(self.intent);
                movement = step(self.intent, external_displacement);
                if blocking_actor(&movement).is_some() {
                    movement = step(self.intent, external_displacement * Vec3::Y);
                }
            }
        } else {
            self.sidestep_hand = None;
        }
        if let Some(direction) = self.intent.direction() {
            self.facing = direction;
        }
        self.movement = movement;
        if self.movement.crushed {
            self.status = TraversalStatus::Crushed;
        }
        // Progress is a new closest approach to the action's target: a blocked
        // walker's sidesteps and back-offs travel without getting anywhere.
        if self.status == TraversalStatus::Moving
            && let (Some(action), Some(destination)) = (self.actions.front().copied(), target.or(climb_target))
        {
            let remaining = self.movement.position.distance_sq(&destination).sqrt();
            let closest = self
                .progress
                .filter(|(timed, _)| *timed == action)
                .map_or(f32::INFINITY, |(_, closest)| closest);
            if remaining + PROGRESS_DISTANCE < closest {
                self.progress = Some((action, remaining));
                self.stalled_secs = 0.0;
            } else {
                self.stalled_secs += env.delta;
            }
            if self.stalled_secs >= STALL_SECONDS {
                self.status = TraversalStatus::Blocked;
            }
        } else {
            self.progress = None;
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
