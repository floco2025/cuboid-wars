use super::missile_blast_hits;
use crate::{
    actors::ActorMap,
    audio::play_explosion_sound,
    carriers::CarrierEntities,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    missiles::{AirGraph, MissileMap, MissileVelocity, OwnedMissile, guide_missile},
    network::{ClientToServer, ClientToServerChannel},
    players::PlayerMap,
    vfx::{BlastRadii, ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, spawn_missile_explosion},
};
use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::{GameplayConfig, NetworkConfig, UpdateCadence},
    constants::MISSILE_RADIUS,
    map::Carriers,
    physics::{CollisionWorld, ball_character_hit, ball_overlaps_character, character_hitbox_center},
    protocol::*,
};

type MissileQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static MissileId,
        &'static mut Position,
        &'static mut PreviousTickPosition,
        &'static mut MissileVelocity,
        &'static mut OwnedMissile,
    ),
    With<MissileMarker>,
>;
type TargetQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Position, &'static FaceYaw),
    (Or<(With<PlayerMarker>, With<ActorMarker>)>, Without<MissileMarker>),
>;

#[derive(SystemParam)]
pub struct MissileMovementParams<'w, 's> {
    missiles: ResMut<'w, MissileMap>,
    query: MissileQuery<'w, 's>,
    targets: TargetQuery<'w, 's>,
    players: Res<'w, PlayerMap>,
    actors: Res<'w, ActorMap>,
    world: Res<'w, CollisionWorld>,
    carriers: Res<'w, Carriers>,
    graph: Res<'w, AirGraph>,
    plates: Res<'w, PlateState>,
    gameplay: Res<'w, GameplayConfig>,
    settings: Res<'w, MapSettings>,
    blast_radii: Res<'w, BlastRadii>,
    to_server: Res<'w, ClientToServerChannel>,
    network: Res<'w, NetworkConfig>,
}

// The shooter is the one client that knows the exact impact point, so its own
// blast plays here instead of waiting for the server's cue.
#[derive(SystemParam)]
pub struct MissileBlastPresentation<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    budget: ResMut<'w, ExplosionVfxBudget>,
    explosion_assets: Res<'w, ExplosionAssets>,
    carrier_entities: Res<'w, CarrierEntities>,
    map_layout: Res<'w, MapLayout>,
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    client_settings: Res<'w, ClientSettings>,
}

