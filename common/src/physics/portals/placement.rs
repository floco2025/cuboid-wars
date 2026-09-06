use std::sync::OnceLock;

use bevy_math::{Mat3, Quat, Vec3};
use rapier3d::{
    parry::{query::intersection_test, shape::Cuboid},
    prelude::{Pose, SharedShape, Vector},
};

use super::PortalFrame;
use crate::{
    constants::{
        PORTAL_FIXTURE_PLANE_DEPTH, PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_LIGHT_CLEARANCE,
        PORTAL_PLATE_CLEARANCE, PORTAL_RIM_SCALE, PORTAL_STANDABLE_NORMAL_Y, PORTAL_UP_DEGENERACY_LIMIT,
    },
    map::MovingFloors,
    math::direction_from_yaw_pitch,
    physics::CollisionWorld,
    protocol::{MapLayout, MovingFloorId, Portal, PortalEnd, PortalPairId, WallLight},
};

// Where a validated portal shot lands: the aperture center (world space),
// outward surface normal, the frame yaw (the shooter's, quarter-turn-snapped
// on vertical-normal surfaces), and the moving floor it sits on, if any.
#[derive(Debug, Clone, Copy)]
pub struct PortalPlacement {
    pub pos: Vec3,
    pub normal: Vec3,
    pub yaw: f32,
    pub anchor: Option<MovingFloorId>,
}

impl PortalPlacement {
    // The wire portal: `pos` becomes relative to the tile's surface center
    // when anchored, so it stays put while the tile moves.
    #[must_use]
    pub fn portal(&self, pair: PortalPairId, end: PortalEnd, floors: &MovingFloors) -> Portal {
        let pos = self.pos - floors.anchor_center(self.anchor);
        Portal {
            pair,
            end,
            pos: pos.into(),
            nx: self.normal.x,
            ny: self.normal.y,
            nz: self.normal.z,
            yaw: self.yaw,
            anchor: self.anchor,
        }
    }
}

// What backs a fitting aperture: the static world, or one moving floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortalBacking {
    Static,
    MovingFloor(MovingFloorId),
}

impl PortalBacking {
    const fn anchor(self) -> Option<MovingFloorId> {
        match self {
            Self::Static => None,
            Self::MovingFloor(id) => Some(id),
        }
    }
}

// The one placement path, shared verbatim: the client runs it to decide fire
// vs dry-fire before sending, the server to authoritatively place. Same
// inputs (map geometry, fixtures, each side's own tile poses), so both reach
// the same answer except within a tick of tile motion at a tile's edge.
#[must_use]
pub fn compute_portal_placement(
    origin: Vec3,
    direction: Vec3,
    yaw: f32,
    range: f32,
    collision_world: &CollisionWorld,
    map_layout: &MapLayout,
    floors: &MovingFloors,
) -> Option<PortalPlacement> {
    let (hit, hit_tile) = collision_world.portal_surface_along_ray(origin, direction, range)?;
    let yaw = portal_placement_yaw(hit.normal, yaw);
    let frame = PortalFrame::from_surface(hit.point, hit.normal, yaw);
    let (pos, backing) = if let Some(backing) = portal_fits(&frame, collision_world, map_layout) {
        (hit.point, backing)
    } else if let Some(centered) =
        hit_tile.and_then(|id| tile_centered(&frame, id, collision_world, map_layout, floors))
    {
        centered
    } else {
        nudged_center(&frame, collision_world, map_layout)?
    };
    Some(PortalPlacement {
        pos,
        normal: hit.normal,
        yaw,
        anchor: backing.anchor(),
    })
}

// A tile is a small surface: a shot on one that does not fit where it lands
// goes to the tile's middle first, which is where a lift's portal belongs,
// and only then bumps around the plane.
fn tile_centered(
    frame: &PortalFrame,
    id: MovingFloorId,
    collision_world: &CollisionWorld,
    map_layout: &MapLayout,
    floors: &MovingFloors,
) -> Option<(Vec3, PortalBacking)> {
    let tile_center = floors.anchor_center(Some(id));
    let center = tile_center + frame.normal * (frame.center - tile_center).dot(frame.normal);
    let candidate = PortalFrame { center, ..*frame };
    portal_fits(&candidate, collision_world, map_layout).map(|backing| (center, backing))
}

