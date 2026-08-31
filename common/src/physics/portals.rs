use bevy_ecs::prelude::*;
use bevy_math::Vec3;

use crate::{
    config::CharacterPhysicsConfig,
    constants::{
        PORTAL_EXIT_CLEARANCE, PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_MIN_APPROACH_SPEED,
        PORTAL_PROJECTILE_EXIT_STANDOFF, PORTAL_STANDABLE_AWAY_SPEED, PORTAL_STANDABLE_NORMAL_Y, PORTAL_TRIGGER_DEPTH,
        PORTAL_UP_DEGENERACY_LIMIT,
    },
    math::direction_from_yaw_pitch,
    protocol::{Portal, PortalEnd},
};

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
        let normal = Vec3::new(portal.nx, portal.ny, portal.nz).normalize();
        // World-up projected onto the plane orients the frame; only a
        // near-vertical normal is degenerate, and there the shooter's
        // placement yaw supplies the in-plane up instead.
        let reference = if normal.y.abs() < PORTAL_UP_DEGENERACY_LIMIT {
            Vec3::Y
        } else {
            direction_from_yaw_pitch(portal.yaw, 0.0)
        };
        let up = (reference - normal * reference.dot(normal)).normalize();
        Self {
            center: portal.pos.into(),
            normal,
            up,
            right: up.cross(normal),
        }
    }
}