pub fn missiles_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    mut cadence: Local<Option<UpdateCadence>>,
    mut params: MissileMovementParams,
    mut presentation: MissileBlastPresentation,
) {
    let delta = time.delta_secs();
    let send = cadence.get_or_insert_with(|| params.network.update_cadence()).ready();
    if params.query.is_empty() {
        return;
    }
    let mut updates = Vec::new();
    let bodies: Vec<_> = params
        .players
        .iter()
        .filter_map(|(id, info)| {
            if !params.players.accepts_generation(*id, info.generation) {
                return None;
            }
            let (pos, yaw) = params.targets.get(info.entity).ok()?;
            Some((
                HitTarget::Player {
                    id: *id,
                    generation: info.generation,
                },
                *pos,
                yaw.0,
                params.gameplay.player.physics(),
            ))
        })
        .chain(params.actors.iter().filter_map(|(id, info)| {
            let (pos, yaw) = params.targets.get(info.entity).ok()?;
            Some((
                HitTarget::Actor(*id),
                *pos,
                yaw.0,
                params.gameplay.expect_actor(&info.kind).physics(),
            ))
        }))
        .collect();
    for (entity, id, mut pos, mut previous, mut velocity, mut owned) in &mut params.query {
        if !params.missiles.contains_key(id) {
            continue;
        }
        previous.0 = *pos;
        owned.seq = owned.seq.wrapping_add(1);
        let flight = &mut owned.flight;
        let target = flight
            .target
            .and_then(|target| {
                bodies.iter().find(|(body, ..)| match (target, body) {
                    (HomingTarget::Player(id), HitTarget::Player { id: other, .. }) => id == *other,
                    (HomingTarget::Actor(id), HitTarget::Actor(other)) => id == *other,
                    _ => false,
                })
            })
            .map(|(_, pos, _, physics)| character_hitbox_center(*pos, *physics));
        velocity.0 = guide_missile(
            flight,
            &params.gameplay.missiles,
            &params.graph,
            &params.carriers,
            &params.world,
            &params.plates.open_barrier_kinds,
            *pos,
            target,
            velocity.0,
            params.settings.movement.missile_speed,
            delta,
        );
        if !flight.armed {
            flight.armed = !bodies.iter().any(|(target, body, yaw, physics)| {
                matches!(target, HitTarget::Player { id, .. } if *id == flight.shooter)
                    && ball_overlaps_character(&pos, MISSILE_RADIUS, body, *yaw, *physics)
            });
        }
        let translation = velocity.0 * delta;
        let origin = Vec3::from(*pos);
        let mut earliest = flight.detonate_at.map(|impact| {
            if translation.length_squared() > 0.0 {
                ((Vec3::from(impact) - origin).dot(translation) / translation.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        let mut consider = |t: f32| {
            if earliest.is_none_or(|old| t < old) {
                earliest = Some(t);
            }
        };
        // A cast that starts inside a collider reports nothing, so a missile
        // swept into geometry would fly out the far side unguided.
        if params
            .world
            .projectile_start_blocked(origin, MISSILE_RADIUS, &params.plates.open_barrier_kinds)
        {
            consider(0.0);
        }
        if let Some(hit) = params.world.cast_moving_ball(origin, translation, MISSILE_RADIUS) {
            consider(hit.t);
        }
        if let Some(hit) = params.world.cast_moving_ball_against_fields(
            origin,
            translation,
            MISSILE_RADIUS,
            &params.plates.open_barrier_kinds,
        ) {
            consider(hit.t);
        }
        for (target, body, yaw, physics) in &bodies {
            if !flight.armed && matches!(target, HitTarget::Player { id, .. } if *id == flight.shooter) {
                continue;
            }
            if let Some(hit) = ball_character_hit(&pos, velocity.0, MISSILE_RADIUS, delta, body, *yaw, *physics) {
                consider(hit.time_of_impact);
            }
        }
        if let Some(t) = earliest {
            let impact = origin + translation * t;
            let hits = missile_blast_hits(
                impact,
                params.blast_radii.missile,
                &params.world,
                &params.plates.open_barrier_kinds,
                bodies.iter().map(|(target, pos, _, physics)| (*target, *pos, *physics)),
            );
            params
                .to_server
                .send(ClientToServer::Send(ClientMessage::MissileDetonated(
                    CMissileDetonated {
                        id: *id,
                        pos: impact.into(),
                        hits,
                    },
                )));
            params.missiles.remove(id);
            commands.entity(entity).despawn();
            spawn_missile_explosion(
                &mut commands,
                &mut ExplosionSpawnCtx {
                    meshes: &mut presentation.meshes,
                    materials: &mut presentation.materials,
                    budget: &mut presentation.budget,
                    explosion_assets: &presentation.explosion_assets,
                    gameplay_config: &params.gameplay,
                    collision_world: Some(&params.world),
                    map_layout: Some(&presentation.map_layout),
                    carriers: &params.carriers,
                    carrier_entities: &presentation.carrier_entities,
                    blast_radii: &params.blast_radii,
                },
                impact.into(),
            );
            play_explosion_sound(
                &mut commands,
                &presentation.asset_server,
                presentation.asset_set.player_sound("explodes"),
                &presentation.client_settings.audio,
                impact,
                Some(params.blast_radii.missile),
            );
        } else {
            *pos += translation;
            if send {
                updates.push(MissileMove {
                    id: *id,
                    seq: owned.seq,
                    movement: MissileMovementState::from_velocity(*pos, velocity.0),
                });
            }
        }
    }
    if !updates.is_empty() {
        params
            .to_server
            .send(ClientToServer::Send(ClientMessage::MissileMoves(CMissileMoves {
                moves: updates,
            })));
    }
}

#[cfg(test)]
#[path = "movement_tests.rs"]
mod tests;
