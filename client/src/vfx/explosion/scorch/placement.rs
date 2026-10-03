use bevy::prelude::*;

use super::{
    clip::{ClipRegion, Convex, HalfPlane},
    marks::SCORCH_SURFACE_OFFSET,
    variants::ScorchStyle,
};
use crate::map::GrassBurn;
use common::{
    map::Carriers,
    physics::{Solid, SolidFace},
    protocol::CarrierId,
};

// How far a face may sit off the mark's plane and still be the surface it lies on.
const COPLANAR_TOLERANCE: f32 = 0.02;
// How far behind a face a blast buried in its solid still marks it: one
// inside a thin wall scorches both its faces, one that burst on a slab does
// not scorch the ceiling under it.
const BURIED_REACH: f32 = 0.15;

// A point of a surface an explosion reaches, in world space.
#[derive(Clone, Copy)]
pub(crate) struct SurfaceContact {
    pub(crate) point: Vec3,
    pub(crate) normal: Vec3,
    pub(crate) carrier: CarrierId,
}

// A mark on a plane of the built world, in the frame of that plane's
// carrier so it rides with it. `region` is where the plane actually carries
// a surface the blast reaches, in the mark's own plane.
#[derive(Clone)]
pub(crate) struct ScorchPlacement {
    pub(super) transform: Transform,
    normal: Vec3,
    pub(super) carrier: CarrierId,
    pub(super) region: ClipRegion,
}

impl ScorchPlacement {
    fn new(point: Vec3, normal: Vec3, carrier: CarrierId, diameter: f32, style: ScorchStyle) -> Self {
        let alignment = Quat::from_rotation_arc(Vec3::Y, normal);
        let spin = Quat::from_axis_angle(normal, style.rotation());
        Self {
            transform: Transform {
                translation: point + normal * SCORCH_SURFACE_OFFSET,
                rotation: spin * alignment,
                scale: Vec3::splat(diameter),
            },
            normal,
            carrier,
            region: ClipRegion::default(),
        }
    }

    pub(super) fn grass_burn(&self, style: ScorchStyle) -> Option<GrassBurn> {
        (self.normal.dot(Vec3::Y) > 0.999).then(|| {
            GrassBurn::new(
                self.carrier,
                self.surface_point(),
                self.radius(),
                style.rotation(),
                style.mesh_index,
                self.region.clone(),
            )
        })
    }

    fn surface_point(&self) -> Vec3 {
        self.transform.translation - self.normal * SCORCH_SURFACE_OFFSET
    }

    fn radius(&self) -> f32 {
        self.transform.scale.x * 0.5
    }

    // The carrier-space half-space `normal · p <= offset`, in the mark's own plane.
    fn half_plane(&self, normal: Vec3, offset: f32) -> HalfPlane {
        let transform = &self.transform;
        let across = transform.rotation * Vec3::X * transform.scale.x;
        let along = transform.rotation * Vec3::Z * transform.scale.z;
        HalfPlane {
            normal: Vec2::new(normal.dot(across), normal.dot(along)),
            offset: offset - normal.dot(transform.translation),
        }
    }

    // Whether the face lies in the mark's plane and faces the way the mark does.
    fn lies_on(&self, face: &SolidFace) -> bool {
        face.normal.dot(self.normal) > 0.999
            && self.normal.dot(face.corners[0] - self.surface_point()).abs() <= COPLANAR_TOLERANCE
    }

    // Whether the mark's disc comes near the face at all.
    fn reaches(&self, face: &SolidFace) -> bool {
        let (min, max) = face
            .corners
            .iter()
            .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), &corner| {
                (min.min(corner), max.max(corner))
            });
        let point = self.surface_point();
        let radius = Vec3::splat(self.radius());
        min.cmple(point + radius).all() && max.cmpge(point - radius).all()
    }

    // The part of the mark's plane the face covers.
    fn footprint(&self, face: &SolidFace) -> Convex {
        let middle = face.corners.iter().sum::<Vec3>() / face.corners.len() as f32;
        face.corners
            .iter()
            .enumerate()
            .map(|(index, &start)| {
                let end = face.corners[(index + 1) % face.corners.len()];
                let mut outward = (end - start).cross(face.normal);
                if outward.dot(start - middle) < 0.0 {
                    outward = -outward;
                }
                self.half_plane(outward, outward.dot(start))
            })
            .collect()
    }

    // Keep the mark on the faces of its plane; a plane with none, such as
    // open terrain, keeps the whole disc.
    fn keep_faces(&mut self, solids: &[Solid]) {
        self.region.keep = solids
            .iter()
            .filter(|solid| solid.carrier == self.carrier)
            .flat_map(|solid| &solid.faces)
            .filter(|face| self.lies_on(face) && self.reaches(face))
            .map(|face| self.footprint(face))
            .collect();
    }
}

