use bevy_ecs::prelude::*;
use bevy_math::{Mat3, Quat, Vec3};
use rapier3d::prelude::ColliderHandle;

use super::PortalFrame;
use crate::{
    config::CharacterPhysicsConfig,
    constants::{
        PORTAL_FUNNEL_CAPTURE_MARGIN, PORTAL_FUNNEL_GAIN, PORTAL_FUNNEL_MAX_SPEED, PORTAL_FUNNEL_MIN_APPROACH,
        PORTAL_FUNNEL_RELEASE_SPEED, PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_PROJECTILE_EXIT_STANDOFF,
        PORTAL_STANDABLE_NORMAL_Y, PORTAL_UP_DEGENERACY_LIMIT,
    },
    math::direction_from_yaw_pitch,
    physics::CollisionWorld,
    protocol::{PlayerMoveIntent, Portal, PortalEnd},
};

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

#[must_use]
pub fn traverse_move_intent(entry: &PortalFrame, exit: &PortalFrame, intent: PlayerMoveIntent) -> PlayerMoveIntent {
    // The server keeps using the last intent until another CMove arrives; it
    // must point out of the exit immediately instead of re-entering it.
    match intent {
        PlayerMoveIntent::Idle => PlayerMoveIntent::Idle,
        PlayerMoveIntent::Walking { direction } => PlayerMoveIntent::Walking {
            direction: traverse_yaw(entry, exit, direction),
        },
        PlayerMoveIntent::Running { direction } => PlayerMoveIntent::Running {
            direction: traverse_yaw(entry, exit, direction),
        },
    }
}

// Facing through the pair, projected back to a yaw. Square-on entries can map
// the facing vertical; the fallbacks pick the stable direction that remains —
// out of a tilted exit, or along a vertical exit's in-plane up.
#[must_use]
pub(super) fn traverse_yaw(entry: &PortalFrame, exit: &PortalFrame, yaw: f32) -> f32 {
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
    #[cfg(test)]
    pub(super) fn first_pair_frames(&self) -> Option<(&PortalFrame, &PortalFrame)> {
        self.pairs.first().map(|pair| (&pair.a.frame, &pair.b.frame))
    }

    // Pairs each owner's A and B ends; a half-placed pair is inert. The
    // collision world supplies each aperture's backing colliders — both
    // sides derive them from the same static world, so the sets agree.
    #[must_use]
    pub fn rebuild(portals: &[Portal], collision_world: &CollisionWorld) -> Self {
        let mut portals = portals.to_vec();
        portals.sort_by_key(|portal| (portal.owner.0, portal.end == PortalEnd::B));
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

    #[must_use]
    pub fn movement_collision_exclusions(
        &self,
        origin: Vec3,
        translation: Vec3,
        physics: CharacterPhysicsConfig,
    ) -> Vec<ColliderHandle> {
        if self.pairs.is_empty() {
            return Vec::new();
        }
        let half_extents = body_half_extents(physics);
        let center = origin + Vec3::Y * half_extents.y;
        let target = center + translation;
        let mut excluded = Vec::new();
        for (gate, _) in self.gates() {
            let start_offset = center - gate.frame.center;
            let target_offset = target - gate.frame.center;
            let start_distance = start_offset.dot(gate.frame.normal);
            let target_distance = target_offset.dot(gate.frame.normal);
            let behind_reach = body_support(half_extents, gate.frame.normal) + TRANSIT_MARGIN;
            if start_distance <= -behind_reach && target_distance <= -behind_reach {
                continue;
            }
            let aperture_offset = if (start_distance > 0.0 && target_distance <= 0.0)
                || (start_distance <= 0.0 && target_distance > 0.0)
            {
                let t = start_distance / (start_distance - target_distance);
                start_offset.lerp(target_offset, t)
            } else if target_distance.abs() < start_distance.abs() {
                target_offset
            } else {
                start_offset
            };
            if in_character_aperture(aperture_offset, &gate.frame) {
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
            let crossing_t = from_distance / (from_distance - to_distance);
            let crossing_offset = (center_from - entry.center).lerp(center_to - entry.center, crossing_t);
            if !in_character_aperture(crossing_offset, entry) {
                continue;
            }
            let offset = center_to - entry.center;
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

    // Portal-2-style funneling: the horizontal pull toward the axis of a
    // vertical-normal aperture the body is flying toward. The pull grows
    // with the lateral offset, captures a little beyond the aperture rect
    // (so a near-miss is gathered in), and disengages the moment the
    // player steers — escaping a fall chain stays deliberate.
    #[must_use]
    pub fn funnel_displacement(
        &self,
        origin: Vec3,
        physics: CharacterPhysicsConfig,
        control_velocity: Vec3,
        vertical_velocity: f32,
        delta: f32,
    ) -> Vec3 {
        if self.pairs.is_empty() {
            return Vec3::ZERO;
        }
        let steering = Vec3::new(control_velocity.x, 0.0, control_velocity.z).length() > PORTAL_FUNNEL_RELEASE_SPEED;
        if steering {
            return Vec3::ZERO;
        }
        let half_extents = body_half_extents(physics);
        let center = origin + Vec3::Y * half_extents.y;
        for (gate, _) in self.gates() {
            let normal = gate.frame.normal;
            if normal.y.abs() <= PORTAL_STANDABLE_NORMAL_Y {
                continue;
            }
            let approaching = if normal.y > 0.0 {
                vertical_velocity < -PORTAL_FUNNEL_MIN_APPROACH
            } else {
                vertical_velocity > PORTAL_FUNNEL_MIN_APPROACH
            };
            if !approaching {
                continue;
            }
            let offset = center - gate.frame.center;
            if offset.dot(normal) <= 0.0 {
                continue;
            }
            let captured = offset.dot(gate.frame.right).abs() <= PORTAL_HALF_WIDTH + PORTAL_FUNNEL_CAPTURE_MARGIN
                && offset.dot(gate.frame.up).abs() <= PORTAL_HALF_HEIGHT + PORTAL_FUNNEL_CAPTURE_MARGIN;
            if !captured {
                continue;
            }
            let lateral = Vec3::new(offset.x, 0.0, offset.z);
            let Some(direction) = lateral.try_normalize() else {
                continue;
            };
            let speed = (lateral.length() * PORTAL_FUNNEL_GAIN).min(PORTAL_FUNNEL_MAX_SPEED);
            return -direction * speed * delta;
        }
        Vec3::ZERO
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
