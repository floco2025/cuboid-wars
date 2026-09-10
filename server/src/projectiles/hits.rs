use bevy::prelude::*;
use common::protocol::*;

use super::PendingProjectileHits;
use crate::{
    actors::ActorMap,
    combat::{
        DeathSource, PendingExplosions, apply_actor_projectile_hit, apply_player_projectile_hit, award_actor_kill,
        kill_player,
    },
    config::ServerGameplayConfig,
    network::broadcast_to_all,
    players::{Invincibility, PlayerMap},
    quests::{QuestBoard, QuestCatalog},
};

pub(super) fn projectile_hits_system(
    mut commands: Commands,
    mut pending: ResMut<PendingProjectileHits>,
    mut players: ResMut<PlayerMap>,
    mut actors: ResMut<ActorMap>,
    mut targets: Query<(&Position, &mut Health)>,
    config: Res<ServerGameplayConfig>,
    invincibility: Res<Invincibility>,
    mut quest_board: ResMut<QuestBoard>,
    quest_catalog: Res<QuestCatalog>,
    mut pending_explosions: ResMut<PendingExplosions>,
) {
    for (shooter, hit) in pending.0.drain(..) {
        // Death preserves in-flight hits; disconnect ends the shooter's authority.
        if !players.get(&shooter).is_some_and(|info| info.connection.logged_in) {
            continue;
        }
        match hit.target {
            HitTarget::Player { id, generation } => {
                let Some(target) = players
                    .get(&id)
                    .filter(|info| info.session.generation == generation)
                    .and_then(|info| info.entity())
                else {
                    continue;
                };
                let Ok((position, mut health)) = targets.get_mut(target) else {
                    continue;
                };
                let death_pos = *position;
                let lethal = apply_player_projectile_hit(&players, id, &mut health, &config, invincibility.0);
                broadcast_to_all(
                    &players,
                    ServerMessage::PlayerHit(SPlayerHit {
                        id,
                        generation,
                        kind: HitKind::Projectile,
                        hit_dir_x: hit.direction[0],
                        hit_dir_z: hit.direction[1],
                        health: *health,
                    }),
                );
                if lethal {
                    kill_player(
                        &mut commands,
                        &mut players,
                        id,
                        target,
                        death_pos,
                        config.player.respawn_secs,
                        DeathSource::Shot(shooter),
                        &config,
                        &mut pending_explosions,
                    );
                }
            }
            HitTarget::Actor(id) => {
                let Some(actor) = actors.get(&id) else {
                    continue;
                };
                let Ok((_, mut health)) = targets.get_mut(actor.entity) else {
                    continue;
                };
                let kind = actor.spawn_kind.clone();
                let lethal = apply_actor_projectile_hit(&mut players, &shooter, &kind, &mut health, &config);
                if lethal {
                    award_actor_kill(&mut players, &mut quest_board, &quest_catalog, shooter, &kind, &config);
                    actors
                        .get_mut(&id)
                        .expect("projectile target actor missing")
                        .last_damager = Some(shooter);
                }
                broadcast_to_all(&players, ServerMessage::ActorHit(SActorHit { id, health: *health }));
            }
        }
    }
}