// The mark the blast at `world_center` leaves where it reached the ground:
// on every face around the contact that lies in its plane, and nowhere
// another solid shadows.
pub(crate) fn ground_scorch_placement(
    contact: SurfaceContact,
    solids: &[Solid],
    carriers: &Carriers,
    world_center: Vec3,
    diameter: f32,
    style: ScorchStyle,
) -> ScorchPlacement {
    let pose = carriers.pose(contact.carrier);
    let point = pose.inverse_transform_point(contact.point);
    let center = pose.inverse_transform_point(world_center);
    let mut placement = ScorchPlacement::new(point, contact.normal, contact.carrier, diameter, style);
    placement.keep_faces(solids);
    placement.region.cut = shadows(&placement, solids, center);
    placement
}

// The marks the blast at `world_center` leaves on the other planes within
// reach: one per plane, where the blast's sphere cuts it, on every face
// lying in that plane, so stacked or adjoining solids share one mark across
// their seams. A blast buried in a solid marks the faces just in front of
// it from inside. `ground` is the contact that already has its own mark.
pub(crate) fn face_scorch_placements(
    solids: &[Solid],
    carriers: &Carriers,
    world_center: Vec3,
    scorch_radius: f32,
    reach_factor: f32,
    style: ScorchStyle,
    ground: Option<SurfaceContact>,
) -> Vec<ScorchPlacement> {
    let mut placements = Vec::<ScorchPlacement>::new();
    for solid in solids {
        let pose = carriers.pose(solid.carrier);
        let center = pose.inverse_transform_point(world_center);
        let height = |face: &SolidFace| face.normal.dot(center - face.corners[0]);
        let buried = solid.faces.iter().all(|face| height(face) <= 0.0);
        for face in &solid.faces {
            let height = height(face);
            if height <= 0.0 && !(buried && height >= -BURIED_REACH) {
                continue;
            }
            if let Some(existing) = placements
                .iter_mut()
                .find(|existing| existing.carrier == solid.carrier && existing.lies_on(face))
            {
                if existing.reaches(face) {
                    let footprint = existing.footprint(face);
                    existing.region.keep.push(footprint);
                }
                continue;
            }
            let on_ground = ground.is_some_and(|ground| {
                ground.carrier == solid.carrier
                    && face.normal.dot(ground.normal) > 0.999
                    && ground
                        .normal
                        .dot(face.corners[0] - pose.inverse_transform_point(ground.point))
                        .abs()
                        <= COPLANAR_TOLERANCE
            });
            if on_ground {
                continue;
            }
            let Some(diameter) = face_scorch_diameter(scorch_radius, height.abs(), reach_factor) else {
                continue;
            };
            let mut placement = ScorchPlacement::new(
                center - face.normal * height,
                face.normal,
                solid.carrier,
                diameter,
                style,
            );
            if !placement.reaches(face) {
                continue;
            }
            placement.region.keep.push(placement.footprint(face));
            placements.push(placement);
        }
    }
    for placement in &mut placements {
        let center = carriers.pose(placement.carrier).inverse_transform_point(world_center);
        placement.region.cut = shadows(placement, solids, center);
    }
    placements
}

// Where no blast from `center` reaches on the mark's plane: what the solids
// within reach hide. Each face the blast sees shadows the pyramid from the
// blast through its edges past its plane, and together a solid's faces
// shadow exactly what it hides, so a blast reaches over a low wall and
// around a short one while a slab or a ramp hides the storey beyond it. The
// surface a mark sits on lies behind its own face and shadows nothing of it.
fn shadows(placement: &ScorchPlacement, solids: &[Solid], center: Vec3) -> Vec<Convex> {
    let mark = placement.transform.translation;
    let radius = Vec3::splat(placement.radius());
    let low = center.min(mark - radius);
    let high = center.max(mark + radius);
    solids
        .iter()
        .filter(|solid| solid.carrier == placement.carrier && solid.min.cmple(high).all() && solid.max.cmpge(low).all())
        .flat_map(|solid| &solid.faces)
        .filter(|face| face.normal.dot(center - face.corners[0]) > 0.0)
        .map(|face| shadow_of(placement, center, face))
        .collect()
}

// The shadow of a face lit from `center`: past its plane, inside the
// pyramid from `center` through its edges.
fn shadow_of(placement: &ScorchPlacement, center: Vec3, face: &SolidFace) -> Convex {
    let corners = &face.corners;
    let middle = corners.iter().sum::<Vec3>() / corners.len() as f32;
    let mut shadow = vec![placement.half_plane(face.normal, face.normal.dot(middle))];
    for (index, &start) in corners.iter().enumerate() {
        let end = corners[(index + 1) % corners.len()];
        let mut normal = (end - start).cross(center - start);
        if normal.dot(middle - start) > 0.0 {
            normal = -normal;
        }
        shadow.push(placement.half_plane(normal, normal.dot(start)));
    }
    shadow
}

fn face_scorch_diameter(scorch_radius: f32, face_distance: f32, reach_factor: f32) -> Option<f32> {
    if face_distance > scorch_radius * reach_factor {
        return None;
    }
    surface_cross_section_diameter(scorch_radius, face_distance)
}

pub(crate) fn surface_cross_section_diameter(radius: f32, surface_distance: f32) -> Option<f32> {
    if surface_distance >= radius {
        return None;
    }
    Some(2.0 * radius.mul_add(radius, -surface_distance * surface_distance).sqrt())
}

#[cfg(test)]
#[path = "tests/placement.rs"]
mod tests;
