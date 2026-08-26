use std::collections::HashMap;

use bevy::prelude::*;

use super::{DeathSource, apply_player_beam_damage, kill_player};
use crate::{
    actors::{ActorMap, BeamState},
    combat::PendingExplosions,
    config::ServerGameplayConfig,
    network::broadcast_to_all,
    players::{Invincibility, PlayerMap},
};
use common::{
    config::GameplayConfig,
    constants::PHYSICS_EPSILON,
    physics::{CollisionWorld, character_center},
    protocol::{ActorId, ActorMarker, Health, HitKind, PlayerMarker, Position, SPlayerHit, ServerMessage},
};

// Cadence for the beam victim's `SPlayerHit` cue (camera shake + HUD
// health). Damage applies every tick; re-shaking at 30 Hz would blur into
// one long judder and flood the wire.
const BEAM_HIT_CUE_INTERVAL_SECS: f32 = 0.25;

// Burn the locked target of every mid-burst laser actor. The beam hits when
// the target is within `fire.range` and the actor-center → target-center
// line of sight is clear of static world (the beam origin is the collider
// center — deliberately not the perception eye height, matching contact
// explosions). Lethal ticks run the standard death sequence with no killer
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
    actor_positions: Query<&Position, (With<ActorMarker>, Without<PlayerMarker>)>,
    mut player_query: Query<(&Position, &mut Health), With<PlayerMarker>>,
    // Per-actor time before which no further hit cue is sent — beam-cue
    // presentation state, so it lives here rather than on `ActorInfo`.
    mut next_cue_at: Local<HashMap<ActorId, f32>>,
) {
    let delta = time.delta_secs();
    let now = time.elapsed_secs();
    let respawn_delay_secs = gameplay_config.player.respawn_delay_secs;
    let player_physics = gameplay_config.player.physics();
    next_cue_at.retain(|id, _| actors.get(id).is_some());

    for (actor_id, info) in actors.iter() {
        let BeamState::Firing { target: target_id, .. } = info.beam else {
            continue;
        };
        let Ok(actor_pos) = actor_positions.get(info.entity) else {
            continue;
        };
        let Some(target_entity) = players
            .get(&target_id)
            .filter(|player| player.logged_in && !player.is_dead())
            .map(|player| player.entity)
        else {
            continue;
        };
        let Ok((target_pos, mut target_health)) = player_query.get_mut(target_entity) else {
            continue;
        };
        let kind_config = server_gameplay_config.expect_actor(&info.spawn_kind);
        let Some(fire) = kind_config.combat.attack.beam() else {
            continue;
        };

        if actor_pos.distance_sq(target_pos) > fire.range * fire.range {
            continue;
        }
        let actor_physics = gameplay_config.expect_actor(&info.spawn_kind).physics();
        let target_center = Vec3::new(
            target_pos.x,
            player_physics.collider_center_y(target_pos.y),
            target_pos.z,
        );
        if !collision_world.line_of_sight_clear(character_center(*actor_pos, actor_physics), target_center) {
            continue;
        }

        let lethal = apply_player_beam_damage(
            &players,
            target_id,
            &mut target_health,
            fire.damage_per_second * delta,
            &server_gameplay_config,
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
                respawn_delay_secs,
                DeathSource::Beam {
                    kind: info.spawn_kind.clone(),
                },
                &server_gameplay_config.feed,
                &mut pending_explosions,
            );
        }
    }
}
