use bevy::prelude::*;

use super::{
    clip::{ClipRegion, Convex, HalfPlane},
    marks::SCORCH_SURFACE_OFFSET,
    variants::ScorchStyle,
};
use crate::map::GrassBurn;
use common::{
    map::{Carriers, RampAxis, ramp_axis},
    math::PHYSICS_EPSILON,
    protocol::{CarrierId, Floor, MapLayout, Ramp, Wall},
};

// How far a record may sit off the mark's plane and still be the surface it lies on.
const COPLANAR_TOLERANCE: f32 = 0.02;

// A point of a surface an explosion reaches, in world space.
#[derive(Clone, Copy)]
pub(crate) struct SurfaceContact {
    pub(crate) point: Vec3,
    pub(crate) normal: Vec3,
    pub(crate) carrier: CarrierId,
}

// A mark on a surface, in the frame of the surface's carrier like the map
// record it marks, so it rides a tile with the tile. `region` is where that
// surface actually is, in the mark's own plane.
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
                self.transform.translation - self.normal * SCORCH_SURFACE_OFFSET,
                self.transform.scale.x * 0.5,
                style.rotation(),
                style.mesh_index,
                self.region.clone(),
            )
        })
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

    // The column above and below a footprint `(min_x, max_x, min_z, max_z)`.
    fn footprint(&self, bounds: (f32, f32, f32, f32)) -> Convex {
        let (min_x, max_x, min_z, max_z) = bounds;
        vec![
            self.half_plane(Vec3::NEG_X, -min_x),
            self.half_plane(Vec3::X, max_x),
            self.half_plane(Vec3::NEG_Z, -min_z),
            self.half_plane(Vec3::Z, max_z),
        ]
    }
}

// The mark the blast at `world_center` leaves where it reached the ground:
// on the coplanar floors, wall tops, and ramps around the contact, and
// nowhere a wall or a floor shadows.
pub(crate) fn ground_scorch_placement(
    contact: SurfaceContact,
    map_layout: &MapLayout,
    carriers: &Carriers,
    world_center: Vec3,
    diameter: f32,
    style: ScorchStyle,
) -> ScorchPlacement {
    let pose = carriers.pose(contact.carrier);
    let point = pose.inverse_transform_point(contact.point);
    let center = pose.inverse_transform_point(world_center);
    let mut placement = ScorchPlacement::new(point, contact.normal, contact.carrier, diameter, style);
    let radius = diameter * 0.5;
    let near = |(min_x, max_x, min_z, max_z): (f32, f32, f32, f32)| {
        min_x <= point.x + radius && max_x >= point.x - radius && min_z <= point.z + radius && max_z >= point.z - radius
    };
    let level = contact.normal.y > 0.999;
    let on_carrier = |carrier: CarrierId| carrier == contact.carrier;

    let mut keep = Vec::new();
    for floor in map_layout.floors.iter().filter(|floor| on_carrier(floor.carrier)) {
        if level && (floor.y - point.y).abs() <= COPLANAR_TOLERANCE && near(floor.bounds_xz()) {
            keep.push(placement.footprint(floor.bounds_xz()));
        }
    }
    for wall in map_layout.walls.iter().filter(|wall| on_carrier(wall.carrier)) {
        let bounds = wall_bounds_xz(wall);
        if level && (wall.y + wall.height - point.y).abs() <= COPLANAR_TOLERANCE && near(bounds) {
            keep.push(placement.footprint(bounds));
        }
    }
    for ramp in map_layout.ramps.iter().filter(|ramp| on_carrier(ramp.carrier)) {
        let Some((normal, offset)) = ramp_plane(ramp) else {
            continue;
        };
        if normal.dot(contact.normal) > 0.999
            && (normal.dot(point) - offset).abs() <= COPLANAR_TOLERANCE
            && near(ramp.bounds_xz())
        {
            keep.push(placement.footprint(ramp.bounds_xz()));
        }
    }
    placement.region.keep = keep;
    placement.region.cut = shadows(&placement, map_layout, center, radius);
    placement
}

// The marks the blast at `world_center` leaves on the wall faces within
// reach: each on its wall's rectangle, and nowhere another wall or a floor
// shadows.
pub(crate) fn wall_scorch_placements(
    map_layout: &MapLayout,
    carriers: &Carriers,
    world_center: Vec3,
    scorch_radius: f32,
    reach_factor: f32,
    style: ScorchStyle,
) -> Vec<ScorchPlacement> {
    let mut placements = Vec::<ScorchPlacement>::new();
    for wall in &map_layout.walls {
        let center = carriers.pose(wall.carrier).inverse_transform_point(world_center);
        let Some(segment) = WallSegment::new(wall) else {
            continue;
        };
        let (closest, side, signed_side_distance) = segment.closest(center);
        let normals: &[Vec3] = if signed_side_distance.abs() <= wall.width * 0.5 {
            &[side, -side]
        } else if signed_side_distance > 0.0 {
            &[side]
        } else {
            &[-side]
        };

        let bottom = wall.y;
        let top = wall.y + wall.height;
        for normal in normals {
            let point = Vec3::new(
                closest.x + normal.x * wall.width * 0.5,
                center.y.clamp(bottom, top),
                closest.z + normal.z * wall.width * 0.5,
            );
            let distance = center.distance(point);
            let Some(diameter) = wall_scorch_diameter(scorch_radius, distance, reach_factor) else {
                continue;
            };
            let mut placement = ScorchPlacement::new(point, *normal, wall.carrier, diameter, style);
            placement.region.keep = vec![vec![
                placement.half_plane(-segment.direction, -segment.start.dot(segment.direction)),
                placement.half_plane(segment.direction, segment.end.dot(segment.direction)),
                placement.half_plane(Vec3::NEG_Y, -bottom),
                placement.half_plane(Vec3::Y, top),
            ]];
            placement.region.cut = shadows(&placement, map_layout, center, diameter * 0.5);
            insert_merging_coincident(&mut placements, placement);
        }
    }
    placements
}

