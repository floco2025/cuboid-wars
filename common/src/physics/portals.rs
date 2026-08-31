use bevy_ecs::prelude::*;
use bevy_math::{Mat3, Quat, Vec3};

use super::CollisionWorld;
use crate::{
    config::CharacterPhysicsConfig,
    constants::{
        LEVEL_HEIGHT, PORTAL_FIXTURE_PLANE_DEPTH, PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_LIGHT_CLEARANCE,
        PORTAL_PLATE_CLEARANCE, PORTAL_PROJECTILE_EXIT_STANDOFF, PORTAL_STANDABLE_NORMAL_Y, PORTAL_UP_DEGENERACY_LIMIT,
    },
    math::direction_from_yaw_pitch,
    protocol::{MapLayout, Portal, PortalEnd},
};
use rapier3d::prelude::ColliderHandle;

// Orthonormal aperture frame of one portal end: `normal` points out of the
// surface into the room, `up`/`right` span the plane with (right, up, normal)
// right-handed. Everything downstream — traversal, triggers, rendering —
// reads this frame; nothing asks what kind of surface the portal is on.
#[derive(Debug, Clone, Copy)]
pub struct PortalFrame {
    pub center: Vec3,
    pub normal: Vec3,
    pub up: Vec3,
    pub right: Vec3,
}

impl PortalFrame {
    #[must_use]
    pub fn from_portal(portal: &Portal) -> Self {
        Self::from_surface(
            portal.pos.into(),
            Vec3::new(portal.nx, portal.ny, portal.nz),
            portal.yaw,
        )
    }

    #[must_use]
    pub fn from_surface(center: Vec3, normal: Vec3, yaw: f32) -> Self {
        let normal = normal.normalize();
        // World-up projected onto the plane orients the frame; only a
        // near-vertical normal is degenerate, and there the shooter's
        // placement yaw supplies the in-plane up instead.
        let reference = if normal.y.abs() < PORTAL_UP_DEGENERACY_LIMIT {
            Vec3::Y
        } else {
            direction_from_yaw_pitch(yaw, 0.0)
        };
        let up = (reference - normal * reference.dot(normal)).normalize();
        Self {
            center,
            normal,
            up,
            right: up.cross(normal),
        }
    }
}

// Where a validated portal shot lands: the aperture center and outward
// surface normal.
#[derive(Debug, Clone, Copy)]
pub struct PortalPlacement {
    pub pos: Vec3,
    pub normal: Vec3,
}

// The one placement path, shared verbatim: the client runs it to decide fire
// vs dry-fire before sending, the server to authoritatively place. Same
// static inputs (map geometry, fixtures), so both reach the same answer.
#[must_use]
pub fn compute_portal_placement(
    origin: Vec3,
    direction: Vec3,
    yaw: f32,
    range: f32,
    collision_world: &CollisionWorld,
    map_layout: &MapLayout,
) -> Option<PortalPlacement> {
    let hit = collision_world.world_surface_along_ray(origin, direction, range)?;
    let frame = PortalFrame::from_surface(hit.point, hit.normal, yaw);
    if portal_fits(&frame, collision_world, map_layout) {
        return Some(PortalPlacement {
            pos: hit.point,
            normal: hit.normal,
        });
    }
    nudged_placement(&frame, collision_world, map_layout)
}

// Portal-2-style placement bump: an aperture that doesn't fit where the
// shot lands slides along the surface plane to the nearest nearby spot that
// does (nearest ring first, straight up tried first within each ring); only
// when nothing within reach fits does the shot fizzle.
const NUDGE_STEP: f32 = 0.25;
const NUDGE_MAX_DISTANCE: f32 = 1.5;
const NUDGE_DIRECTIONS: usize = 16;

fn nudged_placement(
    frame: &PortalFrame,
    collision_world: &CollisionWorld,
    map_layout: &MapLayout,
) -> Option<PortalPlacement> {
    let steps = (NUDGE_MAX_DISTANCE / NUDGE_STEP) as usize;
    for step in 1..=steps {
        let radius = step as f32 * NUDGE_STEP;
        for direction in 0..NUDGE_DIRECTIONS {
            let angle =
                std::f32::consts::FRAC_PI_2 + direction as f32 / NUDGE_DIRECTIONS as f32 * std::f32::consts::TAU;
            let center = frame.center + frame.right * (radius * angle.cos()) + frame.up * (radius * angle.sin());
            let candidate = PortalFrame { center, ..*frame };
            if portal_fits(&candidate, collision_world, map_layout) {
                return Some(PortalPlacement {
                    pos: center,
                    normal: frame.normal,
                });
            }
        }
    }
    None
}

// Probe ball for the fit test and its standoff from the aperture plane.
const FIT_SAMPLE_RADIUS: f32 = 0.1;
const FIT_SAMPLE_OFFSET: f32 = FIT_SAMPLE_RADIUS + 0.03;
const FIT_RIM_SAMPLES: usize = 8;
// The front-clearance slab: how far off the plane it starts and how deep it
// reaches into the room.
const FIT_FRONT_GAP: f32 = 0.02;
const FIT_FRONT_DEPTH: f32 = 0.3;

// The aperture must actually work as a hole: every sample across it needs
// solid surface BEHIND the plane (no hanging past an edge) and clear space
// IN FRONT of it (no floor slab or abutting wall cutting through the oval —
// this is what stops a portal placed too low for a body to fit). On top of
// the geometry, the aperture must not cover surface fixtures: wall lights,
// and pressure plates for standable portals.
fn portal_fits(frame: &PortalFrame, collision_world: &CollisionWorld, map_layout: &MapLayout) -> bool {
    // One oriented box sweeps the whole slab in front of the aperture, so
    // geometry crossing the oval anywhere — a wall standing on a floor
    // aperture, a slab clipping one edge — rejects, not only geometry near
    // a probe point.
    let rotation = Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal));
    let front_center = frame.center + frame.normal * (FIT_FRONT_GAP + FIT_FRONT_DEPTH / 2.0);
    if collision_world.oriented_cuboid_overlaps_world(
        front_center,
        Vec3::new(PORTAL_HALF_WIDTH, PORTAL_HALF_HEIGHT, FIT_FRONT_DEPTH / 2.0),
        rotation,
    ) {
        return false;
    }
    // Backing stays per-sample: an any-overlap query cannot express "solid
    // everywhere behind the plane".
    for sample in aperture_samples(frame) {
        if !collision_world.ball_overlaps_world(sample - frame.normal * FIT_SAMPLE_OFFSET, FIT_SAMPLE_RADIUS) {
            return false;
        }
    }
    for light in &map_layout.wall_lights {
        if fixture_blocks(frame, Vec3::from(light.pos), PORTAL_LIGHT_CLEARANCE) {
            return false;
        }
    }
    if frame.normal.y > PORTAL_STANDABLE_NORMAL_Y {
        for plate in &map_layout.pressure_plates {
            let center = Vec3::new(plate.center_x, f32::from(plate.level) * LEVEL_HEIGHT, plate.center_z);
            if fixture_blocks(frame, center, PORTAL_PLATE_CLEARANCE) {
                return false;
            }
        }
    }
    true
}

