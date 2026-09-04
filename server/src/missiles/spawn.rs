use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::RngExt;

use crate::{
    actors::{ActorMap, ActorStateQuery},
    config::ServerGameplayConfig,
    missiles::{MissileInfo, MissileMap, MissileVelocity, steering::sweep_clear},
    network::broadcast_to_all,
    players::{PlayerMap, PlayerStateQuery},
};
use common::constants::MISSILE_SPAWN_OFFSET;
use common::{
    config::GameplayConfig,
    constants::MISSILE_RADIUS,
    physics::{CollisionWorld, acquire_lock},
    protocol::*,
};

// Latency slack on the server-side range re-check: an honest client's lock
// was valid up to an RTT ago, so the target may have drifted past the exact
// lock radius by the time the fire message lands.
const LOCK_RANGE_GRACE: f32 = 2.0;
// A launch direction must have this many seconds of clear runway, so the
// random spread can't point a fresh missile straight into a nearby wall
// its turn rate has no room to recover from.
const LAUNCH_CLEAR_SECS: f32 = 0.5;
const LAUNCH_SAMPLES: usize = 8;

pub fn handle_missile_shot_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: &CMissileShot,
    players: &mut PlayerMap,
    missiles: &mut MissileMap,
    player_data: &PlayerStateQuery,
    actors: &ActorMap,
    actor_data: &ActorStateQuery,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
    map_settings: &MapSettings,
    plates: &PlateState,
    unlimited_missiles: bool,
) {
    if !map_settings.weapons.missiles {
        return;
    }
    // Untrusted boundary: drop non-finite aim before it becomes a NaN
    // velocity. Checked before the ammo/cooldown gate so a bad message
    // doesn't burn a missile.
    if !(msg.face_yaw.is_finite() && msg.face_pitch.is_finite()) {
        return;
    }

    let Ok((shooter_pos, _, _, _)) = player_data.get(entity) else {
        return;
    };
    let eye = Vec3::new(
        shooter_pos.x,
        shooter_pos.y + gameplay_config.player.eye_height(),
        shooter_pos.z,
    );

    // Lock re-validation with the SAME shared `acquire_lock` the client's
    // crosshair uses, re-run against current authoritative positions with
    // the aim the fire message carried. Red crosshair and server acceptance
    // agree by construction; the assist radius plus the range grace absorb
    // the target's drift over the message's RTT. With `require_lock` off, a
    // shot whose claim fails (or that never claimed) launches unguided along
    // the aim instead; with it on, it drops silently.
    let aim = common::math::direction_from_yaw_pitch(msg.face_yaw, msg.face_pitch);
    let validated = msg.target.filter(|claimed| {
        let candidates = players
            .iter()
            .filter(|(target_id, _)| **target_id != id)
            .filter_map(|(target_id, info)| {
                let (pos, _, face_yaw, _) = player_data.get(info.entity()?).ok()?;
                Some((
                    HomingTarget::Player(*target_id),
                    *pos,
                    face_yaw.0,
                    gameplay_config.player.physics(),
                ))
            })
            .chain(actors.iter().filter_map(|(target_id, info)| {
                let (pos, _, face_yaw, _) = actor_data.get(info.entity).ok()?;
                Some((
                    HomingTarget::Actor(*target_id),
                    *pos,
                    face_yaw.0,
                    gameplay_config.expect_actor(&info.spawn_kind).physics(),
                ))
            }))
            .collect::<Vec<_>>();
        acquire_lock(
            collision_world,
            eye,
            aim,
            gameplay_config.missiles.lock_range + LOCK_RANGE_GRACE,
            gameplay_config.missiles.lock_assist_radius,
            candidates.into_iter(),
        ) == Some(*claimed)
    });
    if gameplay_config.missiles.require_lock && validated.is_none() {
        return;
    }

    if !players
        .get_mut(&id)
        .is_some_and(|player| player.try_start_missile(unlimited_missiles))
    {
        return;
    }

    commands.entity(entity).insert(FaceYaw(msg.face_yaw));

    let missile_config = server_gameplay_config.weapons.missiles;
    let missile_speed = map_settings.movement.missile_speed;
    let dir = aim;
    // An unguided shot flies exactly where aimed — random spread would just
    // make it useless.
    let spread = if validated.is_some() {
        missile_config.launch_spread_degrees.to_radians()
    } else {
        0.0
    };
    // Muzzle stays on the aim axis; only the flight direction is perturbed,
    // so the steering has something visible to correct.
    let muzzle = eye + dir * MISSILE_SPAWN_OFFSET;
    let spawn_pos: Position = muzzle.into();
    let launch_dir = clear_launch_direction(
        dir,
        spread,
        muzzle,
        missile_speed * LAUNCH_CLEAR_SECS,
        MISSILE_RADIUS,
        collision_world,
        &plates.open_barrier_kinds,
        &mut rand::rng(),
    );
    let velocity = launch_dir * missile_speed;

    let weave_phase = rand::rng().random_range(0.0..TAU);
    let missile_id = missiles.allocate();
    let missile_entity = commands
        .spawn((MissileMarker, missile_id, spawn_pos, MissileVelocity(velocity)))
        .id();
    missiles.insert(
        missile_id,
        MissileInfo::new(
            missile_entity,
            id,
            validated,
            dir,
            weave_phase,
            missile_config.lifetime_secs,
        ),
    );

    // To everyone including the shooter — clients don't predict missile
    // spawns; the server owns the whole flight.
    broadcast_to_all(
        players,
        ServerMessage::MissileLaunch(SMissileLaunch {
            id: missile_id,
            shooter: id,
            movement: MissileMovementState::from_velocity(spawn_pos, velocity),
        }),
    );
}