// Where no blast from `center` reaches on the mark's plane: what the walls,
// floors, and ramps within reach hide. Each face the blast sees shadows the
// pyramid from the blast through its edges past its plane, and together a
// body's faces shadow exactly what it hides, so a blast reaches over a low
// wall and around a short one while a floor or a ramp hides the storey beyond
// it. The surface a mark sits on lies behind its own face and shadows nothing
// of it.
fn shadows(placement: &ScorchPlacement, map_layout: &MapLayout, center: Vec3, radius: f32) -> Vec<Convex> {
    let mark = placement.transform.translation;
    let low = center.min(mark - Vec3::splat(radius));
    let high = center.max(mark + Vec3::splat(radius));
    let on_carrier = |carrier: CarrierId| carrier == placement.carrier;
    let walls = map_layout
        .walls
        .iter()
        .filter(|wall| on_carrier(wall.carrier))
        .filter_map(wall_prism);
    let floors = map_layout
        .floors
        .iter()
        .filter(|floor| on_carrier(floor.carrier))
        .map(floor_prism);
    let ramps = map_layout
        .ramps
        .iter()
        .filter(|ramp| on_carrier(ramp.carrier))
        .filter_map(ramp_prism);
    walls
        .chain(floors)
        .chain(ramps)
        .filter(|prism| {
            let (min, max) = prism.bounds();
            min.cmple(high).all() && max.cmpge(low).all()
        })
        .flat_map(|prism| prism.faces())
        .filter(|(facing, corners)| facing.dot(center - corners[0]) > 0.0)
        .map(|(facing, corners)| shadow_of(placement, center, facing, &corners))
        .collect()
}

// A convex base polygon swept along `extrusion`: a wall's or a floor's slab,
// a ramp's wedge.
struct Prism {
    base: Vec<Vec3>,
    extrusion: Vec3,
}

impl Prism {
    fn bounds(&self) -> (Vec3, Vec3) {
        self.base
            .iter()
            .flat_map(|&corner| [corner, corner + self.extrusion])
            .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), corner| {
                (min.min(corner), max.max(corner))
            })
    }

    // Every face with its outward normal.
    fn faces(&self) -> Vec<(Vec3, Vec<Vec3>)> {
        let centroid = self.base.iter().sum::<Vec3>() / self.base.len() as f32 + self.extrusion * 0.5;
        let lid: Vec<Vec3> = self.base.iter().map(|&corner| corner + self.extrusion).collect();
        let mut faces = vec![(self.extrusion, lid), (-self.extrusion, self.base.clone())];
        for (index, &start) in self.base.iter().enumerate() {
            let end = self.base[(index + 1) % self.base.len()];
            let mut normal = (end - start).cross(self.extrusion);
            if normal.dot(start - centroid) < 0.0 {
                normal = -normal;
            }
            faces.push((normal, vec![start, end, end + self.extrusion, start + self.extrusion]));
        }
        faces
    }
}

fn wall_prism(wall: &Wall) -> Option<Prism> {
    let segment = WallSegment::new(wall)?;
    let half = segment.side() * (wall.width * 0.5);
    let bottom = Vec3::Y * wall.y;
    Some(Prism {
        base: vec![
            segment.start - half + bottom,
            segment.end - half + bottom,
            segment.end + half + bottom,
            segment.start + half + bottom,
        ],
        extrusion: Vec3::Y * wall.height,
    })
}

fn floor_prism(floor: &Floor) -> Prism {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
    let bottom = floor.y - floor.thickness;
    Prism {
        base: vec![
            Vec3::new(min_x, bottom, min_z),
            Vec3::new(max_x, bottom, min_z),
            Vec3::new(max_x, bottom, max_z),
            Vec3::new(min_x, bottom, max_z),
        ],
        extrusion: Vec3::Y * floor.thickness,
    }
}

