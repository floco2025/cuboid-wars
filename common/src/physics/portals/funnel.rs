use bevy_math::Vec3;

use super::{PortalFrame, PortalSet, traversal::body_support};
use crate::{
    config::{CharacterPhysicsConfig, PortalFunnelConfig},
    constants::{CHARACTER_TERMINAL_VELOCITY, PORTAL_STANDABLE_NORMAL_Y},
    math::PHYSICS_EPSILON,
    physics::{CollisionWorld, character_movement_center, character_movement_shape},
    protocol::FieldId,
};

// What the funnel judges a gate by: where on the plane, relative to a point
// on it, the body centre's vertical projection will be when the capsule
// meets the plane, how long that takes, and the horizontal drift on the way.
#[derive(Debug, Clone, Copy)]
pub struct FunnelPrediction {
    pub arrival: Vec3,
    pub time: f32,
    pub drift: Vec3,
}

// The motion a released body is judged and corrected by.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FunnelMotion {
    // Relative to the gate.
    pub velocity: Vec3,
    pub gravity: f32,
    // The air braking released input is under, in m/s², and the tick it is applied per.
    pub brake: f32,
    pub delta: f32,
}

// `None` while the body is behind the plane, moving away from it, or at an
// apex gravity will not bring down onto it.
pub(crate) fn funnel_prediction(
    plane_point: Vec3,
    normal: Vec3,
    body_center: Vec3,
    reach: f32,
    motion: FunnelMotion,
) -> Option<FunnelPrediction> {
    let offset = body_center - plane_point;
    let distance = offset.dot(normal);
    let approach = motion.velocity.dot(normal);
    if distance <= 0.0 || approach > 0.0 || (approach == 0.0 && motion.gravity * normal.y <= 0.0) {
        return None;
    }
    let projected = offset - Vec3::Y * (distance / normal.y);
    let time = arrival_time(
        (distance - reach).max(0.0),
        motion.velocity,
        normal,
        motion.gravity,
        motion.velocity.y,
    )?;
    let heading = motion.velocity.with_y(0.0);
    let drift = heading.normalize_or_zero() * braked_travel(heading.length(), motion.brake, time, motion.delta);
    let future = projected + drift;
    Some(FunnelPrediction {
        arrival: future - Vec3::Y * (future.dot(normal) / normal.y),
        time,
        drift,
    })
}

// How far a released body travels in `time` while air braking takes `brake`
// off its speed each tick, as the motor does: every tick moves at the speed
// braking left it, and the last one only as far as the time allows.
fn braked_travel(speed: f32, brake: f32, time: f32, delta: f32) -> f32 {
    if brake <= 0.0 || delta <= 0.0 {
        return speed * time;
    }
    let mut travel = 0.0;
    let mut speed = speed;
    let mut left = time;
    while left > 0.0 && speed > 0.0 {
        let step = left.min(delta);
        travel += speed * step;
        speed -= brake * delta;
        left -= step;
    }
    travel
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
    brake: f32,
    delta: f32,
) -> Option<FunnelPrediction> {
    funnel_prediction(
        plane_point,
        Vec3::Y,
        character_movement_center(origin.into(), physics),
        body_support(&character_movement_shape(physics), Vec3::Y),
        FunnelMotion {
            velocity,
            gravity,
            brake,
            delta,
        },
    )
}

// Capture is judged where the body will arrive: within the aperture grown by
// the margin for the time it still has to fly. Aiming errors grow with the
// flight, so the catch is widest from far away and narrows to the mouth.
#[must_use]
pub fn funnel_captures(frame: &PortalFrame, config: PortalFunnelConfig, prediction: &FunnelPrediction) -> bool {
    let margin = config.margin_at(prediction.time);
    prediction.arrival.dot(frame.right).abs() <= frame.size.half_width() + margin
        && prediction.arrival.dot(frame.up).abs() <= frame.size.half_height() + margin
}

pub(crate) struct FunnelStep<'a> {
    pub origin: Vec3,
    pub physics: CharacterPhysicsConfig,
    pub velocity: Vec3,
    pub gravity: f32,
    // Air braking on released input, which the prediction of the arrival includes.
    pub brake: f32,
    pub delta: f32,
    pub config: PortalFunnelConfig,
    pub world: &'a CollisionWorld,
    pub passable_fields: &'a [FieldId],
}

impl PortalSet {
    // Called once by the airborne player policy, only without movement intent.
    // Capture is the vertical projection of the body's arrival onto a
    // floor/ceiling/ramp aperture, within the margin for the time left. A
    // ballistic prediction supplies the time before the capsule meets the
    // rim. The result is this tick's share
    // of the slide that lands the body on the aperture's centre by then. It
    // moves the body and never its velocity: what goes in at an angle leaves
    // the other end at that angle. Recompute from actual motion each tick; no
    // future physics ticks or per-player funnel state. Moving gates use their
    // current linear velocity.
    pub(crate) fn funnel_correction(&self, step: FunnelStep<'_>) -> Option<Vec3> {
        if self.is_empty() || !step.config.assists() || step.delta <= 0.0 {
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
            let reach = body_support(&shape, normal);
            let Some(prediction) = funnel_prediction(
                frame.center,
                normal,
                center,
                reach,
                FunnelMotion {
                    velocity: step.velocity - gate.carry / step.delta,
                    gravity: step.gravity,
                    brake: step.brake,
                    delta: step.delta,
                },
            ) else {
                continue;
            };
            let time = prediction.time;
            if !funnel_captures(frame, step.config, &prediction)
                || best.as_ref().is_some_and(|(earlier, _)| *earlier <= time)
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
            best = Some((time, (frame.center - center).with_y(0.0) - prediction.drift));
        }
        let (time, gap) = best?;
        // One tick is the shortest actionable deadline. A late near-miss can
        // still hit the rim; collision resolution always remains authoritative.
        let time = time.max(step.delta);
        // Left alone the body drifts on the way down; the slide covers what
        // then still separates it from the centre, at an even rate.
        Some(gap * (step.delta / time))
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
