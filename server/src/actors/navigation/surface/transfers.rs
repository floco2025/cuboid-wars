use bevy::math::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    map::CarrierPose,
    protocol::{Carrier, CarrierId},
};

use super::{CarrierDock, RouteFailure, SurfaceLocation, SurfaceNavigation, SurfaceRoute, TraversalAction};
use crate::actors::SurfaceGoal;

#[derive(Clone)]
pub(super) struct DockLink {
    shore: SurfaceLocation,
    aboard: SurfaceLocation,
    dock: CarrierDock,
}

pub(super) fn dock_links(
    navigation: &SurfaceNavigation,
    physics: CharacterPhysicsConfig,
    motions: &[Carrier],
) -> Vec<DockLink> {
    let mut links = Vec::new();
    let reach = physics.movement_collider.diameter + 0.6;
    for (index, motion) in motions.iter().enumerate() {
        let carrier = CarrierId::from_carried_index(index);
        let Some((mesh, _)) = navigation.mesh(carrier, physics) else {
            continue;
        };
        let Some((parent, _)) = navigation.mesh(motion.parent, physics) else {
            continue;
        };
        for dock in [motion.from, motion.to] {
            let pose = CarrierPose::from_translation(dock.into());
            let mut candidates = Vec::new();
            for (polygon, vertices) in mesh.polygons.iter().enumerate() {
                let center = Vec3::from(mesh.center(polygon));
                for (edge, next) in mesh.neighbors[polygon].iter().enumerate() {
                    if next.is_some() {
                        continue;
                    }
                    let Some(boundary) =
                        mesh.project(polygon, vertices[edge].midpoint(vertices[(edge + 1) % vertices.len()]))
                    else {
                        continue;
                    };
                    let Some(shore) = parent.locate(pose.transform_point(boundary).into(), reach) else {
                        continue;
                    };
                    if (shore.position.y - pose.transform_point(boundary).y).abs() > 0.2 {
                        continue;
                    }
                    let inward = (center - boundary).normalize_or_zero() * physics.movement_collider.radius().min(0.4);
                    // A strip narrower than the inward step has no standing point here.
                    let Some(aboard) = mesh.locate((boundary + inward).into(), 0.1) else {
                        continue;
                    };
                    if parent.locate(pose.transform_position(&aboard.position), 0.15).is_some() {
                        continue;
                    }
                    let distance = shore.position.distance_sq(&pose.transform_position(&aboard.position));
                    candidates.push((distance, shore, aboard));
                }
            }
            candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut chosen: Vec<(SurfaceLocation, SurfaceLocation)> = Vec::new();
            for (_, shore, aboard) in candidates {
                if chosen
                    .iter()
                    .any(|(s, a)| parent.connected(*s, shore, false) && mesh.connected(*a, aboard, false))
                {
                    continue;
                }
                chosen.push((shore, aboard));
                links.push(DockLink {
                    shore,
                    aboard,
                    dock: CarrierDock {
                        parent: motion.parent,
                        position: dock,
                    },
                });
            }
        }
    }
    links
}

#[derive(Clone, Copy)]
enum Leg {
    Walk,
    Transfer { link: usize, boarding: bool },
}

