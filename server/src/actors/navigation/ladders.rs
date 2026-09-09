use std::collections::VecDeque;

use bevy::prelude::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    constants::{LADDER_CLIMB_MIN_SPEED, LADDER_RAIL_INSET, LADDER_STANDOFF_CLEARANCE, TICK_SECS},
    map::Carriers,
    physics::{
        ActorMovementStep, CharacterMovementResult, CharacterSupport, CollisionWorld, LadderVolume, step_actor_movement,
    },
    protocol::{ActorMoveIntent, BarrierKindId, Ladder, MapSettings, Position},
};

use super::{NavGraph, NavNode, NavWaypoint, PlannedRoute, WaypointKind};

const LANDING_CLEARANCE: f32 = 0.05;

// The climbing body the links are validated for, in the carrier-local world
// the graph covers.
pub(super) struct LadderClimber<'a> {
    pub(super) collision_world: &'a CollisionWorld,
    pub(super) map_settings: &'a MapSettings,
    pub(super) physics: CharacterPhysicsConfig,
    pub(super) passable_kinds: &'a [BarrierKindId],
    pub(super) carriers: &'a Carriers,
}

impl LadderClimber<'_> {
    fn step(&self, start: Position, vertical_velocity: f32, intent: ActorMoveIntent) -> CharacterMovementResult {
        step_actor_movement(ActorMovementStep {
            start,
            vertical_velocity,
            intent,
            external_displacement: Vec3::ZERO,
            delta: TICK_SECS,
            can_use_ladders: true,
            physics: self.physics,
            open_kinds: self.passable_kinds,
            collision_world: self.collision_world,
            map_settings: self.map_settings,
            carriers: self.carriers,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LadderLink {
    pub from: NavNode,
    pub to: NavNode,
    pub ladder: Ladder,
    // Equal endpoints describe a climb to a free end and back to the same landing.
    pub waypoints: Vec<NavWaypoint>,
}

impl LadderLink {
    fn contains(&self, pos: &Position) -> bool {
        let mount_y = self.waypoints[0].position.y;
        let climb_y = self.waypoints[1].position.y;
        LadderVolume::from_ladder(&self.ladder).contains(pos)
            && pos.y >= mount_y.min(climb_y) - LANDING_CLEARANCE
            && pos.y <= mount_y.max(climb_y) + LANDING_CLEARANCE * 2.0
    }

    fn exit_from(&self, start: &Position) -> Vec<NavWaypoint> {
        let mut steps = self.waypoints[self.waypoints.len() - 2..].to_vec();
        let target_y = steps[0].position.y;
        if let WaypointKind::Climb { ascending, .. } = &mut steps[0].kind {
            *ascending = target_y > start.y;
        }
        steps
    }
}

impl NavGraph {
    pub(crate) fn ladder_links(&self, kind: &str) -> &[LadderLink] {
        self.ladder_routes.get(kind).map_or(&[], Vec::as_slice)
    }

    pub(super) fn route_neighbors(&self, node: NavNode, ladders: &[LadderLink]) -> Vec<NavNode> {
        let mut neighbors = self.neighbors(node).to_vec();
        for link in ladders.iter().filter(|link| link.from == node && link.to != node) {
            if !neighbors.contains(&link.to) {
                neighbors.push(link.to);
            }
        }
        neighbors
    }

    pub(super) fn edge_waypoints(&self, from: NavNode, to: NavNode, ladders: &[LadderLink]) -> Vec<NavWaypoint> {
        if !self.neighbors(from).contains(&to)
            && let Some(link) = ladders.iter().find(|link| link.from == from && link.to == to)
        {
            return link.waypoints.to_vec();
        }
        vec![NavWaypoint::walk(self.node_center(to))]
    }

    pub(super) fn route_start_node(&self, start: &Position, ladders: &[LadderLink]) -> Option<NavNode> {
        self.ladder_exit(start, ladders)
            .map(|link| link.to)
            .or_else(|| self.nearest_node_for_position(start))
    }

    pub(super) fn ladder_exit<'a>(&self, start: &Position, ladders: &'a [LadderLink]) -> Option<&'a LadderLink> {
        ladders.iter().filter(|link| link.contains(start)).min_by(|a, b| {
            (self.node_center(a.to).y - start.y)
                .abs()
                .total_cmp(&(self.node_center(b.to).y - start.y).abs())
        })
    }

    pub(crate) fn ladder_exit_route(&self, start: &Position, ladders: &[LadderLink]) -> Option<PlannedRoute> {
        let exit = self.ladder_exit(start, ladders)?;
        Some(PlannedRoute {
            waypoints: exit.exit_from(start).into(),
            destination_node: exit.to,
        })
    }

    pub(super) fn prepend_ladder_exit(
        &self,
        start: &Position,
        route: &mut PlannedRoute,
        ladders: &[LadderLink],
    ) -> bool {
        let Some(link) = self.ladder_exit(start, ladders) else {
            return false;
        };
        let mut steps: VecDeque<_> = link.exit_from(start).into();
        steps.append(&mut route.waypoints);
        route.waypoints = steps;
        true
    }

    pub(crate) fn ladder_target_route(
        &self,
        start: &Position,
        target: &Position,
        ladders: &[LadderLink],
        physics: CharacterPhysicsConfig,
    ) -> Option<PlannedRoute> {
        ladders
            .iter()
            .filter(|link| link.contains(target))
            .filter_map(|link| {
                let mut climb = link.waypoints[1];
                climb.position.y = target.y;
                if let WaypointKind::Climb { ascending, .. } = &mut climb.kind {
                    *ascending = target.y > start.y;
                }
                let mut route = if link.contains(start) {
                    // A stationary target between simulation steps must not make the climber reverse every decision.
                    if (target.y - start.y).abs() < 0.1 {
                        climb.position.y = start.y;
                    }
                    PlannedRoute {
                        waypoints: VecDeque::new(),
                        destination_node: link.to,
                    }
                } else {
                    let mut approach = self.engagement_route(
                        ladders,
                        start,
                        &self.node_center(link.from),
                        physics.movement_collider.radius(),
                        physics.movement_collider.radius(),
                    )?;
                    approach.waypoints.push_back(link.waypoints[0]);
                    if let WaypointKind::Climb { ascending, .. } = &mut climb.kind {
                        *ascending = target.y > self.node_center(link.from).y;
                    }
                    approach
                };
                route.waypoints.push_back(climb);
                route.destination_node = link.to;
                Some(route)
            })
            .min_by(|a, b| route_length(start, a).total_cmp(&route_length(start, b)))
    }

    pub(super) fn build_ladder_links(
        &self,
        ladder: &Ladder,
        climber: &LadderClimber<'_>,
        speeds: [f32; 2],
    ) -> Vec<LadderLink> {
        let mut landings = Vec::new();
        let normal = Vec3::new(ladder.nx, 0.0, ladder.nz);
        let midpoint = Vec3::new(
            f32::midpoint(ladder.x1, ladder.x2),
            0.0,
            f32::midpoint(ladder.z1, ladder.z2),
        );
        let standoff = climber.physics.movement_collider.radius() + LADDER_STANDOFF_CLEARANCE;
        let rail = midpoint + normal * (LADDER_RAIL_INSET + standoff);
        for level in ladder.level..=ladder.level.saturating_add(ladder.levels) {
            for side in [-1.0, 1.0] {
                let center = midpoint + normal * (side * self.cell_size() / 2.0);
                let node = NavNode {
                    level,
                    row: self.geometry.cell_row_containing_z(center.z),
                    col: self.geometry.cell_col_containing_x(center.x),
                };
                if !self.is_cover_destination(node) {
                    continue;
                }
                let pos = self.node_center(node);
                let step = climber.step(pos, 0.0, ActorMoveIntent::Idle);
                if step.support == CharacterSupport::Ground && (step.position.y - pos.y).abs() < 0.1 {
                    landings.push(node);
                }
            }
        }
        let mut links = Vec::new();
        for &from in &landings {
            for &to in &landings {
                if from.level == to.level {
                    continue;
                }
                // Connect consecutive usable landings; longer routes compose these climbs.
                if landings
                    .iter()
                    .any(|node| from.level.min(to.level) < node.level && node.level < from.level.max(to.level))
                {
                    continue;
                }
                let start = self.node_center(from);
                let exit = self.node_center(to);
                let waypoints = landing_waypoints(start, exit, rail, ladder);
                if speeds
                    .into_iter()
                    .all(|speed| route_is_walkable(start, &waypoints, climber, speed))
                {
                    links.push(LadderLink {
                        from,
                        to,
                        ladder: *ladder,
                        waypoints,
                    });
                }
            }
        }
        for &landing in &landings {
            let start = self.node_center(landing);
            for end in [ladder.level, ladder.level.saturating_add(ladder.levels)] {
                if end == landing.level
                    || landings.iter().any(|node| {
                        if end > landing.level {
                            node.level > landing.level
                        } else {
                            node.level < landing.level
                        }
                    })
                {
                    continue;
                }
                let mut waypoints = landing_waypoints(start, start, rail, ladder);
                if let WaypointKind::Climb { ascending, .. } = &mut waypoints[1].kind {
                    *ascending = end < landing.level;
                }
                waypoints.insert(
                    1,
                    NavWaypoint {
                        position: Position {
                            x: rail.x,
                            y: self.geometry.level_y(end) + LANDING_CLEARANCE,
                            z: rail.z,
                        },
                        kind: WaypointKind::Climb {
                            normal_x: ladder.nx,
                            normal_z: ladder.nz,
                            ascending: end > landing.level,
                        },
                    },
                );
                if speeds
                    .into_iter()
                    .all(|speed| route_is_walkable(start, &waypoints, climber, speed))
                {
                    links.push(LadderLink {
                        from: landing,
                        to: landing,
                        ladder: *ladder,
                        waypoints,
                    });
                }
            }
        }
        links
    }
}

fn landing_waypoints(start: Position, exit: Position, rail: Vec3, ladder: &Ladder) -> Vec<NavWaypoint> {
    vec![
        NavWaypoint {
            position: Position {
                x: rail.x,
                y: start.y,
                z: rail.z,
            },
            kind: WaypointKind::Mount,
        },
        NavWaypoint {
            position: Position {
                x: rail.x,
                y: exit.y + LANDING_CLEARANCE,
                z: rail.z,
            },
            kind: WaypointKind::Climb {
                normal_x: ladder.nx,
                normal_z: ladder.nz,
                ascending: exit.y > start.y,
            },
        },
        NavWaypoint {
            position: exit,
            kind: WaypointKind::Exit,
        },
    ]
}

fn route_length(start: &Position, route: &PlannedRoute) -> f32 {
    let mut previous = *start;
    route
        .waypoints
        .iter()
        .map(|waypoint| {
            let distance = previous.distance_sq(&waypoint.position).sqrt();
            previous = waypoint.position;
            distance
        })
        .sum()
}

fn route_is_walkable(mut pos: Position, waypoints: &[NavWaypoint], climber: &LadderClimber<'_>, speed: f32) -> bool {
    let climb_ratio = climber.map_settings.movement.ladder_climb_ratio;
    let mut velocity = 0.0;
    for &waypoint in waypoints {
        let seconds = match waypoint.kind {
            WaypointKind::Climb { .. } => {
                (pos.y - waypoint.position.y).abs() / (speed.max(LADDER_CLIMB_MIN_SPEED) * climb_ratio)
            }
            _ => pos.horizontal_distance_sq(&waypoint.position).sqrt() / speed,
        } + 3.0;
        let ticks = (seconds / TICK_SECS).ceil() as usize;
        let mut arrived = false;
        for _ in 0..ticks {
            if waypoint.reached(&pos) {
                arrived = true;
                break;
            }
            let intent = waypoint.movement_intent(&pos, speed);
            let step = climber.step(pos, velocity, intent);
            if step.blocked
                || step.crushed
                || (matches!(waypoint.kind, WaypointKind::Climb { .. })
                    && !waypoint.reached(&step.position)
                    && step.support != CharacterSupport::Ladder)
            {
                return false;
            }
            pos = step.position;
            velocity = step.vertical_velocity;
        }
        if !arrived {
            return false;
        }
    }
    true
}
