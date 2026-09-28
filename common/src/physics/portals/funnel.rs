use bevy_math::Vec3;

use super::{PortalSet, traversal::body_support};
use crate::{
    config::{CharacterPhysicsConfig, PortalFunnelConfig},
    constants::{CHARACTER_TERMINAL_VELOCITY, PORTAL_STANDABLE_NORMAL_Y},
    math::PHYSICS_EPSILON,
    physics::{CollisionWorld, character_movement_center, character_movement_shape},
    protocol::FieldId,
};

#[derive(Default)]
pub(crate) struct FunnelCorrection {
    pub displacement: Vec3,
    pub velocity_change: Vec3,
}

pub(crate) struct FunnelStep<'a> {
    pub origin: Vec3,
    pub physics: CharacterPhysicsConfig,
    pub velocity: Vec3,
    pub gravity: f32,
    pub delta: f32,
    pub config: PortalFunnelConfig,
    pub world: &'a CollisionWorld,
    pub passable_fields: &'a [FieldId],
}

impl PortalSet {
    // Called once by the airborne player policy, only without movement intent.
    // Capture is the vertical projection onto a floor/ceiling/ramp aperture,
    // expanded by the authored margin. A ballistic prediction supplies the
    // time before the capsule meets the rim. A cubic brings its horizontal
    // offset AND drift to zero by then, without changing vertical momentum.
    // Recompute from actual motion each tick; no future physics ticks or
    // per-player funnel state. Moving gates use their current linear velocity.
    pub(crate) fn funnel_correction(&self, step: FunnelStep<'_>) -> FunnelCorrection {
        if self.is_empty() || step.config.capture_margin <= 0.0 || step.delta <= 0.0 {
            return FunnelCorrection::default();
        }
        let center = character_movement_center(step.origin.into(), step.physics);
        let shape = character_movement_shape(step.physics);
        let mut best = None;
        for (gate, _) in self.gates() {
            let frame = &gate.frame;
            let normal = frame.normal;
            if normal.y.abs() <= PORTAL_STANDABLE_NORMAL_Y {
                continue;
            }
            let relative = step.velocity - gate.carry / step.delta;
            let offset = center - frame.center;
            let distance = offset.dot(normal);
            let approach = relative.dot(normal);
            if distance <= 0.0 || approach > 0.0 || (approach == 0.0 && step.gravity * normal.y <= 0.0) {
                continue;
            }
            let projected = offset - Vec3::Y * (distance / normal.y);
            let inside = |offset: Vec3| {
                offset.dot(frame.right).abs() <= frame.size.half_width() + step.config.capture_margin
                    && offset.dot(frame.up).abs() <= frame.size.half_height() + step.config.capture_margin
            };
            if !inside(projected) {
                continue;
            }
            let reach = body_support(&shape, normal);
            let Some(time) = arrival_time(
                (distance - reach).max(0.0),
                relative,
                normal,
                step.gravity,
                step.velocity.y,
            ) else {
                continue;
            };
            if best.as_ref().is_some_and(|(earlier, _, _)| *earlier <= time) {
                continue;
            }
            // Do not catch an unrelated fast fly-by just because it passes over the opening.
            let future = projected + relative.with_y(0.0) * time;
            let future = future - Vec3::Y * (future.dot(normal) / normal.y);
            if !inside(future) {
                continue;
            }
            let target = frame.center + gate.carry / step.delta * time + Vec3::Y * (reach / normal.y);
            // One capsule sweep for a shortlisted candidate: floors/walls/fields
            // between the player and the opening prevent capture. The opening's
            // own backing and the gate currently straddled are already holes.
            let mut excluded = self.collision_exclusions(step.origin, step.physics);
            excluded.extend_from_slice(&gate.backing);
            let target_origin = step.origin + target - center;
            if !step.world.character_sweep_clear(
                step.origin.into(),
                target_origin.into(),
                step.physics,
                step.passable_fields,
                &excluded,
            ) {
                continue;
            }
            best = Some((time, frame.center - center, gate.carry / step.delta));
        }
        let Some((time, offset, carrier_velocity)) = best else {
            return FunnelCorrection::default();
        };
        // One tick is the shortest actionable deadline. A late near-miss can
        // still hit the rim; collision resolution always remains authoritative.
        let time = time.max(step.delta);
        let t = step.delta;
        let offset = offset.with_y(0.0);
        let relative = (step.velocity - carrier_velocity).with_y(0.0);
        let a = offset * (3.0 / (time * time)) - relative * (2.0 / time);
        let b = offset * (-2.0 / (time * time * time)) + relative / (time * time);
        FunnelCorrection {
            displacement: a * t * t + b * t * t * t,
            velocity_change: a * (2.0 * t) + b * (3.0 * t * t),
        }
    }
}

// First approach to a plane, including terminal fall speed and ceiling apexes.
fn arrival_time(distance: f32, velocity: Vec3, normal: Vec3, gravity: f32, vertical: f32) -> Option<f32> {
    if distance <= PHYSICS_EPSILON {
        return Some(0.0);
    }
    let speed = velocity.dot(normal);
    let acceleration = -gravity * normal.y;
    let time = plane_time(distance, speed, acceleration)?;
    if gravity <= PHYSICS_EPSILON || vertical - gravity * time >= -CHARACTER_TERMINAL_VELOCITY {
        return Some(time);
    }
    let terminal_time = ((vertical + CHARACTER_TERMINAL_VELOCITY) / gravity).max(0.0);
    let remaining = distance + speed * terminal_time + 0.5 * acceleration * terminal_time * terminal_time;
    let terminal_speed = speed + acceleration * terminal_time;
    (terminal_speed < -PHYSICS_EPSILON).then(|| terminal_time + remaining / -terminal_speed)
}

fn plane_time(distance: f32, speed: f32, acceleration: f32) -> Option<f32> {
    if acceleration.abs() <= PHYSICS_EPSILON {
        return (speed < -PHYSICS_EPSILON).then(|| distance / -speed);
    }
    let discriminant = speed * speed - 2.0 * acceleration * distance;
    if discriminant < 0.0 {
        return None;
    }
    let denominator = -speed + discriminant.sqrt();
    (denominator > PHYSICS_EPSILON).then(|| 2.0 * distance / denominator)
}
