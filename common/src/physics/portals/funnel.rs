use bevy_math::Vec3;

use super::{PortalFrame, PortalSet, traversal::body_support};
use crate::{
    config::{CharacterPhysicsConfig, PortalFunnelConfig},
    constants::{CHARACTER_TERMINAL_VELOCITY, PORTAL_STANDABLE_NORMAL_Y},
    math::PHYSICS_EPSILON,
    physics::{CollisionWorld, character_movement_center, character_movement_shape},
    protocol::FieldId,
};

// What the funnel judges a gate by: the body centre's vertical projection
// onto the plane, relative to a point on it, now and when the capsule meets
// the plane at the current horizontal velocity.
#[derive(Debug, Clone, Copy)]
pub struct FunnelPrediction {
    pub offset: Vec3,
    pub arrival: Vec3,
    pub time: f32,
}

// `None` while the body is behind the plane, moving away from it, or at an
// apex gravity will not bring down onto it.
pub(crate) fn funnel_prediction(
    plane_point: Vec3,
    normal: Vec3,
    body_center: Vec3,
    reach: f32,
    relative: Vec3,
    gravity: f32,
    vertical: f32,
) -> Option<FunnelPrediction> {
    let offset = body_center - plane_point;
    let distance = offset.dot(normal);
    let approach = relative.dot(normal);
    if distance <= 0.0 || approach > 0.0 || (approach == 0.0 && gravity * normal.y <= 0.0) {
        return None;
    }
    let projected = offset - Vec3::Y * (distance / normal.y);
    let time = arrival_time((distance - reach).max(0.0), relative, normal, gravity, vertical)?;
    // Do not catch an unrelated fast fly-by just because it passes over the opening.
    let future = projected + relative.with_y(0.0) * time;
    Some(FunnelPrediction {
        offset: projected,
        arrival: future - Vec3::Y * (future.dot(normal) / normal.y),
        time,
    })
}

// The funnel's prediction for a body over a level plane, as a floor portal
// centred on `plane_point` would be judged.
#[must_use]
pub fn floor_funnel_prediction(
    plane_point: Vec3,
    origin: Vec3,
    physics: CharacterPhysicsConfig,
    velocity: Vec3,
    gravity: f32,
) -> Option<FunnelPrediction> {
    funnel_prediction(
        plane_point,
        Vec3::Y,
        character_movement_center(origin.into(), physics),
        body_support(&character_movement_shape(physics), Vec3::Y),
        velocity,
        gravity,
        velocity.y,
    )
}

// Capture is the aperture expanded by the authored margin, met both now and on arrival.
#[must_use]
pub fn funnel_captures(frame: &PortalFrame, margin: f32, prediction: &FunnelPrediction) -> bool {
    let inside = |offset: Vec3| {
        offset.dot(frame.right).abs() <= frame.size.half_width() + margin
            && offset.dot(frame.up).abs() <= frame.size.half_height() + margin
    };
    inside(prediction.offset) && inside(prediction.arrival)
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
    // time before the capsule meets the rim. The result is this tick's share
    // of the slide that lands the body on the aperture's centre by then. It
    // moves the body and never its velocity: what goes in at an angle leaves
    // the other end at that angle. Recompute from actual motion each tick; no
    // future physics ticks or per-player funnel state. Moving gates use their
    // current linear velocity.
    pub(crate) fn funnel_correction(&self, step: FunnelStep<'_>) -> Option<Vec3> {
        if self.is_empty() || step.config.capture_margin <= 0.0 || step.delta <= 0.0 {
            return None;
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
            let reach = body_support(&shape, normal);
            let Some(prediction) = funnel_prediction(
                frame.center,
                normal,
                center,
                reach,
                relative,
                step.gravity,
                step.velocity.y,
            ) else {
                continue;
            };
            let time = prediction.time;
            if !funnel_captures(frame, step.config.capture_margin, &prediction)
                || best.as_ref().is_some_and(|(earlier, _, _)| *earlier <= time)
            {
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
        let (time, offset, carrier_velocity) = best?;
        // One tick is the shortest actionable deadline. A late near-miss can
        // still hit the rim; collision resolution always remains authoritative.
        let time = time.max(step.delta);
        let relative = (step.velocity - carrier_velocity).with_y(0.0);
        // Left alone the body drifts `relative * time` on the way down; the
        // slide covers what then still separates it from the centre.
        Some((offset.with_y(0.0) - relative * time) * (step.delta / time))
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