// The wedge: its cross-section along the run, swept across the width.
fn ramp_prism(ramp: &Ramp) -> Option<Prism> {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    let (min_y, max_y) = ramp.bounds_y();
    let rises = ramp.y2 >= ramp.y1;
    match ramp_axis(ramp) {
        RampAxis::X => {
            let (low_x, high_x) = if rises { (ramp.x1, ramp.x2) } else { (ramp.x2, ramp.x1) };
            ((high_x - low_x).abs() >= PHYSICS_EPSILON).then(|| Prism {
                base: vec![
                    Vec3::new(low_x, min_y, min_z),
                    Vec3::new(high_x, min_y, min_z),
                    Vec3::new(high_x, max_y, min_z),
                ],
                extrusion: Vec3::Z * (max_z - min_z),
            })
        }
        RampAxis::Z => {
            let (low_z, high_z) = if rises { (ramp.z1, ramp.z2) } else { (ramp.z2, ramp.z1) };
            ((high_z - low_z).abs() >= PHYSICS_EPSILON).then(|| Prism {
                base: vec![
                    Vec3::new(min_x, min_y, low_z),
                    Vec3::new(min_x, min_y, high_z),
                    Vec3::new(min_x, max_y, high_z),
                ],
                extrusion: Vec3::X * (max_x - min_x),
            })
        }
    }
}

// The shadow of a convex face lit from `center`: past its plane, inside the
// pyramid from `center` through its edges.
fn shadow_of(placement: &ScorchPlacement, center: Vec3, facing: Vec3, corners: &[Vec3]) -> Convex {
    let middle = corners.iter().sum::<Vec3>() / corners.len() as f32;
    let mut shadow = vec![placement.half_plane(facing, facing.dot(middle))];
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

struct WallSegment {
    start: Vec3,
    end: Vec3,
    direction: Vec3,
    length: f32,
}

impl WallSegment {
    fn new(wall: &Wall) -> Option<Self> {
        let start = Vec3::new(wall.x1, 0.0, wall.z1);
        let end = Vec3::new(wall.x2, 0.0, wall.z2);
        let length = start.distance(end);
        (length > f32::EPSILON).then(|| Self {
            start,
            end,
            direction: (end - start) / length,
            length,
        })
    }

    fn side(&self) -> Vec3 {
        Vec3::new(-self.direction.z, 0.0, self.direction.x)
    }

    // The segment point nearest `p` on the ground plane, the side axis, and
    // `p`'s signed distance along it.
    fn closest(&self, p: Vec3) -> (Vec3, Vec3, f32) {
        let flat = Vec3::new(p.x, 0.0, p.z);
        let progress = ((flat - self.start).dot(self.direction) / self.length).clamp(0.0, 1.0);
        let closest = self.start + (self.end - self.start) * progress;
        let side = self.side();
        (closest, side, (flat - closest).dot(side))
    }
}

fn wall_bounds_xz(wall: &Wall) -> (f32, f32, f32, f32) {
    let half = wall.width * 0.5;
    (
        wall.x1.min(wall.x2) - half,
        wall.x1.max(wall.x2) + half,
        wall.z1.min(wall.z2) - half,
        wall.z1.max(wall.z2) + half,
    )
}

// A ramp's surface as `normal · p = offset`.
fn ramp_plane(ramp: &Ramp) -> Option<(Vec3, f32)> {
    let rise = ramp.y2 - ramp.y1;
    let normal = match ramp_axis(ramp) {
        RampAxis::X => {
            let run = ramp.x2 - ramp.x1;
            (run.abs() >= PHYSICS_EPSILON).then(|| Vec3::new(-rise / run, 1.0, 0.0))
        }
        RampAxis::Z => {
            let run = ramp.z2 - ramp.z1;
            (run.abs() >= PHYSICS_EPSILON).then(|| Vec3::new(0.0, 1.0, -rise / run))
        }
    }?
    .normalize();
    Some((normal, normal.dot(Vec3::new(ramp.x1, ramp.y1, ramp.z1))))
}

fn wall_scorch_diameter(scorch_radius: f32, wall_distance: f32, reach_factor: f32) -> Option<f32> {
    if wall_distance > scorch_radius * reach_factor {
        return None;
    }
    surface_cross_section_diameter(scorch_radius, wall_distance)
}

pub(crate) fn surface_cross_section_diameter(radius: f32, surface_distance: f32) -> Option<f32> {
    if surface_distance >= radius {
        return None;
    }
    Some(2.0 * radius.mul_add(radius, -surface_distance * surface_distance).sqrt())
}

// Adjoining wall sections meet at one face point and give the same mark
// twice, each cut to its own rectangle; one mark on both rectangles covers
// the seam.
fn insert_merging_coincident(placements: &mut Vec<ScorchPlacement>, candidate: ScorchPlacement) {
    if let Some(existing) = placements.iter_mut().find(|existing| {
        existing.carrier == candidate.carrier
            && existing.normal.dot(candidate.normal) > 0.999
            && existing
                .transform
                .translation
                .distance_squared(candidate.transform.translation)
                < 1e-6
    }) {
        existing.region.keep.extend(candidate.region.keep);
        return;
    }
    placements.push(candidate);
}

#[cfg(test)]
#[path = "tests/placement.rs"]
mod tests;