fn aperture_samples(frame: &PortalFrame) -> impl Iterator<Item = Vec3> {
    let center = frame.center;
    let (right, up) = (frame.right, frame.up);
    std::iter::once(center).chain((0..FIT_RIM_SAMPLES).map(move |i| {
        let angle = i as f32 / FIT_RIM_SAMPLES as f32 * std::f32::consts::TAU;
        center + right * (PORTAL_HALF_WIDTH * angle.cos()) + up * (PORTAL_HALF_HEIGHT * angle.sin())
    }))
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

// Map a vector through a pair: decompose in the entry frame, re-emit in the
// exit frame with right and normal negated — a 180° turn about `up` in frame
// space, so a proper rotation (never a mirror) that preserves length.
#[must_use]
pub fn traverse_vector(entry: &PortalFrame, exit: &PortalFrame, v: Vec3) -> Vec3 {
    let across = v.dot(entry.right);
    let along_up = v.dot(entry.up);
    let into = v.dot(entry.normal);
    exit.up * along_up - exit.right * across - exit.normal * into
}

// Facing through the pair, projected back to a yaw. Square-on entries can map
// the facing vertical; the fallbacks pick the stable direction that remains —
// out of a tilted exit, or along a vertical exit's in-plane up.
#[must_use]
fn traverse_yaw(entry: &PortalFrame, exit: &PortalFrame, yaw: f32) -> f32 {
    let mapped = traverse_vector(entry, exit, direction_from_yaw_pitch(yaw, 0.0));
    if mapped.x * mapped.x + mapped.z * mapped.z > 0.01 {
        mapped.x.atan2(mapped.z)
    } else if exit.normal.y.abs() < PORTAL_UP_DEGENERACY_LIMIT {
        exit.normal.x.atan2(exit.normal.z)
    } else {
        exit.up.x.atan2(exit.up.z)
    }
}

fn in_aperture(offset_from_center: Vec3, frame: &PortalFrame) -> bool {
    let across = offset_from_center.dot(frame.right) / PORTAL_HALF_WIDTH;
    let along_up = offset_from_center.dot(frame.up) / PORTAL_HALF_HEIGHT;
    across * across + along_up * along_up <= 1.0
}

// Characters get the full rectangle, not the oval: a standing body's center
// sits well below a wall portal's center, where the oval narrows laterally
// to a sliver of the visible width. The forgiving gate keeps the whole
// drawn width walkable; projectiles keep the exact oval.
fn in_character_aperture(offset_from_center: Vec3, frame: &PortalFrame) -> bool {
    offset_from_center.dot(frame.right).abs() <= PORTAL_HALF_WIDTH
        && offset_from_center.dot(frame.up).abs() <= PORTAL_HALF_HEIGHT
}

// The character's occupied box — feet to collider top — not the collider
// itself: the collider floats `bottom_y_offset` above the feet, and the
// trigger must read "standing on the aperture" as contact.
fn body_half_extents(physics: CharacterPhysicsConfig) -> Vec3 {
    Vec3::new(
        physics.collider.width / 2.0,
        physics.collider.top_y_offset() / 2.0,
        physics.collider.depth / 2.0,
    )
}

fn body_support(half_extents: Vec3, direction: Vec3) -> f32 {
    direction.abs().dot(half_extents)
}

// Outcome of a character crossing a portal plane this tick.
#[derive(Debug, Clone, Copy)]
pub struct CharacterPortalHop {
    // New entity origin (feet): the entry pose mapped continuously through
    // the pair — aperture offset carried (clamped to keep the body inside
    // the exit aperture) and the crossing penetration carried, so
    // pass-through is seamless.
    pub origin: Vec3,
    pub yaw: f32,
    pub vertical_velocity: f32,
    // Horizontal momentum carry, expressed as knockback — the only persistent
    // horizontal velocity in the character model. Control velocity is
    // excluded: the held keys re-supply it in the exit frame next tick, and
    // carrying it too would double it.
    pub knockback: Vec3,
    // The gate that was crossed, for camera view mapping.
    pub entry: PortalFrame,
    pub exit: PortalFrame,
}

// Fraction `t` of a projectile tick at which the swept ball touches an entry
// plane, plus where and how it continues from the linked exit.
#[derive(Debug, Clone, Copy)]
pub struct ProjectileHop {
    pub t: f32,
    pub entry_point: Vec3,
    pub exit_pos: Vec3,
    pub exit_velocity: Vec3,
}

// One linked portal end: its aperture frame plus the world colliders that
// back it. While a character's body overlaps the aperture, the backing is
// excluded from its collision and support queries — that is what makes the
// surface passable.
#[derive(Debug, Clone)]
struct PortalGate {
    frame: PortalFrame,
    backing: Vec<ColliderHandle>,
}

#[derive(Debug, Clone)]
struct PortalPairGates {
    a: PortalGate,
    b: PortalGate,
}

// How deep behind the plane the backing lookup reaches (covers wall and
// floor slabs), and how far to each side of the plane a body keeps its
// exclusion — entry, crossing, and emergence all stay collision-free.
const BACKING_DEPTH: f32 = 0.5;
const TRANSIT_MARGIN: f32 = 0.3;
// Longest per-tick displacement legitimate motion can produce (terminal
// fall plus knockback, with slack); larger jumps are external teleports.
const MAX_CROSSING_STEP: f32 = 4.0;

// Every complete portal pair in the world. Portals are shot-placed at
// runtime, so they cannot live in the immutable `CollisionWorld`; both sides
// hold this resource and rebuild it from the replicated portal list, through
// this one constructor, so their frames agree exactly.
#[derive(Resource, Debug, Default, Clone)]
pub struct PortalSet {
    pairs: Vec<PortalPairGates>,
}

impl PortalSet {
    // Pairs each owner's A and B ends; a half-placed pair is inert. The
    // collision world supplies each aperture's backing colliders — both
    // sides derive them from the same static world, so the sets agree.
    #[must_use]
    pub fn rebuild(portals: &[Portal], collision_world: &CollisionWorld) -> Self {
        let mut pairs = Vec::new();
        for a in portals.iter().filter(|portal| portal.end == PortalEnd::A) {
            let Some(b) = portals
                .iter()
                .find(|portal| portal.owner == a.owner && portal.end == PortalEnd::B)
            else {
                continue;
            };
            pairs.push(PortalPairGates {
                a: gate_from_portal(a, collision_world),
                b: gate_from_portal(b, collision_world),
            });
        }
        Self { pairs }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    // The (entry, exit) frames of the gate a teleport used, recovered from
    // its endpoints — the teleport cue carries positions, not portal ids.
    #[must_use]
    pub fn traversal_frames(&self, from: Vec3, to: Vec3) -> Option<(PortalFrame, PortalFrame)> {
        self.gates()
            .map(|(entry, exit)| {
                let score = from.distance_squared(entry.frame.center) + to.distance_squared(exit.frame.center);
                (score, (entry.frame, exit.frame))
            })
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, frames)| frames)
    }

    fn gates(&self) -> impl Iterator<Item = (&PortalGate, &PortalGate)> {
        self.pairs
            .iter()
            .flat_map(|pair| [(&pair.a, &pair.b), (&pair.b, &pair.a)])
    }

    // World colliders a character at `origin` may pass through right now:
    // the backing of every linked aperture whose front corridor the body is
    // in. One-sided and front-unbounded — the hole is open to any body in
    // front of it (a fast fall must not land on the surface in the tick
    // before contact), and solid from behind past a small emergence margin.
    #[must_use]
    pub fn collision_exclusions(&self, origin: Vec3, physics: CharacterPhysicsConfig) -> Vec<ColliderHandle> {
        if self.pairs.is_empty() {
            return Vec::new();
        }
        let half_extents = body_half_extents(physics);
        let center = origin + Vec3::Y * half_extents.y;
        let mut excluded = Vec::new();
        for (gate, _) in self.gates() {
            let offset = center - gate.frame.center;
            let behind_reach = body_support(half_extents, gate.frame.normal) + TRANSIT_MARGIN;
            if offset.dot(gate.frame.normal) > -behind_reach && in_character_aperture(offset, &gate.frame) {
                excluded.extend_from_slice(&gate.backing);
            }
        }
        excluded
    }

    // Plane-crossing trigger: the tick a body's center passes from the
    // front of a linked aperture to its back (the backing colliders were
    // excluded, so it physically sank in), it continues from the paired
    // end. Position maps continuously — aperture offset carried (clamped so
    // the body stays inside the exit aperture; this is also what lets a
    // steering player escape a fall chain) and penetration carried — and so
    // does velocity, split into the vertical component and a knockback
    // shove, the character model's only persistent channels.
    #[must_use]
    pub fn character_hop(
        &self,
        from: Vec3,
        to: Vec3,
        physics: CharacterPhysicsConfig,
        control_velocity: Vec3,
        knockback: Vec3,
        vertical_velocity: f32,
        yaw: f32,
        knockback_cap: f32,
    ) -> Option<CharacterPortalHop> {
        // A crossing is continuous motion; a jump no single tick of movement
        // can produce is an external teleport (a respawn-style rescue) that
        // happens to sign-cross a plane, not a portal entry.
        if from.distance_squared(to) > MAX_CROSSING_STEP * MAX_CROSSING_STEP {
            return None;
        }
        let half_extents = body_half_extents(physics);
        let center_from = from + Vec3::Y * half_extents.y;
        let center_to = to + Vec3::Y * half_extents.y;
        let velocity = control_velocity + knockback + Vec3::Y * vertical_velocity;
        for (entry_gate, exit_gate) in self.gates() {
            let entry = &entry_gate.frame;
            let exit = &exit_gate.frame;
            let from_distance = (center_from - entry.center).dot(entry.normal);
            let to_distance = (center_to - entry.center).dot(entry.normal);
            if !(from_distance > 0.0 && to_distance <= 0.0) {
                continue;
            }
            let offset = center_to - entry.center;
            if !in_character_aperture(offset, entry) {
                continue;
            }
            let across_limit = (PORTAL_HALF_WIDTH - body_support(half_extents, exit.right)).max(0.0);
            let up_limit = (PORTAL_HALF_HEIGHT - body_support(half_extents, exit.up)).max(0.0);
            let exit_center = exit.center
                + exit.right * (-offset.dot(entry.right)).clamp(-across_limit, across_limit)
                + exit.up * offset.dot(entry.up).clamp(-up_limit, up_limit)
                + exit.normal * (-to_distance);
            let carry = traverse_vector(entry, exit, knockback + Vec3::Y * vertical_velocity);
            return Some(CharacterPortalHop {
                origin: exit_center - Vec3::Y * half_extents.y,
                yaw: traverse_yaw(entry, exit, yaw),
                vertical_velocity: traverse_vector(entry, exit, velocity).y,
                knockback: Vec3::new(carry.x, 0.0, carry.z).clamp_length_max(knockback_cap),
                entry: *entry,
                exit: *exit,
            });
        }
        None
    }

    // First portal plane the swept ball touches from the front this tick,
    // inside the aperture. Requiring a front-side start means an exiting
    // projectile (standoff, outward velocity) can never re-fire the same end.
    #[must_use]
    pub fn projectile_hop(&self, pos: Vec3, velocity: Vec3, delta: f32, radius: f32) -> Option<ProjectileHop> {
        let translation = velocity * delta;
        let mut best: Option<ProjectileHop> = None;
        for (entry_gate, exit_gate) in self.gates() {
            let entry = &entry_gate.frame;
            let exit = &exit_gate.frame;
            let start_distance = (pos - entry.center).dot(entry.normal);
            let end_distance = (pos + translation - entry.center).dot(entry.normal);
            if start_distance <= radius || end_distance > radius {
                continue;
            }
            let t = (start_distance - radius) / (start_distance - end_distance);
            if best.as_ref().is_some_and(|hop| hop.t <= t) {
                continue;
            }
            let entry_point = pos + translation * t;
            let offset = entry_point - entry.center;
            if !in_aperture(offset, entry) {
                continue;
            }
            // Exit at the frame-mirrored aperture offset, standing the ball
            // off the exit plane.
            let exit_pos = exit.center - exit.right * offset.dot(entry.right)
                + exit.up * offset.dot(entry.up)
                + exit.normal * (radius + PORTAL_PROJECTILE_EXIT_STANDOFF);
            best = Some(ProjectileHop {
                t,
                entry_point,
                exit_pos,
                exit_velocity: traverse_vector(entry, exit, velocity),
            });
        }
        best
    }
}