// Map a vector through a pair: decompose in the entry frame, re-emit in the
// exit frame with right and normal negated — a 180° turn about `up` in frame
// space, so a proper rotation (never a mirror) that preserves length.
#[must_use]
fn traverse_vector(entry: &PortalFrame, exit: &PortalFrame, v: Vec3) -> Vec3 {
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

// Outcome of a character stepping through a portal this tick.
#[derive(Debug, Clone, Copy)]
pub struct CharacterPortalHop {
    // New entity origin (feet), body centered on the exit aperture and clear
    // of its plane.
    pub origin: Vec3,
    pub yaw: f32,
    pub vertical_velocity: f32,
    // Horizontal momentum carry, expressed as knockback — the only persistent
    // horizontal velocity in the character model. Control velocity is
    // excluded: the held keys re-supply it in the exit frame next tick, and
    // carrying it too would double it.
    pub knockback: Vec3,
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

#[derive(Debug, Clone, Copy)]
struct PortalPairFrames {
    a: PortalFrame,
    b: PortalFrame,
}

// Every complete portal pair in the world. Portals are shot-placed at
// runtime, so they cannot live in the immutable `CollisionWorld`; both sides
// hold this resource and rebuild it from the replicated portal list, through
// this one constructor, so their frames agree exactly.
#[derive(Resource, Debug, Default, Clone)]
pub struct PortalSet {
    pairs: Vec<PortalPairFrames>,
}

impl PortalSet {
    // Pairs each owner's A and B ends; a half-placed pair is inert.
    #[must_use]
    pub fn rebuild(portals: &[Portal]) -> Self {
        let mut pairs = Vec::new();
        for a in portals.iter().filter(|portal| portal.end == PortalEnd::A) {
            let Some(b) = portals
                .iter()
                .find(|portal| portal.owner == a.owner && portal.end == PortalEnd::B)
            else {
                continue;
            };
            pairs.push(PortalPairFrames {
                a: PortalFrame::from_portal(a),
                b: PortalFrame::from_portal(b),
            });
        }
        Self { pairs }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    fn gates(&self) -> impl Iterator<Item = (&PortalFrame, &PortalFrame)> {
        self.pairs
            .iter()
            .flat_map(|pair| [(&pair.a, &pair.b), (&pair.b, &pair.a)])
    }

    // Character trigger + traversal, checked after the movement step. It
    // fires BEFORE any plane penetration — collision never lets a body reach
    // the surface — so the crossing test is proximity: the body face nearest
    // the plane within `PORTAL_TRIGGER_DEPTH`, body center inside the
    // aperture, moving inward. Standable portals (normal pointing up enough
    // to rest on) also trigger at rest: gravity is their inward motion.
    #[must_use]
    pub fn character_hop(
        &self,
        origin: Vec3,
        physics: CharacterPhysicsConfig,
        control_velocity: Vec3,
        knockback: Vec3,
        vertical_velocity: f32,
        yaw: f32,
        knockback_cap: f32,
    ) -> Option<CharacterPortalHop> {
        let half_extents = body_half_extents(physics);
        let center = origin + Vec3::Y * half_extents.y;
        let velocity = control_velocity + knockback + Vec3::Y * vertical_velocity;
        for (entry, exit) in self.gates() {
            let offset = center - entry.center;
            let center_distance = offset.dot(entry.normal);
            if center_distance <= 0.0 {
                continue;
            }
            if center_distance - body_support(half_extents, entry.normal) > PORTAL_TRIGGER_DEPTH {
                continue;
            }
            if !in_aperture(offset, entry) {
                continue;
            }
            let inward_speed = -velocity.dot(entry.normal);
            let required = if entry.normal.y > PORTAL_STANDABLE_NORMAL_Y {
                -PORTAL_STANDABLE_AWAY_SPEED
            } else {
                PORTAL_MIN_APPROACH_SPEED
            };
            if inward_speed <= required {
                continue;
            }
            let exit_center =
                exit.center + exit.normal * (body_support(half_extents, exit.normal) + PORTAL_EXIT_CLEARANCE);
            let carry = traverse_vector(entry, exit, knockback + Vec3::Y * vertical_velocity);
            return Some(CharacterPortalHop {
                origin: exit_center - Vec3::Y * half_extents.y,
                yaw: traverse_yaw(entry, exit, yaw),
                vertical_velocity: traverse_vector(entry, exit, velocity).y,
                knockback: Vec3::new(carry.x, 0.0, carry.z).clamp_length_max(knockback_cap),
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
        for (entry, exit) in self.gates() {
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

    fn pair(a_pos: Vec3, a_normal: Vec3, b_pos: Vec3, b_normal: Vec3) -> PortalSet {
        PortalSet::rebuild(&[
            portal(PortalEnd::A, a_pos, a_normal, 0.0),
            portal(PortalEnd::B, b_pos, b_normal, 0.0),
        ])
    }

    fn frames(set: &PortalSet) -> (&PortalFrame, &PortalFrame) {
        let pair = set.pairs.first().expect("portal set has no pair");
        (&pair.a, &pair.b)
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
            .character_hop(Vec3::ZERO, player_physics(), Vec3::ZERO, Vec3::ZERO, -10.0, 0.0, CAP)
            .expect("fall into a floor portal did not trigger");
        assert!(hop.vertical_velocity.abs() < 1e-4);
        assert!((hop.knockback - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn walking_into_wall_portal_exits_floor_portal_upward() {
        let physics = player_physics();
        let depth = physics.collider.depth / 2.0;
        let set = pair(Vec3::new(0.0, 0.9, 0.0), Vec3::Z, Vec3::new(10.0, 0.0, 10.0), Vec3::Y);
        let hop = set
            .character_hop(
                Vec3::new(0.0, 0.0, depth + 0.05),
                physics,
                Vec3::new(0.0, 0.0, -6.0),
                Vec3::ZERO,
                0.0,
                PI,
                CAP,
            )
            .expect("walk into a wall portal did not trigger");
        // Control velocity maps into the vertical write but not the carry.
        assert!((hop.vertical_velocity - 6.0).abs() < 1e-4);
        assert!(hop.knockback.length() < 1e-4);
        // Feet land exactly the exit clearance above the floor plane (y = 0).
        assert!((hop.origin.y - PORTAL_EXIT_CLEARANCE).abs() < 1e-4);
    }

    #[test]
    fn wall_trigger_requires_inward_motion() {
        let physics = player_physics();
        let depth = physics.collider.depth / 2.0;
        let set = pair(Vec3::new(0.0, 0.9, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let origin = Vec3::new(0.0, 0.0, depth + 0.05);
        let moving = set.character_hop(origin, physics, Vec3::new(0.0, 0.0, -1.0), Vec3::ZERO, 0.0, PI, CAP);
        assert!(moving.is_some());
        let resting = set.character_hop(origin, physics, Vec3::ZERO, Vec3::ZERO, 0.0, PI, CAP);
        assert!(resting.is_none());
    }

    #[test]
    fn standing_in_floor_portal_triggers_at_rest() {
        let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.character_hop(Vec3::ZERO, player_physics(), Vec3::ZERO, Vec3::ZERO, 0.0, 0.0, CAP);
        assert!(hop.is_some());
    }

    #[test]
    fn jumping_out_of_floor_portal_does_not_retrigger() {
        let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.character_hop(Vec3::ZERO, player_physics(), Vec3::ZERO, Vec3::ZERO, 12.0, 0.0, CAP);
        assert!(hop.is_none());
    }

    #[test]
    fn ceiling_portal_triggers_only_while_rising() {
        let physics = player_physics();
        let top = physics.collider.top_y_offset();
        let set = pair(
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::NEG_Y,
            Vec3::new(10.0, 1.0, 10.0),
            Vec3::X,
        );
        let origin = Vec3::new(0.0, 4.0 - top - 0.05, 0.0);
        assert!(
            set.character_hop(origin, physics, Vec3::ZERO, Vec3::ZERO, 8.0, 0.0, CAP)
                .is_some()
        );
        assert!(
            set.character_hop(origin, physics, Vec3::ZERO, Vec3::ZERO, -1.0, 0.0, CAP)
                .is_none()
        );
    }

    #[test]
    fn back_side_of_a_portal_never_triggers() {
        let physics = player_physics();
        let set = pair(Vec3::new(0.0, 0.9, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set.character_hop(
            Vec3::new(0.0, 0.0, -0.5),
            physics,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::ZERO,
            0.0,
            0.0,
            CAP,
        );
        assert!(hop.is_none());
    }

    #[test]
    fn body_outside_the_aperture_does_not_trigger() {
        let physics = player_physics();
        let depth = physics.collider.depth / 2.0;
        let set = pair(Vec3::new(0.0, 0.9, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let origin = Vec3::new(2.0 * PORTAL_HALF_WIDTH, 0.0, depth + 0.05);
        let hop = set.character_hop(origin, physics, Vec3::new(0.0, 0.0, -1.0), Vec3::ZERO, 0.0, PI, CAP);
        assert!(hop.is_none());
    }

    #[test]
    fn wall_exit_stands_the_body_clear_of_the_plane() {
        let physics = player_physics();
        let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
        let hop = set
            .character_hop(Vec3::ZERO, physics, Vec3::ZERO, Vec3::ZERO, 0.0, 0.0, CAP)
            .expect("floor portal did not trigger at rest");
        let clearance = hop.origin.x - physics.collider.width / 2.0 - 10.0;
        assert!(clearance > 0.0 && clearance < 0.1);
    }

    #[test]
    fn knockback_carry_is_capped() {
        let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 2.0, 0.0), Vec3::X);
        let hop = set
            .character_hop(Vec3::ZERO, player_physics(), Vec3::ZERO, Vec3::ZERO, -50.0, 0.0, CAP)
            .expect("fall into a floor portal did not trigger");
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
    fn half_placed_pair_is_inert() {
        let set = PortalSet::rebuild(&[portal(PortalEnd::A, Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0)]);
        assert!(set.is_empty());
        let hop = set.projectile_hop(Vec3::new(0.0, 1.0, 2.0), Vec3::new(0.0, 0.0, -30.0), 0.1, 0.08);
        assert!(hop.is_none());
    }
}