// Resample the random spread until the launch has a clear runway; a boxed-in
// muzzle falls back to the straight aim (which the lock re-validation just
// saw reach the target).
#[expect(clippy::too_many_arguments, reason = "launch sweep needs the full collision context")]
fn clear_launch_direction(
    aim: Vec3,
    spread_rad: f32,
    muzzle: Vec3,
    runway: f32,
    radius: f32,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    rng: &mut impl RngExt,
) -> Vec3 {
    for _ in 0..LAUNCH_SAMPLES {
        let candidate = launch_direction(aim, spread_rad, rng);
        if sweep_clear(collision_world, open_kinds, muzzle, candidate * runway, radius) {
            return candidate;
        }
    }
    aim
}

// Random direction within the spread cone around `aim`: uniform azimuth,
// tilt sampled in [spread/2, spread] — a minimum deviation so the corrective
// curve is always visible. Zero spread launches straight.
fn launch_direction(aim: Vec3, spread_rad: f32, rng: &mut impl RngExt) -> Vec3 {
    if spread_rad <= 0.0 {
        return aim;
    }
    let tilt = rng.random_range(spread_rad / 2.0..=spread_rad);
    let azimuth = rng.random_range(0.0..TAU);
    let tilt_axis = Quat::from_axis_angle(aim, azimuth) * aim.any_orthonormal_vector();
    Quat::from_axis_angle(tilt_axis, tilt) * aim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_direction_stays_within_spread() {
        let mut rng = rand::rng();
        let aim = Vec3::new(0.3, 0.5, 0.8).normalize();
        let spread = 50.0_f32.to_radians();
        for _ in 0..100 {
            let launched = launch_direction(aim, spread, &mut rng);
            assert!((launched.length() - 1.0).abs() < 1e-4, "direction stays unit length");
            let angle = aim.angle_between(launched);
            assert!(
                (spread / 2.0 - 1e-3..=spread + 1e-3).contains(&angle),
                "deviation {angle} outside [spread/2, spread]"
            );
        }
    }

    #[test]
    fn launch_direction_zero_spread_is_straight() {
        let mut rng = rand::rng();
        let aim = Vec3::Z;
        assert_eq!(launch_direction(aim, 0.0, &mut rng), aim);
    }
}