// Vertical portals take their in-plane up from the shooter's yaw; snapping
// it to quarter turns keeps a hand-placed floor/ceiling pair from
// precessing the mapped offset — and the traveler's view — a little on
// every pass of a fall loop. Wall yaws pass through: their frames ignore it.
fn portal_placement_yaw(normal: Vec3, face_yaw: f32) -> f32 {
    if normal.normalize().y.abs() < PORTAL_UP_DEGENERACY_LIMIT {
        face_yaw
    } else {
        (face_yaw / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2
    }
}

// Portal-2-style placement bump: an aperture that doesn't fit where the
// shot lands slides along the surface plane to the nearest nearby spot that
// does (nearest ring first, straight up tried first within each ring); only
// when nothing within reach fits does the shot fizzle.
const NUDGE_STEP: f32 = 0.125;
const NUDGE_MAX_DISTANCE: f32 = 1.5;
const NUDGE_DIRECTIONS: usize = 16;

fn nudged_center(
    frame: &PortalFrame,
    collision_world: &CollisionWorld,
    map_layout: &MapLayout,
) -> Option<(Vec3, PortalBacking)> {
    let steps = (NUDGE_MAX_DISTANCE / NUDGE_STEP) as usize;
    for step in 1..=steps {
        let radius = step as f32 * NUDGE_STEP;
        for direction in 0..NUDGE_DIRECTIONS {
            let angle =
                std::f32::consts::FRAC_PI_2 + direction as f32 / NUDGE_DIRECTIONS as f32 * std::f32::consts::TAU;
            let center = frame.center + frame.right * (radius * angle.cos()) + frame.up * (radius * angle.sin());
            let candidate = PortalFrame { center, ..*frame };
            if let Some(backing) = portal_fits(&candidate, collision_world, map_layout) {
                return Some((center, backing));
            }
        }
    }
    None
}

const FIT_SAMPLE_OFFSET: f32 = 0.13;
const FIT_BACKING_NORMAL_DOT: f32 = 0.999;
const FIT_RIM_SAMPLES: usize = 8;
// The front-clearance slab: how far off the plane it starts and how deep it
// reaches into the room.
const FIT_FRONT_GAP: f32 = 0.02;
const FIT_FRONT_DEPTH: f32 = 0.3;
const FIT_FRONT_RIM_SEGMENTS: usize = 64;

// The portal must actually work as a hole: every sample around its visible
// rim needs solid surface BEHIND the plane (no hanging past an edge) and
// clear space IN FRONT of it (no floor slab, powered light bridge, or
// abutting wall cutting through the oval). On top of the geometry, the
// aperture must not cover surface fixtures: wall lights, and pressure plates
// for standable portals. Returns what backs the fitting aperture.
fn portal_fits(frame: &PortalFrame, collision_world: &CollisionWorld, map_layout: &MapLayout) -> Option<PortalBacking> {
    // Sweep the oval itself so geometry outside the visible rim cannot make
    // a portal float above a ramp, while geometry crossing the opening still
    // rejects between the backing probes.
    let rotation = Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal));
    let front_center = frame.center + frame.normal * (FIT_FRONT_GAP + FIT_FRONT_DEPTH / 2.0);
    if collision_world.oriented_shape_overlaps_surface(front_center, rotation, front_clearance_shape().as_ref()) {
        return None;
    }
    // Each sample must meet a surface parallel to the shot face, and every
    // sample the same kind of backing. Otherwise a ramp lip and its upper
    // floor can jointly masquerade as one backing, and an aperture
    // straddling a tile's edge would be anchored to a tile that carries
    // only part of it.
    let mut backing = None;
    for sample in aperture_samples(frame) {
        let probe_start = sample + frame.normal * FIT_SAMPLE_OFFSET;
        let (hit, tile) =
            collision_world.portal_surface_along_ray(probe_start, -frame.normal, 2.0 * FIT_SAMPLE_OFFSET)?;
        if hit.normal.dot(frame.normal) < FIT_BACKING_NORMAL_DOT {
            return None;
        }
        let sample_backing = tile.map_or(PortalBacking::Static, PortalBacking::MovingFloor);
        match backing {
            None => backing = Some(sample_backing),
            Some(first) if first != sample_backing => return None,
            Some(_) => {}
        }
    }
    for light in &map_layout.wall_lights {
        if wall_light_blocks(frame, light) {
            return None;
        }
    }
    if frame.normal.y > PORTAL_STANDABLE_NORMAL_Y {
        for plate in &map_layout.pressure_plates {
            let center = Vec3::new(plate.center_x, plate.center_y, plate.center_z);
            if fixture_blocks(frame, center, PORTAL_PLATE_CLEARANCE) {
                return None;
            }
        }
    }
    backing
}