fn gate_from_portal(portal: &Portal, collision_world: &CollisionWorld) -> PortalGate {
    let frame = PortalFrame::from_portal(portal);
    let rotation = Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal));
    let backing = collision_world.colliders_overlapping_oriented_cuboid(
        frame.center - frame.normal * (BACKING_DEPTH / 2.0),
        Vec3::new(PORTAL_HALF_WIDTH, PORTAL_HALF_HEIGHT, BACKING_DEPTH / 2.0),
        rotation,
    );
    PortalGate { frame, backing }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;
    use crate::{config::GameplayConfig, math::angle_delta_radians, protocol::PlayerId};

    const CAP: f32 = 22.5;

    fn portal(end: PortalEnd, pos: Vec3, normal: Vec3, yaw: f32) -> Portal {
        Portal {
            owner: PlayerId(1),
            end,
            pos: pos.into(),
            nx: normal.x,
            ny: normal.y,
            nz: normal.z,
            yaw,
        }
    }

    fn empty_world() -> CollisionWorld {
        CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default())
    }

    fn pair(a_pos: Vec3, a_normal: Vec3, b_pos: Vec3, b_normal: Vec3) -> PortalSet {
        PortalSet::rebuild(
            &[
                portal(PortalEnd::A, a_pos, a_normal, 0.0),
                portal(PortalEnd::B, b_pos, b_normal, 0.0),
            ],
            &empty_world(),
        )
    }

    fn frames(set: &PortalSet) -> (&PortalFrame, &PortalFrame) {
        let pair = set.pairs.first().expect("portal set has no pair");
        (&pair.a.frame, &pair.b.frame)
    }

    fn player_physics() -> CharacterPhysicsConfig {
        GameplayConfig::load_default()
            .expect("default gameplay config should load")
            .player
            .physics()
    }

    fn assert_frame_valid(frame: &PortalFrame) {
        assert!((frame.normal.length() - 1.0).abs() < 1e-5);
        assert!((frame.up.length() - 1.0).abs() < 1e-5);
        assert!((frame.right.length() - 1.0).abs() < 1e-5);
        assert!(frame.up.dot(frame.normal).abs() < 1e-5);
        assert!((frame.right.cross(frame.up) - frame.normal).length() < 1e-5);
    }

    #[test]
    fn frames_are_right_handed_orthonormal_for_any_normal() {
        for normal in [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Z,
            Vec3::NEG_Z,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::new(0.0, 0.6, 0.8),
            Vec3::new(-1.0, -1.0, 1.4),
        ] {
            let frame = PortalFrame::from_portal(&portal(PortalEnd::A, Vec3::ZERO, normal, 1.2));
            assert_frame_valid(&frame);
        }
    }

    #[test]
    fn ramp_frame_up_points_along_the_slope() {
        let frame = PortalFrame::from_portal(&portal(PortalEnd::A, Vec3::ZERO, Vec3::new(0.0, 0.6, 0.8), 0.0));
        assert!((frame.up - Vec3::new(0.0, 0.8, -0.6)).length() < 1e-5);
        assert!((frame.right - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn traversal_preserves_speed() {
        let set = pair(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.6, 0.8),
            Vec3::new(9.0, 3.0, 1.0),
            Vec3::X,
        );
        let (entry, exit) = frames(&set);
        let v = Vec3::new(1.3, -4.2, 2.9);
        assert!((traverse_vector(entry, exit, v).length() - v.length()).abs() < 1e-4);
    }

    #[test]
    fn facing_wall_pair_acts_as_a_tunnel() {
        let set = pair(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Z,
            Vec3::new(0.0, 1.0, 10.0),
            Vec3::NEG_Z,
        );
        let (entry, exit) = frames(&set);
        let v = Vec3::new(0.0, 0.0, -6.0);
        assert!((traverse_vector(entry, exit, v) - v).length() < 1e-5);
        assert!(angle_delta_radians(traverse_yaw(entry, exit, PI), PI).abs() < 1e-5);
    }

    #[test]
    fn same_wall_pair_reverses_heading() {
        let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(5.0, 1.0, 0.0), Vec3::Z);
        let (entry, exit) = frames(&set);
        let out = traverse_vector(entry, exit, Vec3::new(0.0, 0.0, -6.0));
        assert!((out - Vec3::new(0.0, 0.0, 6.0)).length() < 1e-5);
        assert!(angle_delta_radians(traverse_yaw(entry, exit, PI), 0.0).abs() < 1e-5);
    }

    #[test]
    fn travelers_rightward_drift_stays_rightward() {
        // Facing -Z the traveler's right is +X; through a facing pair the
        // exit heading stays -Z, so their right must stay +X.
        let set = pair(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Z,
            Vec3::new(0.0, 1.0, 10.0),
            Vec3::NEG_Z,
        );
        let (entry, exit) = frames(&set);
        let out = traverse_vector(entry, exit, Vec3::new(0.5, 0.0, -6.0));
        assert!((out - Vec3::new(0.5, 0.0, -6.0)).length() < 1e-5);
        // Through a same-wall pair the exit heading is +Z: traveler's right
        // becomes world -X, and the drift must follow it.
        let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(5.0, 1.0, 0.0), Vec3::Z);
        let (entry, exit) = frames(&set);
        let out = traverse_vector(entry, exit, Vec3::new(0.5, 0.0, -6.0));
        assert!((out - Vec3::new(-0.5, 0.0, 6.0)).length() < 1e-5);
    }

    #[test]
    fn wall_to_wall_yaw_matches_closed_form() {
        for (entry_normal, exit_normal) in [(Vec3::Z, Vec3::X), (Vec3::NEG_X, Vec3::Z), (Vec3::X, Vec3::NEG_Z)] {
            let set = pair(
                Vec3::new(0.0, 1.0, 0.0),
                entry_normal,
                Vec3::new(7.0, 1.0, 3.0),
                exit_normal,
            );
            let (entry, exit) = frames(&set);
            let yaw = entry_normal.x.atan2(entry_normal.z) + PI - 0.4; // mostly into the entry
            let expected = yaw + exit_normal.x.atan2(exit_normal.z) - entry_normal.x.atan2(entry_normal.z) + PI;
            assert!(angle_delta_radians(traverse_yaw(entry, exit, yaw), expected).abs() < 1e-4);
        }
    }

    #[test]
    fn square_on_wall_entry_to_floor_exit_faces_the_exit_up() {
        let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(5.0, 0.0, 5.0), Vec3::Y);
        let (entry, exit) = frames(&set);
        // Walking dead-on into the wall maps the facing vertical; the fallback
        // is the floor exit's in-plane up (its placement yaw, here 0 = +Z).
        assert!(angle_delta_radians(traverse_yaw(entry, exit, PI), 0.0).abs() < 1e-4);
    }

    #[test]
    fn falling_into_floor_portal_carries_out_of_wall_as_knockback() {
        let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 2.0, 0.0), Vec3::X);
        let hop = set
            .character_hop(
                Vec3::new(0.0, -0.85, 0.0),
                Vec3::new(0.0, -0.95, 0.0),
                player_physics(),
                Vec3::ZERO,
                Vec3::ZERO,
                -10.0,
                0.0,
                CAP,
            )
            .expect("fall through a floor portal did not trigger");
        assert!(hop.vertical_velocity.abs() < 1e-4);
        assert!((hop.knockback - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn walking_into_wall_portal_exits_floor_portal_upward() {
        let set = pair(Vec3::new(0.0, 0.9, 0.0), Vec3::Z, Vec3::new(10.0, 0.0, 10.0), Vec3::Y);
        let hop = set
            .character_hop(
                Vec3::new(0.0, 0.0, 0.1),
                Vec3::new(0.0, 0.0, -0.1),
                player_physics(),
                Vec3::new(0.0, 0.0, -6.0),
                Vec3::ZERO,
                0.0,
                PI,
                CAP,
            )
            .expect("walk through a wall portal did not trigger");
        // Control maps into the vertical write but not the carry.
        assert!((hop.vertical_velocity - 6.0).abs() < 1e-4);
        assert!(hop.knockback.length() < 1e-4);
        // Emerges half-in: the crossing penetration is carried through.
        assert!((hop.origin.y - (0.1 - 0.9)).abs() < 1e-4);
    }

    #[test]
    fn crossing_the_plane_triggers_and_carries_penetration() {
        let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set
            .character_hop(
                Vec3::new(0.0, 0.7, 0.15),
                Vec3::new(0.0, 0.7, -0.05),
                player_physics(),
                Vec3::new(0.0, 0.0, -6.0),
                Vec3::ZERO,
                0.0,
                PI,
                CAP,
            )
            .expect("crossing did not trigger");
        // The exit continues in front of the paired plane by the same
        // penetration the entry reached — seamless pass-through.
        assert!((hop.origin.x - 10.05).abs() < 1e-4);
    }

    #[test]
    fn approaching_without_crossing_does_not_trigger() {
        let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.character_hop(
            Vec3::new(0.0, 0.7, 0.5),
            Vec3::new(0.0, 0.7, 0.1),
            player_physics(),
            Vec3::new(0.0, 0.0, -6.0),
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        );
        assert!(hop.is_none());
    }

    #[test]
    fn crossing_from_behind_does_not_trigger() {
        let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.character_hop(
            Vec3::new(0.0, 0.7, -0.2),
            Vec3::new(0.0, 0.7, 0.2),
            player_physics(),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::ZERO,
            0.0,
            0.0,
            CAP,
        );
        assert!(hop.is_none());
    }

    #[test]
    fn crossing_outside_the_aperture_does_not_trigger() {
        let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.character_hop(
            Vec3::new(2.0, 0.7, 0.15),
            Vec3::new(2.0, 0.7, -0.05),
            player_physics(),
            Vec3::new(0.0, 0.0, -6.0),
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        );
        assert!(hop.is_none());
    }

    #[test]
    fn off_center_crossing_uses_the_full_rectangle() {
        // Body center 0.7 below and 0.65 beside the portal center: the oval
        // would reject this; the rectangular character gate does not.
        let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.character_hop(
            Vec3::new(0.65, 0.0, 0.15),
            Vec3::new(0.65, 0.0, -0.05),
            player_physics(),
            Vec3::new(0.0, 0.0, -6.0),
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        );
        assert!(hop.is_some());
    }

    #[test]
    fn knockback_carry_is_capped() {
        let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 2.0, 0.0), Vec3::X);
        let hop = set
            .character_hop(
                Vec3::new(0.0, -0.85, 0.0),
                Vec3::new(0.0, -0.95, 0.0),
                player_physics(),
                Vec3::ZERO,
                Vec3::ZERO,
                -50.0,
                0.0,
                CAP,
            )
            .expect("fall through a floor portal did not trigger");
        assert!((hop.knockback.length() - CAP).abs() < 1e-4);
    }

    #[test]
    fn projectile_hop_requires_front_side_approach() {
        let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let toward = set.projectile_hop(Vec3::new(0.0, 1.0, 2.0), Vec3::new(0.0, 0.0, -30.0), 0.1, 0.08);
        assert!(toward.is_some());
        let from_behind = set.projectile_hop(Vec3::new(0.0, 1.0, -2.0), Vec3::new(0.0, 0.0, 30.0), 0.1, 0.08);
        assert!(from_behind.is_none());
    }

    #[test]
    fn projectile_hop_rejects_shots_outside_the_aperture() {
        let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.projectile_hop(
            Vec3::new(2.0 * PORTAL_HALF_WIDTH, 1.0, 2.0),
            Vec3::new(0.0, 0.0, -30.0),
            0.1,
            0.08,
        );
        assert!(hop.is_none());
    }

    #[test]
    fn projectile_continues_straight_through_a_facing_pair() {
        let set = pair(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Z,
            Vec3::new(0.0, 1.0, 10.0),
            Vec3::NEG_Z,
        );
        let velocity = Vec3::new(1.0, 0.0, -30.0);
        let hop = set
            .projectile_hop(Vec3::new(0.2, 1.1, 2.0), velocity, 0.1, 0.08)
            .expect("projectile aimed at the portal did not hop");
        // A facing pair is a tunnel: the lateral offset and velocity carry over.
        assert!((hop.exit_velocity - velocity).length() < 1e-4);
        assert!((hop.exit_pos.x - hop.entry_point.x).abs() < 1e-4);
        assert!((hop.exit_pos.y - hop.entry_point.y).abs() < 1e-4);
        assert!((hop.exit_velocity.length() - velocity.length()).abs() < 1e-4);
    }

    #[test]
    fn traversal_frames_recovers_the_used_gate_in_both_directions() {
        let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let (entry, exit) = set
            .traversal_frames(Vec3::new(0.1, 1.2, 0.6), Vec3::new(10.6, 1.1, 10.0))
            .expect("no gate recovered");
        assert!((entry.normal - Vec3::Z).length() < 1e-5);
        assert!((exit.normal - Vec3::X).length() < 1e-5);
        let (entry, exit) = set
            .traversal_frames(Vec3::new(10.6, 1.1, 10.0), Vec3::new(0.1, 1.2, 0.6))
            .expect("no reverse gate recovered");
        assert!((entry.normal - Vec3::X).length() < 1e-5);
        assert!((exit.normal - Vec3::Z).length() < 1e-5);
    }

    #[test]
    fn half_placed_pair_is_inert() {
        let set = PortalSet::rebuild(
            &[portal(PortalEnd::A, Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0)],
            &empty_world(),
        );
        assert!(set.is_empty());
        let hop = set.projectile_hop(Vec3::new(0.0, 1.0, 2.0), Vec3::new(0.0, 0.0, -30.0), 0.1, 0.08);
        assert!(hop.is_none());
    }

    use crate::constants::{FLOOR_THICKNESS, WALL_THICKNESS};
    use crate::protocol::{BarrierKindTable, Floor, PlatePurpose, Position, PressurePlate, Wall, WallLight};

    // One 12 m wall along X at z = 0 (level 0) with the room floor on +Z.
    fn placement_layout() -> MapLayout {
        MapLayout {
            walls: vec![Wall {
                x1: -6.0,
                z1: 0.0,
                x2: 6.0,
                z2: 0.0,
                width: WALL_THICKNESS,
                level: 0,
            }],
            floors: vec![Floor {
                x1: -6.0,
                z1: 0.0,
                x2: 6.0,
                z2: 6.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
            }],
            ..Default::default()
        }
    }

    fn place(layout: &MapLayout, origin: Vec3, toward: Vec3, yaw: f32) -> Option<PortalPlacement> {
        let world = CollisionWorld::from_map_layout(layout, &BarrierKindTable::default());
        compute_portal_placement(origin, (toward - origin).normalize(), yaw, 40.0, &world, layout)
    }

    #[test]
    fn placement_accepts_a_clear_wall_center() {
        let layout = placement_layout();
        let placement =
            place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI).expect("clear wall center rejected");
        assert!((placement.normal - Vec3::Z).length() < 1e-4);
    }

    #[test]
    fn low_wall_shot_nudges_up_until_the_aperture_fits() {
        let layout = placement_layout();
        let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 0.5, 0.0), PI)
            .expect("low shot did not nudge up onto the wall");
        assert!(placement.pos.y > 1.3);
        assert!(placement.pos.x.abs() < 0.3);
    }

    #[test]
    fn shot_past_the_walls_end_nudges_back_onto_it() {
        let layout = placement_layout();
        let placement = place(&layout, Vec3::new(5.9, 1.6, 3.0), Vec3::new(5.9, 1.6, 0.0), PI)
            .expect("edge shot did not nudge back onto the wall");
        assert!(placement.pos.x < 5.45);
    }

    #[test]
    fn floor_shot_under_a_crossing_wall_nudges_clear_of_it() {
        let mut layout = placement_layout();
        // A second wall crossing the floor at z = 3 cuts any aperture that
        // straddles it — including between rim probes.
        layout.walls.push(Wall {
            x1: -6.0,
            z1: 3.0,
            x2: 6.0,
            z2: 3.0,
            width: WALL_THICKNESS,
            level: 0,
        });
        let placement = place(&layout, Vec3::new(2.0, 1.6, 2.5), Vec3::new(2.0, 0.0, 2.5), 0.0)
            .expect("wall-cut floor shot did not nudge clear");
        assert!(placement.pos.z < 1.56);
        assert!((placement.pos.x - 2.0).abs() < 0.01);
    }

    #[test]
    fn shot_with_no_fitting_spot_anywhere_fizzles() {
        let mut layout = placement_layout();
        // A 1 m stub wall can never back the 1.4 m aperture, nudged or not.
        layout.walls[0].x1 = -0.5;
        layout.walls[0].x2 = 0.5;
        let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI);
        assert!(placement.is_none());
    }

    #[test]
    fn shot_at_a_wall_light_nudges_clear_of_it() {
        let mut layout = placement_layout();
        layout.wall_lights.push(WallLight {
            pos: Position { x: 0.0, y: 1.6, z: 0.2 },
            yaw: 0.0,
        });
        let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI)
            .expect("shot at the light did not nudge clear");
        // The nudged aperture leaves the light outside its grown keep-out oval.
        let across = placement.pos.x / (PORTAL_HALF_WIDTH + PORTAL_LIGHT_CLEARANCE);
        let along_up = (placement.pos.y - 1.6) / (PORTAL_HALF_HEIGHT + PORTAL_LIGHT_CLEARANCE);
        assert!(across * across + along_up * along_up > 0.99);
        // Far enough along the wall the light does not even nudge the shot.
        let clear =
            place(&layout, Vec3::new(4.0, 1.6, 3.0), Vec3::new(4.0, 1.6, 0.0), PI).expect("clear shot rejected");
        assert!((clear.pos.x - 4.0).abs() < 0.01);
    }

    #[test]
    fn placement_rejects_a_floor_portal_covering_a_pressure_plate() {
        let mut layout = placement_layout();
        layout.pressure_plates.push(PressurePlate {
            level: 0,
            center_x: 3.0,
            center_z: 3.0,
            purpose: PlatePurpose::Firework,
        });
        assert!(place(&layout, Vec3::new(3.0, 1.6, 3.0), Vec3::new(3.0, 0.0, 3.0), 0.0).is_none());
        // The same shot well away from the plate lands.
        assert!(place(&layout, Vec3::new(-3.0, 1.6, 3.0), Vec3::new(-3.0, 0.0, 3.0), 0.0).is_some());
    }

    // Mirrors one server tick: movement step, then the traversal check with
    // the landing-recovered fall speed, then the fall tracker's bookkeeping.
    // Mirrors one server tick: the movement step (portal backing excluded,
    // so the body sinks straight through), then the crossing check between
    // the previous and current positions.
    #[test]
    fn perpetual_floor_fall_keeps_its_speed_across_hops() {
        use crate::constants::TICK_SECS;
        use crate::physics::{CharacterEnvironment, CharacterStep, step_character_movement};

        let gameplay = GameplayConfig::load_default().expect("default gameplay config should load");
        let physics = gameplay.player.physics();
        let layout = MapLayout {
            floors: vec![
                Floor {
                    x1: -10.0,
                    z1: -10.0,
                    x2: 10.0,
                    z2: 10.0,
                    y: 0.0,
                    thickness: FLOOR_THICKNESS,
                    level: 0,
                },
                Floor {
                    x1: 40.0,
                    z1: 40.0,
                    x2: 60.0,
                    z2: 60.0,
                    y: 0.0,
                    thickness: FLOOR_THICKNESS,
                    level: 0,
                },
            ],
            ..Default::default()
        };
        let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
        let set = PortalSet::rebuild(
            &[
                portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
                portal(PortalEnd::B, Vec3::new(50.0, 0.0, 50.0), Vec3::Y, 0.0),
            ],
            &world,
        );
        let env = CharacterEnvironment {
            collision_world: &world,
            gravity: 25.0,
            passable_kinds: &[],
            physics,
            ladder_climb_ratio: gameplay.movement.ladder_climb_ratio,
            portals: Some(&set),
        };

        let mut pos = crate::protocol::Position { x: 0.0, y: 8.0, z: 0.0 };
        let mut vertical_velocity = 0.0_f32;
        let mut entry_speeds: Vec<f32> = Vec::new();

        for _ in 0..(30 * 8) {
            let from = pos;
            let result = step_character_movement(
                CharacterStep {
                    start: pos,
                    vertical_velocity,
                    control_velocity: Vec3::ZERO,
                    external_displacement: Vec3::ZERO,
                    delta: TICK_SECS,
                },
                &env,
            );
            pos = result.position;
            vertical_velocity = result.vertical_velocity;
            if let Some(hop) = set.character_hop(
                Vec3::from(from),
                Vec3::from(pos),
                physics,
                Vec3::ZERO,
                Vec3::ZERO,
                vertical_velocity,
                0.0,
                22.5,
            ) {
                entry_speeds.push(-vertical_velocity);
                pos = hop.origin.into();
                vertical_velocity = hop.vertical_velocity;
            }
        }

        assert!(
            entry_speeds.len() >= 3,
            "only {} hops in 8 s: {entry_speeds:?}",
            entry_speeds.len()
        );
        let first = entry_speeds[0];
        let last = *entry_speeds.last().expect("no hops recorded");
        let expected = (2.0 * env.gravity * 8.0)
            .sqrt()
            .min(crate::constants::CHARACTER_TERMINAL_VELOCITY);
        assert!(first > expected - 3.0, "first entry too slow: {entry_speeds:?}");
        assert!(last > first - 3.0, "speed decayed across hops: {entry_speeds:?}");
    }

    // The fast cycle: floor portal with its pair on the ceiling directly
    // above. Every pass adds a room's worth of gravity and the flight time
    // shrinks well below the teleport cooldown, so this test fails if
    // slow-entry pacing ever throttles a real fall chain.
    // The fast cycle: floor portal with its pair on the ceiling directly
    // above. Every pass adds a room of gravity; speed must build to the
    // terminal cap and stay there.
    #[test]
    fn floor_to_ceiling_fall_accelerates_toward_terminal_velocity() {
        use crate::constants::TICK_SECS;
        use crate::physics::{CharacterEnvironment, CharacterStep, step_character_movement};

        let gameplay = GameplayConfig::load_default().expect("default gameplay config should load");
        let physics = gameplay.player.physics();
        let layout = MapLayout {
            floors: vec![Floor {
                x1: -10.0,
                z1: -10.0,
                x2: 10.0,
                z2: 10.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
            }],
            ..Default::default()
        };
        let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
        let set = PortalSet::rebuild(
            &[
                portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
                portal(PortalEnd::B, Vec3::new(0.0, 4.0, 0.0), Vec3::NEG_Y, 0.0),
            ],
            &world,
        );
        let env = CharacterEnvironment {
            collision_world: &world,
            gravity: 25.0,
            passable_kinds: &[],
            physics,
            ladder_climb_ratio: gameplay.movement.ladder_climb_ratio,
            portals: Some(&set),
        };

        let mut pos = crate::protocol::Position { x: 0.0, y: 3.0, z: 0.0 };
        let mut vertical_velocity = 0.0_f32;
        let mut entry_speeds: Vec<f32> = Vec::new();

        for _ in 0..(30 * 8) {
            let from = pos;
            let result = step_character_movement(
                CharacterStep {
                    start: pos,
                    vertical_velocity,
                    control_velocity: Vec3::ZERO,
                    external_displacement: Vec3::ZERO,
                    delta: TICK_SECS,
                },
                &env,
            );
            pos = result.position;
            vertical_velocity = result.vertical_velocity;
            if let Some(hop) = set.character_hop(
                Vec3::from(from),
                Vec3::from(pos),
                physics,
                Vec3::ZERO,
                Vec3::ZERO,
                vertical_velocity,
                0.0,
                22.5,
            ) {
                entry_speeds.push(-vertical_velocity);
                pos = hop.origin.into();
                vertical_velocity = hop.vertical_velocity;
            }
        }

        assert!(
            entry_speeds.len() >= 8,
            "only {} hops in 8 s: {entry_speeds:?}",
            entry_speeds.len()
        );
        let last = *entry_speeds.last().expect("no hops recorded");
        assert!(
            last > crate::constants::CHARACTER_TERMINAL_VELOCITY - 2.0,
            "fall chain never reached terminal velocity: {entry_speeds:?}"
        );
        for window in entry_speeds.windows(2) {
            assert!(
                window[1] > window[0] - 0.5,
                "speed regressed mid-chain: {entry_speeds:?}"
            );
        }
    }

    #[test]
    fn aperture_offset_carries_through_an_opposing_pair() {
        // Floor -> ceiling: the mapped offset preserves world drift, so a
        // steering player accumulates displacement across hops.
        let set = pair(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::Y,
            Vec3::new(10.0, 4.0, 10.0),
            Vec3::NEG_Y,
        );
        let hop = set
            .character_hop(
                Vec3::new(0.0, -0.85, 0.5),
                Vec3::new(0.0, -0.95, 0.5),
                player_physics(),
                Vec3::ZERO,
                Vec3::ZERO,
                -5.0,
                0.0,
                CAP,
            )
            .expect("offset crossing did not trigger");
        assert!((hop.origin.x - 10.0).abs() < 1e-4);
        assert!((hop.origin.z - 10.5).abs() < 1e-4);
    }

    #[test]
    fn carried_offset_is_clamped_to_the_exit_aperture() {
        let set = pair(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::Y,
            Vec3::new(10.0, 4.0, 10.0),
            Vec3::NEG_Y,
        );
        let physics = player_physics();
        let hop = set
            .character_hop(
                Vec3::new(0.55, -0.85, 0.0),
                Vec3::new(0.55, -0.95, 0.0),
                physics,
                Vec3::ZERO,
                Vec3::ZERO,
                -5.0,
                0.0,
                CAP,
            )
            .expect("edge crossing did not trigger");
        let limit = PORTAL_HALF_WIDTH - physics.collider.width / 2.0;
        assert!((hop.origin.x - 10.0).abs() <= limit + 1e-4);
        assert!(hop.origin.x > 10.0);
    }

    // Holding a direction while looping must break the loop within a few
    // hops — the whole point of carrying the aperture offset.
    // Holding a direction while looping must break the loop within a few
    // hops: the crossing gate is aperture-bound, so accumulated drift makes
    // the body miss the hole and land beside it.
    #[test]
    fn steering_sideways_escapes_a_portal_fall_chain() {
        use crate::constants::TICK_SECS;
        use crate::physics::{CharacterEnvironment, CharacterStep, step_character_movement};

        let gameplay = GameplayConfig::load_default().expect("default gameplay config should load");
        let physics = gameplay.player.physics();
        let layout = MapLayout {
            floors: vec![Floor {
                x1: -10.0,
                z1: -10.0,
                x2: 10.0,
                z2: 10.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
            }],
            ..Default::default()
        };
        let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
        let set = PortalSet::rebuild(
            &[
                portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
                portal(PortalEnd::B, Vec3::new(0.0, 4.0, 0.0), Vec3::NEG_Y, 0.0),
            ],
            &world,
        );
        let env = CharacterEnvironment {
            collision_world: &world,
            gravity: 25.0,
            passable_kinds: &[],
            physics,
            ladder_climb_ratio: gameplay.movement.ladder_climb_ratio,
            portals: Some(&set),
        };

        let mut pos = crate::protocol::Position { x: 0.0, y: 3.0, z: 0.0 };
        let mut vertical_velocity = 0.0_f32;
        let mut hops = 0;

        for _ in 0..(30 * 8) {
            // Fall in hands-off, then steer once the chain is running.
            let control = if hops >= 1 {
                Vec3::new(0.0, 0.0, 6.0)
            } else {
                Vec3::ZERO
            };
            let from = pos;
            let result = step_character_movement(
                CharacterStep {
                    start: pos,
                    vertical_velocity,
                    control_velocity: control,
                    external_displacement: Vec3::ZERO,
                    delta: TICK_SECS,
                },
                &env,
            );
            pos = result.position;
            vertical_velocity = result.vertical_velocity;
            if let Some(hop) = set.character_hop(
                Vec3::from(from),
                Vec3::from(pos),
                physics,
                control,
                Vec3::ZERO,
                vertical_velocity,
                0.0,
                22.5,
            ) {
                pos = hop.origin.into();
                vertical_velocity = hop.vertical_velocity;
                hops += 1;
            }
        }

        assert!(hops >= 1, "the chain never started");
        assert!(hops <= 10, "steering never escaped the chain: {hops} hops");
        assert!(pos.z > 2.0, "escaped body did not keep moving: z = {}", pos.z);
    }

    #[test]
    fn an_external_teleport_is_not_a_crossing() {
        // Sign-crosses the plane, but no tick of real motion jumps this far.
        let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.character_hop(
            Vec3::new(0.0, 50.0, 0.15),
            Vec3::new(0.0, 0.7, -0.05),
            player_physics(),
            Vec3::ZERO,
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        );
        assert!(hop.is_none());
    }
}
