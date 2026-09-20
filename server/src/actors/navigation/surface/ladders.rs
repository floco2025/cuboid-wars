use bevy::math::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    constants::{LADDER_RAIL_INSET, LADDER_STANDOFF_CLEARANCE, LADDER_VOLUME_DEPTH},
    physics::CollisionMesh,
    protocol::{FieldId, Ladder, Position},
};

use super::{SurfaceLocation, SurfaceMesh, TraversalAction};

#[derive(Clone)]
pub(super) struct SurfaceLink {
    pub from: SurfaceLocation,
    pub to: SurfaceLocation,
    pub actions: Vec<TraversalAction>,
}

impl SurfaceMesh {
    pub(super) fn add_ladders(
        &mut self,
        ladders: &[Ladder],
        physics: CharacterPhysicsConfig,
        geometry: &[CollisionMesh],
        open: &[FieldId],
    ) {
        let standoff = physics.movement_collider.radius() + LADDER_STANDOFF_CLEARANCE;
        if standoff >= LADDER_VOLUME_DEPTH {
            return;
        }
        for (index, ladder) in ladders
            .iter()
            .enumerate()
            .filter(|(_, ladder)| ladder.carrier == self.carrier)
        {
            let normal = Vec3::new(ladder.nx, 0.0, ladder.nz);
            let middle = Vec3::new((ladder.x1 + ladder.x2) / 2.0, ladder.y, (ladder.z1 + ladder.z2) / 2.0);
            let rail = middle + normal * (LADDER_RAIL_INSET + standoff);
            let mut landings: Vec<SurfaceLocation> = Vec::new();
            for (top, height) in [(false, ladder.y), (true, ladder.y + ladder.height)] {
                for side in [-1.0, 1.0] {
                    let probe = (middle + normal * side * (physics.movement_collider.radius() + 0.4)).with_y(height);
                    let landing = self
                        .polygons
                        .iter()
                        .enumerate()
                        .filter_map(|(polygon, _)| {
                            let point = self.project(polygon, probe)?;
                            ((point.y - height).abs() < 0.3
                                && point.with_y(0.0).distance_squared(probe.with_y(0.0)) < 0.36)
                                .then_some(SurfaceLocation {
                                    carrier: self.carrier,
                                    polygon,
                                    position: point.into(),
                                })
                        })
                        .min_by(|a, b| {
                            Vec3::from(a.position)
                                .distance_squared(probe)
                                .total_cmp(&Vec3::from(b.position).distance_squared(probe))
                        });
                    let Some(landing) = landing else {
                        continue;
                    };
                    if landings.iter().any(|old| old.polygon == landing.polygon) {
                        continue;
                    }
                    // The ladder can support a mount over an edge, but a wall
                    // between the rear landing and front mount must still block it.
                    if !top
                        && side < 0.0
                        && geometry
                            .iter()
                            .filter(|mesh| {
                                mesh.carrier == self.carrier && mesh.field.is_none_or(|field| !open.contains(&field))
                            })
                            .any(|mesh| {
                                !mesh.character_path_clear(
                                    landing.position,
                                    rail.with_y(landing.position.y).into(),
                                    physics,
                                )
                            })
                    {
                        continue;
                    }
                    landings.push(landing);
                }
            }
            for &from in &landings {
                for &to in &landings {
                    if (from.position.y - to.position.y).abs() < 0.5 {
                        continue;
                    }
                    let ascending = to.position.y > from.position.y;
                    let mount = Position::from(rail.with_y(from.position.y));
                    let climb = Position::from(rail.with_y(if ascending {
                        ladder.y + ladder.height + 0.05
                    } else {
                        ladder.y + 0.05
                    }));
                    self.links[from.polygon].push(SurfaceLink {
                        from,
                        to,
                        actions: vec![
                            TraversalAction::Walk {
                                carrier: self.carrier,
                                target: from.position,
                            },
                            TraversalAction::MountLadder {
                                carrier: self.carrier,
                                ladder: index,
                                target: mount,
                            },
                            TraversalAction::Climb {
                                carrier: self.carrier,
                                ladder: index,
                                target: climb,
                                normal: [ladder.nx, ladder.nz],
                                ascending,
                            },
                            TraversalAction::ExitLadder {
                                carrier: self.carrier,
                                ladder: index,
                                target: to.position,
                            },
                        ],
                    });
                }
            }
        }
        self.update_components();
    }
}

#[cfg(test)]
#[path = "tests/ladders.rs"]
mod tests;