fn front_clearance_shape() -> &'static SharedShape {
    static SHAPE: OnceLock<SharedShape> = OnceLock::new();
    SHAPE.get_or_init(|| {
        let mut points = Vec::with_capacity(FIT_FRONT_RIM_SEGMENTS * 2);
        for depth in [-FIT_FRONT_DEPTH / 2.0, FIT_FRONT_DEPTH / 2.0] {
            for i in 0..FIT_FRONT_RIM_SEGMENTS {
                let angle = i as f32 / FIT_FRONT_RIM_SEGMENTS as f32 * std::f32::consts::TAU;
                points.push(Vector::new(
                    PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE * angle.cos(),
                    PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE * angle.sin(),
                    depth,
                ));
            }
        }
        SharedShape::convex_hull(&points).expect("portal front-clearance hull is degenerate")
    })
}

fn aperture_samples(frame: &PortalFrame) -> impl Iterator<Item = Vec3> {
    let center = frame.center;
    let (right, up) = (frame.right, frame.up);
    std::iter::once(center).chain((0..FIT_RIM_SAMPLES).map(move |i| {
        let angle = i as f32 / FIT_RIM_SAMPLES as f32 * std::f32::consts::TAU;
        center
            + right * (PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE * angle.cos())
            + up * (PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE * angle.sin())
    }))
}

fn wall_light_blocks(frame: &PortalFrame, light: &WallLight) -> bool {
    let light_normal = direction_from_yaw_pitch(light.yaw, 0.0);
    light_normal.dot(frame.normal) >= FIT_BACKING_NORMAL_DOT
        && fixture_blocks(frame, Vec3::from(light.pos), PORTAL_LIGHT_CLEARANCE)
}

// A fixture blocks the aperture when it sits near the portal plane inside
// the clearance-grown oval.
fn fixture_blocks(frame: &PortalFrame, fixture: Vec3, clearance: f32) -> bool {
    let offset = fixture - frame.center;
    if offset.dot(frame.normal).abs() > PORTAL_FIXTURE_PLANE_DEPTH {
        return false;
    }
    let across = offset.dot(frame.right) / (PORTAL_HALF_WIDTH + clearance);
    let along_up = offset.dot(frame.up) / (PORTAL_HALF_HEIGHT + clearance);
    across * across + along_up * along_up <= 1.0
}

const PORTAL_OVERLAP_HALF_DEPTH: f32 = 0.05;

// Whether the candidate crosses another end where the ends are right now;
// an end that later rides its tile into a static one is not foreseen.
#[must_use]
pub fn portal_placement_overlaps(
    placement: &PortalPlacement,
    pair: PortalPairId,
    end: PortalEnd,
    existing: &[Portal],
    floors: &MovingFloors,
) -> bool {
    let candidate = PortalFrame::from_surface(placement.pos, placement.normal, placement.yaw);
    existing.iter().any(|portal| {
        (portal.pair, portal.end) != (pair, end)
            && portal_frames_overlap(&candidate, &PortalFrame::from_portal(portal, floors))
    })
}

fn portal_frames_overlap(a: &PortalFrame, b: &PortalFrame) -> bool {
    let shape = Cuboid::new(Vector::new(
        PORTAL_HALF_WIDTH,
        PORTAL_HALF_HEIGHT,
        PORTAL_OVERLAP_HALF_DEPTH,
    ));
    intersection_test(&portal_pose(a), &shape, &portal_pose(b), &shape).is_ok_and(|overlaps| overlaps)
}

fn portal_pose(frame: &PortalFrame) -> Pose {
    let rotation = Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal));
    let axis_angle = rotation.to_scaled_axis();
    Pose::new(
        Vector::new(frame.center.x, frame.center.y, frame.center.z),
        Vector::new(axis_angle.x, axis_angle.y, axis_angle.z),
    )
}
