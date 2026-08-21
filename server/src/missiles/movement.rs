use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::ActorMap,
    combat::PendingExplosions,
    map::OpenBarrierKinds,
    missiles::{MissileMap, MissileVelocity},
    network::broadcast_to_all,
    players::PlayerMap,
};
use common::{
    config::GameplayConfig,
    constants::MISSILE_RADIUS,
    physics::{CollisionWorld, ball_character_hit, ball_overlaps_character},
    protocol::*,
};

// Broadcast a course intent once the steered direction has drifted this far
// (radians) from the last broadcast. Straight flight sends nothing; a
// full-rate turn re-sends every tick, bounded by the handful of live
// missiles the per-player ammo cap allows.
const MISSILE_INTENT_EPSILON_RAD: f32 = 0.05;

type MissileMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static MissileId,
        &'static mut Position,
        &'static MissileVelocity,
    ),
    With<MissileMarker>,
>;

type MissilePlayerQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Position, &'static FaceYaw, &'static PlayerId),
    (With<PlayerMarker>, Without<ActorMarker>, Without<MissileMarker>),
>;

type MissileActorQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Position, &'static FaceYaw, &'static ActorId),
    (With<ActorMarker>, Without<PlayerMarker>, Without<MissileMarker>),
>;

#[derive(SystemParam)]
pub struct MissileMovementParams<'w, 's> {
    missile_query: MissileMovementQuery<'w, 's>,
    player_query: MissilePlayerQuery<'w, 's>,
    actor_query: MissileActorQuery<'w, 's>,
    missiles: ResMut<'w, MissileMap>,
    players: Res<'w, PlayerMap>,
    actors: Res<'w, ActorMap>,
    pending_explosions: ResMut<'w, PendingExplosions>,
    collision_world: Res<'w, CollisionWorld>,
    open_barrier_kinds: Res<'w, OpenBarrierKinds>,
    gameplay_config: Res<'w, GameplayConfig>,
}

pub fn missiles_movement_system(mut commands: Commands, time: Res<Time>, mut params: MissileMovementParams) {
    let delta = time.delta_secs();

    for (entity, id, mut pos, velocity) in &mut params.missile_query {
        let Some(info) = params.missiles.get_mut(id) else {
            continue;
        };

        // Arm self-hits once the missile has cleared the shooter's collider.
        // A missing shooter arms immediately.
        if !info.armed {
            let overlaps_shooter = params
                .players
                .get(&info.shooter)
                .and_then(|shooter| params.player_query.get(shooter.entity).ok())
                .is_some_and(|(shooter_pos, face_yaw, _)| {
                    ball_overlaps_character(
                        &pos,
                        MISSILE_RADIUS,
                        shooter_pos,
                        face_yaw.0,
                        params.gameplay_config.player.physics(),
                    )
                });
            if !overlaps_shooter {
                info.armed = true;
            }
        }
        let armed = info.armed;
        let shooter = info.shooter;

        if let Some(impact) = info.detonate_at {
            detonate_missile(
                &mut commands,
                &mut params.missiles,
                &params.players,
                &mut params.pending_explosions,
                entity,
                *id,
                impact,
            );
            continue;
        }

        let translation = velocity.0 * delta;
        let origin = Vec3::from(*pos);

        let mut earliest_t: Option<f32> = None;
        let mut consider = |t: f32| {
            if earliest_t.is_none_or(|current| t < current) {
                earliest_t = Some(t);
            }
        };
        if let Some(hit) = params
            .collision_world
            .cast_moving_ball(origin, translation, MISSILE_RADIUS)
        {
            consider(hit.t);
        }
        if let Some(hit) = params.collision_world.cast_moving_ball_against_barriers(
            origin,
            translation,
            MISSILE_RADIUS,
            &params.open_barrier_kinds.0,
        ) {
            consider(hit.t);
        }
        for (target_pos, face_yaw, player_id) in &params.player_query {
            if *player_id == shooter && !armed {
                continue;
            }
            if let Some(hit) = ball_character_hit(
                &pos,
                velocity.0,
                MISSILE_RADIUS,
                delta,
                target_pos,
                face_yaw.0,
                params.gameplay_config.player.physics(),
            ) {
                consider(hit.time_of_impact);
            }
        }
        for (target_pos, face_yaw, actor_id) in &params.actor_query {
            let actor_info = params
                .actors
                .get(actor_id)
                .expect("actor in query missing from ActorMap");
            let physics = params.gameplay_config.expect_actor(&actor_info.spawn_kind).physics();
            if let Some(hit) =
                ball_character_hit(&pos, velocity.0, MISSILE_RADIUS, delta, target_pos, face_yaw.0, physics)
            {
                consider(hit.time_of_impact);
            }
        }

        match earliest_t {
            Some(t) => {
                let impact: Position = (origin + translation * t).into();
                detonate_missile(
                    &mut commands,
                    &mut params.missiles,
                    &params.players,
                    &mut params.pending_explosions,
                    entity,
                    *id,
                    impact,
                );
            }
            None => {
                *pos += translation;
                let dir = velocity.0.normalize_or_zero();
                if course_drifted(dir, info.last_broadcast_dir) {
                    info.last_broadcast_dir = dir;
                    broadcast_to_all(
                        &params.players,
                        ServerMessage::MissileMove(SMissileMove {
                            id: *id,
                            movement: MissileMovementState::from_velocity(*pos, velocity.0),
                        }),
                    );
                }
            }
        }
    }
}

fn detonate_missile(
    commands: &mut Commands,
    missiles: &mut MissileMap,
    players: &PlayerMap,
    pending_explosions: &mut PendingExplosions,
    entity: Entity,
    id: MissileId,
    pos: Position,
) {
    let Some(info) = missiles.remove(&id) else {
        return;
    };
    broadcast_to_all(players, ServerMessage::MissileDeath(SMissileDeath { id, pos }));
    pending_explosions.push_missile(info.shooter, pos);
    commands.entity(entity).despawn();
}

// The steered direction has drifted past the broadcast epsilon since the
// last `SMissileMove`. A zero direction (degenerate velocity) never
// broadcasts.
fn course_drifted(dir: Vec3, last_broadcast_dir: Vec3) -> bool {
    dir != Vec3::ZERO && dir.dot(last_broadcast_dir) < MISSILE_INTENT_EPSILON_RAD.cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotated(base: Vec3, radians: f32) -> Vec3 {
        Quat::from_rotation_y(radians) * base
    }

    #[test]
    fn course_within_epsilon_does_not_broadcast() {
        let last = Vec3::Z;
        assert!(!course_drifted(last, last), "identical heading is quiet");
        assert!(!course_drifted(rotated(last, 0.04), last), "sub-epsilon drift is quiet");
    }

    #[test]
    fn course_past_epsilon_broadcasts() {
        let last = Vec3::Z;
        assert!(course_drifted(rotated(last, 0.06), last));
        assert!(course_drifted(-last, last), "a reversal always broadcasts");
    }

    #[test]
    fn degenerate_direction_never_broadcasts() {
        assert!(!course_drifted(Vec3::ZERO, Vec3::Z));
    }
}