fn itinerary(
    navigation: &SurfaceNavigation,
    links: &[DockLink],
    from: SurfaceGoal,
    to: SurfaceGoal,
    physics: CharacterPhysicsConfig,
    ladders: bool,
) -> Result<(Vec<SurfaceLocation>, Vec<(usize, usize, Leg)>), RouteFailure> {
    let start = navigation
        .mesh(from.carrier, physics)
        .ok_or(RouteFailure::NavigationUnavailable)?
        .0
        .locate(from.position, 1.0)
        .ok_or(RouteFailure::StartOutsideMesh)?;
    let goal = navigation
        .mesh(to.carrier, physics)
        .ok_or(RouteFailure::NavigationUnavailable)?
        .0
        .locate(to.position, 0.7)
        .ok_or(RouteFailure::GoalOutsideMesh)?;
    let connected = |a: SurfaceLocation, b: SurfaceLocation| {
        a.carrier == b.carrier
            && navigation
                .mesh(a.carrier, physics)
                .is_some_and(|(mesh, _)| mesh.connected(a, b, ladders))
    };
    if connected(start, goal) {
        return Ok((vec![start, goal], vec![(0, 1, Leg::Walk)]));
    }
    let mut nodes = vec![start, goal];
    for link in links {
        nodes.extend([link.shore, link.aboard]);
    }
    let mut costs = vec![f32::INFINITY; nodes.len()];
    let mut parents = vec![None; nodes.len()];
    let mut visited = vec![false; nodes.len()];
    costs[0] = 0.0;
    loop {
        let current = (0..nodes.len())
            .filter(|&index| !visited[index] && costs[index].is_finite())
            .min_by(|&a, &b| costs[a].total_cmp(&costs[b]));
        let Some(current) = current else {
            return Err(RouteFailure::Disconnected);
        };
        if current == 1 {
            break;
        }
        visited[current] = true;
        for next in 0..nodes.len() {
            if visited[next] {
                continue;
            }
            let (leg, cost) = if connected(nodes[current], nodes[next]) {
                (
                    Leg::Walk,
                    nodes[current].position.distance_sq(&nodes[next].position).sqrt(),
                )
            } else if current >= 2 && next >= 2 && (current - 2) / 2 == (next - 2) / 2 && current != next {
                (
                    Leg::Transfer {
                        link: (current - 2) / 2,
                        boarding: (current - 2) % 2 == 0,
                    },
                    4.0,
                )
            } else {
                continue;
            };
            let cost = costs[current] + cost;
            if cost < costs[next] {
                costs[next] = cost;
                parents[next] = Some((current, leg));
            }
        }
    }
    let mut legs = Vec::new();
    let mut cursor = 1;
    while cursor != 0 {
        let (previous, leg) = parents[cursor].expect("parent missing from a reached transfer node");
        legs.push((previous, cursor, leg));
        cursor = previous;
    }
    legs.reverse();
    Ok((nodes, legs))
}

pub(super) fn reachable(
    navigation: &SurfaceNavigation,
    links: &[DockLink],
    from: SurfaceGoal,
    to: SurfaceGoal,
    physics: CharacterPhysicsConfig,
    ladders: bool,
) -> bool {
    itinerary(navigation, links, from, to, physics, ladders).is_ok()
}

pub(super) fn route(
    navigation: &SurfaceNavigation,
    links: &[DockLink],
    from: SurfaceGoal,
    to: SurfaceGoal,
    physics: CharacterPhysicsConfig,
    ladders: bool,
    limit: usize,
) -> Result<SurfaceRoute, RouteFailure> {
    let (nodes, legs) = itinerary(navigation, links, from, to, physics, ladders)?;
    let mut route = SurfaceRoute {
        actions: Default::default(),
        expanded: 0,
    };
    for (from, to, leg) in legs {
        match leg {
            Leg::Walk => {
                let start = nodes[from];
                let part = navigation
                    .mesh(start.carrier, physics)
                    .ok_or(RouteFailure::NavigationUnavailable)?
                    .0
                    .route_for(
                        start.position,
                        nodes[to].position,
                        0.1,
                        limit.saturating_sub(route.expanded),
                        ladders,
                    )?;
                route.expanded += part.expanded;
                route.actions.extend(part.actions);
            }
            Leg::Transfer { link, boarding } => {
                let link = &links[link];
                let carrier = link.aboard.carrier;
                if boarding {
                    route.actions.extend([
                        TraversalAction::WaitForDock {
                            carrier,
                            dock: link.dock,
                        },
                        TraversalAction::Board {
                            carrier,
                            target: link.aboard.position,
                            dock: link.dock,
                        },
                    ]);
                } else {
                    route.actions.extend([
                        TraversalAction::Ride {
                            carrier,
                            dock: link.dock,
                        },
                        TraversalAction::Walk {
                            carrier: link.shore.carrier,
                            target: link.shore.position,
                        },
                    ]);
                }
            }
        }
    }
    Ok(route)
}

#[cfg(test)]
#[path = "tests/transfers.rs"]
mod tests;
