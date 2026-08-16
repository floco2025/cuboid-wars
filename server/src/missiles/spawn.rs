use std::collections::VecDeque;
use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::RngExt;

use crate::{
    actors::ActorMap,
    config::ServerGameplayConfig,
    map::OpenBarrierKinds,
    missiles::{MissileInfo, MissileMap, MissileVelocity, guidance::sweep_clear},
    network::broadcast_to_all,
    players::{PlayerInfo, PlayerMap},
};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, character_center},
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
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    actors: &ActorMap,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection, &Health), With<ActorMarker>>,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
    open_barrier_kinds: &OpenBarrierKinds,
) {
    // Untrusted boundary: drop non-finite aim before it becomes a NaN
    // velocity. Checked before the ammo/cooldown gate so a bad message
    // doesn't burn a missile.
    if !(msg.face_dir.is_finite() && msg.face_pitch.is_finite()) {
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

    // Loose lock re-validation — the client aimed up to an RTT ago, so no
    // exact ray re-check: the target must exist, be alive, be in range, and
    // have clear sight. A failed check drops the message silently.
    let target_center = match msg.target {
        HomingTarget::Player(target_id) => {
            let Some(info) = players.get(&target_id) else {
                return;
            };
            if !info.logged_in || info.is_dead() {
                return;
            }
            let Ok((pos, _, _, _)) = player_data.get(info.entity) else {
                return;
            };
            character_center(*pos, gameplay_config.player.physics())
        }
        HomingTarget::Actor(target_id) => {
            let Some(info) = actors.get(&target_id) else {
                return;
            };
            let Ok((pos, _, _, _)) = actor_data.get(info.entity) else {
                return;
            };
            character_center(*pos, gameplay_config.validated_actor(&info.spawn_kind).physics())
        }
    };
    let max_distance = gameplay_config.missiles.lock_max_distance + LOCK_RANGE_GRACE;
    if eye.distance_squared(target_center) > max_distance * max_distance {
        return;
    }
    if !collision_world.line_of_sight_clear(eye, target_center) {
        return;
    }

    if !players.get_mut(&id).is_some_and(PlayerInfo::try_start_missile) {
        return;
    }

    commands.entity(entity).insert(FaceDirection(msg.face_dir));

    let missile_config = server_gameplay_config.missiles;
    let pitch_cos = msg.face_pitch.cos();
    let dir = Vec3::new(
        msg.face_dir.sin() * pitch_cos,
        msg.face_pitch.sin(),
        msg.face_dir.cos() * pitch_cos,
    );
    // Muzzle stays on the aim axis; only the flight direction is perturbed,
    // so the steering has something visible to correct.
    let muzzle = eye + dir * missile_config.spawn_offset;
    let spawn_pos: Position = muzzle.into();
    let launch_dir = clear_launch_direction(
        dir,
        missile_config.launch_spread_degrees.to_radians(),
        muzzle,
        missile_config.speed * LAUNCH_CLEAR_SECS,
        missile_config.radius,
        collision_world,
        &open_barrier_kinds.0,
        &mut rand::rng(),
    );
    let velocity = launch_dir * missile_config.speed;

    let missile_id = missiles.allocate();
    let missile_entity = commands
        .spawn((MissileMarker, missile_id, spawn_pos, MissileVelocity(velocity)))
        .id();
    missiles.insert(
        missile_id,
        MissileInfo {
            entity: missile_entity,
            shooter: id,
            target: Some(msg.target),
            path: VecDeque::new(),
            path_target: None,
            path_retry_timer: 0.0,
            avoid_dir: None,
            avoid_timer: 0.0,
            last_target_center: None,
            lifetime_timer: missile_config.lifetime_secs,
            stall_anchor: None,
            stall_timer: missile_config.stall_secs,
            armed: false,
            last_broadcast_dir: dir,
            detonate_at: None,
        },
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
