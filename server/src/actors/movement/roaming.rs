use std::collections::BTreeMap;

use common::{
    map::{CarrierPose, Carriers},
    protocol::CarrierId,
};

use crate::actors::{
    SurfaceGoal,
    navigation::{ActorTerritory, surface::SurfaceRoute},
};

use super::traversal::TraversalAction;

pub(super) fn route_stays_home(
    route: &SurfaceRoute,
    start: SurfaceGoal,
    home: &ActorTerritory,
    carriers: &Carriers,
) -> bool {
    let mut poses = BTreeMap::new();
    let mut position = start;
    for action in &route.actions {
        match *action {
            TraversalAction::Walk { carrier, target }
            | TraversalAction::MountLadder { carrier, target, .. }
            | TraversalAction::Climb { carrier, target, .. }
            | TraversalAction::ExitLadder { carrier, target, .. } => {
                position = SurfaceGoal {
                    carrier,
                    position: target,
                };
            }
            TraversalAction::WaitForDock { .. } => continue,
            TraversalAction::Board { carrier, target, dock } => {
                let parent = poses
                    .get(&dock.parent)
                    .copied()
                    .unwrap_or_else(|| carriers.pose(dock.parent));
                poses.insert(
                    carrier,
                    parent.then(&CarrierPose::from_translation(dock.position.into())),
                );
                position = SurfaceGoal {
                    carrier,
                    position: target,
                };
            }
            TraversalAction::Ride { carrier, dock } => {
                if position.carrier != carrier {
                    return false;
                }
                let parent = poses
                    .get(&dock.parent)
                    .copied()
                    .unwrap_or_else(|| carriers.pose(dock.parent));
                poses.insert(
                    carrier,
                    parent.then(&CarrierPose::from_translation(dock.position.into())),
                );
            }
        }
        let pose = |carrier: CarrierId| poses.get(&carrier).copied().unwrap_or_else(|| carriers.pose(carrier));
        let world = pose(position.carrier).transform_position(&position.position);
        // The territory is convex, so action endpoints also bound the segments
        // between them. Boarding and riding use their future dock poses.
        if !home.contains_position(pose(home.carrier).inverse_transform_point(world.into())) {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[path = "tests/roaming.rs"]
mod tests;
