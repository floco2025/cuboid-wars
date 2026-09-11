use std::collections::HashMap;

use bevy::prelude::*;

use super::{DeathSource, apply_player_beam_damage, kill_player};
use crate::{
    actors::ActorMap,
    combat::PendingExplosions,
    config::ServerGameplayConfig,
    network::broadcast_to_all,
    players::{Invincibility, PlayerMap},
};
use common::{
    config::GameplayConfig,
    math::PHYSICS_EPSILON,
    physics::CollisionWorld,
    protocol::{ActorId, ActorMarker, Health, HitKind, PlateState, PlayerMarker, Position, SPlayerHit, ServerMessage},
};

// Cadence for the beam victim's `SPlayerHit` cue (camera shake + HUD
// health). Damage applies every tick; re-shaking at 30 Hz would blur into
// one long judder and flood the wire.
const BEAM_HIT_CUE_INTERVAL_SECS: f32 = 0.25;

// Burn the locked target of every firing laser actor. The beam hits when
// the target is within `fire.range` and the emitter → target-center
// attack path is clear of world geometry and active fields. The emitter
// uses the actor's beam origin height, independently of its body dimensions.
// Lethal ticks run the standard death sequence with no killer
// credit, like falls and blasts. A throttled `SPlayerHit` cue gives the
// victim the directional camera shake and an instant HUD health update.
pub fn actors_beam_damage_system(
    mut commands: Commands,
    time: Res<Time>,
    mut players: ResMut<PlayerMap>,
    actors: Res<ActorMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    invincibility: Res<Invincibility>,
    collision_world: Res<CollisionWorld>,
    plates: Res<PlateState>,
    actor_positions: Query<&Position, (With<ActorMarker>, Without<PlayerMarker>)>,
    mut player_query: Query<(&Position, &mut Health), With<PlayerMarker>>,
    // Per-actor time before which no further hit cue is sent — beam-cue
    // presentation state, so it lives here rather than on `ActorInfo`.
    mut next_cue_at: Local<HashMap<ActorId, f32>>,
) {
    if actors.peaceful {
        return;
    }
    let delta = time.delta_secs();
    let now = time.elapsed_secs();
    let respawn_secs = server_gameplay_config.player.respawn_secs;
    let player_physics = gameplay_config.player.physics();
    next_cue_at.retain(|id, _| actors.get(id).is_some());

    for (actor_id, info) in actors.iter() {
        let Some(target_id) = info.beam.target() else {
            continue;
        };
        let Ok(actor_pos) = actor_positions.get(info.entity) else {
            continue;
        };
        let Some((target_entity, target_generation)) = players
            .get(&target_id)
            .filter(|player| player.connection.logged_in)
            .and_then(|player| Some((player.entity()?, player.session.generation)))
        else {
            continue;
        };
        let Ok((target_pos, mut target_health)) = player_query.get_mut(target_entity) else {
            continue;
        };
        let kind_config = server_gameplay_config.expect_actor(&info.spawn_kind);
        let Some(range) = kind_config.attack.beam_range() else {
            continue;
        };

        if actor_pos.distance_sq(target_pos) > range * range {
            continue;
        }
        let actor_config = gameplay_config.expect_actor(&info.spawn_kind);
        let target_center = Vec3::new(target_pos.x, player_physics.hitbox_center_y(target_pos.y), target_pos.z);
        if !collision_world.attack_path_clear(
            Vec3::from(*actor_pos) + Vec3::Y * actor_config.beam_origin_y_offset(),
            target_center,
            &plates.open_barriers,
        ) {
            continue;
        }

        let beam_dps = server_gameplay_config
            .combat
            .damage
            .expect_actor(&info.spawn_kind)
            .beam_dps
            .expect("beam kind lacks beam_dps after validation");
        let lethal = apply_player_beam_damage(
            &players,
            target_id,
            &mut target_health,
            beam_dps * delta,
            invincibility.0,
        );

        if next_cue_at.get(actor_id).is_none_or(|at| now >= *at) {
            next_cue_at.insert(*actor_id, now + BEAM_HIT_CUE_INTERVAL_SECS);
            let dx = target_pos.x - actor_pos.x;
            let dz = target_pos.z - actor_pos.z;
            let length = (dx * dx + dz * dz).sqrt();
            let (hit_dir_x, hit_dir_z) = if length > PHYSICS_EPSILON {
                (dx / length, dz / length)
            } else {
                (0.0, 0.0)
            };
            broadcast_to_all(
                &players,
                ServerMessage::PlayerHit(SPlayerHit {
                    id: target_id,
                    generation: target_generation,
                    kind: HitKind::Beam,
                    hit_dir_x,
                    hit_dir_z,
                    health: *target_health,
                }),
            );
        }

        if lethal {
            let death_pos = *target_pos;
            info!(
                "{} was burned down by {}'s beam",
                players.describe(&target_id),
                actors.describe(actor_id)
            );
            kill_player(
                &mut commands,
                &mut players,
                target_id,
                target_entity,
                death_pos,
                respawn_secs,
                DeathSource::Beam {
                    kind: info.spawn_kind.clone(),
                },
                &server_gameplay_config,
                &mut pending_explosions,
            );
        }
    }
}
