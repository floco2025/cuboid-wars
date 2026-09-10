use bevy::prelude::*;
use std::f32::consts::PI;

use super::{super::context::ServerMessageContext, snap_player};
use crate::{
    network::SampleTiming,
    players::{
        LocalPlayerInfo, PlayerInfo, PlayerMap, PlayerSpawnContext, PortalTransitBlend, RemotePlayerMotion,
        spawn_player,
    },
    ui::BannerMessage,
};
use common::{
    map::Carriers,
    protocol::{Player, PlayerGeneration, PlayerId, SPlayerRelocated, sequence_is_newer},
};

pub(in crate::network) fn sync_players(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    tick: u32,
    server_players: &[(PlayerId, Player)],
) {
    for (id, player) in server_players {
        sync_player(commands, context, my_player_id, tick, *id, player);
    }
    for (id, generation, entity) in absent_bodies(&context.players, tick, server_players) {
        context.players.retire_body(id, generation);
        if id == my_player_id {
            commands.entity(entity).insert(Visibility::Hidden);
            context.local_player_info.is_dead = true;
            context.local_player_info.reports.clear_crossings();
        } else {
            commands.entity(entity).despawn();
            context.players.remove(&id);
        }
    }
}

// Bodies the snapshot no longer lists, except those placed after the tick it describes.
fn absent_bodies(
    players: &PlayerMap,
    tick: u32,
    server_players: &[(PlayerId, Player)],
) -> Vec<(PlayerId, PlayerGeneration, Entity)> {
    players
        .iter()
        .filter(|(id, info)| {
            !server_players.iter().any(|(present, _)| present == *id) && !sequence_is_newer(info.spawn_tick, tick)
        })
        .map(|(id, info)| (*id, info.generation, info.entity))
        .collect()
}

pub(in crate::network) fn handle_player_relocated_message(
    message: SPlayerRelocated,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    if context
        .players
        .get(&message.id)
        .is_some_and(|info| info.generation == message.player.generation)
    {
        return;
    }
    sync_player(
        commands,
        context,
        context.my_player_id.0,
        message.tick,
        message.id,
        &message.player,
    );
}

fn sync_player(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    tick: u32,
    id: PlayerId,
    player: &Player,
) {
    if !context.players.accepts_generation(id, player.generation) {
        return;
    }
    let is_local = id == my_player_id;
    let was_dead = is_local && context.local_player_info.is_dead;
    let sample_timing = SampleTiming::new(&context.client_settings.interpolation, &context.network);
    let body_changed = match context.players.get_mut(&id) {
        Some(info) => place_player_body(
            commands,
            info,
            &mut context.local_player_info,
            is_local,
            tick,
            player,
            &context.carriers,
            sample_timing,
        ),
        None => {
            let entity = spawn_player(
                commands,
                PlayerSpawnContext {
                    carriers: &context.carriers,
                    asset_server: &context.assets.asset_server,
                    meshes: &mut context.meshes,
                    materials: &mut context.materials,
                    images: &mut context.images,
                    asset_set: &context.assets.asset_set,
                    client_settings: &context.client_settings,
                    gameplay_config: &context.gameplay_config,
                    max_health: context.assets.max_health.player,
                    sample_timing,
                },
                id,
                player,
                is_local,
            );
            context
                .players
                .insert(id, PlayerInfo::from_snapshot(entity, player, tick));
            if is_local {
                begin_local_body(&mut context.local_player_info, player);
            }
            true
        }
    };
    if is_local {
        *context.portal_access = player.portal_access;
        if body_changed {
            if let Ok(camera) = context.cameras.single() {
                commands.entity(camera).remove::<PortalTransitBlend>();
            }
            if was_dead && let Some(reminder) = context.quest_log.reminder() {
                context.banner.push(BannerMessage::QuestAnnouncement(reminder));
            }
        }
    }
    if let Some(info) = context.players.get_mut(&id) {
        info.apply_snapshot(player);
        commands.entity(info.entity).insert(player.health);
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "placing a body touches the map entry, the local state, and the buffer timing"
)]
fn place_player_body(
    commands: &mut Commands,
    info: &mut PlayerInfo,
    local: &mut LocalPlayerInfo,
    is_local: bool,
    tick: u32,
    player: &Player,
    carriers: &Carriers,
    sample_timing: SampleTiming,
) -> bool {
    if !player.generation.is_newer_than(info.generation) {
        return false;
    }
    info.generation = player.generation;
    info.last_movement_tick = tick;
    info.spawn_tick = tick;
    snap_player(commands, info, &player.movement, carriers);
    commands.entity(info.entity).insert(Visibility::Visible);
    if is_local {
        begin_local_body(local, player);
    } else {
        commands
            .entity(info.entity)
            .insert(RemotePlayerMotion::new(player.movement, sample_timing));
    }
    true
}

fn begin_local_body(local: &mut LocalPlayerInfo, player: &Player) {
    local.stored_yaw = player.movement.face_yaw + PI;
    local.stored_pitch = 0.0;
    local.reports.begin_body(player.generation);
    local.is_dead = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        characters::PreviousTickPosition,
        players::{PlayerAnimationMotion, PlayerMap, interpolate_remote_players_system},
        test_fixtures,
    };
    use bevy::ecs::system::{RunSystemOnce, SystemState};
    use common::{
        physics::{CharacterVerticalVelocity, PlayerMotionBundle},
        protocol::{Health, PlayerGeneration, PlayerMove, PlayerMoveIntent, Position},
    };

    fn timing() -> SampleTiming {
        SampleTiming {
            delay_ticks: 2.0,
            interval_ticks: 1.0,
        }
    }

    fn player() -> Player {
        Player::new(
            "Player".into(),
            Position::default(),
            PlayerMoveIntent::Idle,
            0.0,
            0,
            Health(100.0),
        )
    }

    #[test]
    fn relocation_is_applied_once_even_if_the_snapshot_arrives_first() {
        for (first_tick, later_tick) in [(10, 12), (12, 10)] {
            let mut world = World::new();
            let entity = world.spawn(Position::default()).id();
            let old = player();
            let mut info = PlayerInfo::from_snapshot(entity, &old, 0);
            let mut local = LocalPlayerInfo {
                is_dead: true,
                ..default()
            };
            local.reports.begin_crossing(old.movement.pos);
            let mut relocated = old.clone();
            relocated.generation = PlayerGeneration(1);
            relocated.movement.pos.x = 100.0;
            relocated.movement.face_yaw = 1.0;
            let mut state = SystemState::<Commands>::new(&mut world);
            assert!(place_player_body(
                &mut state.get_mut(&mut world).expect("commands unavailable"),
                &mut info,
                &mut local,
                true,
                first_tick,
                &relocated,
                &Carriers::default(),
                timing(),
            ));
            state.apply(&mut world);
            assert_eq!(
                *world.get::<Position>(entity).expect("position missing"),
                relocated.movement.pos
            );
            assert_eq!(
                world
                    .get::<PreviousTickPosition>(entity)
                    .expect("render anchor missing")
                    .0,
                relocated.movement.pos
            );
            assert!(!local.is_dead);
            assert_eq!(local.stored_yaw, 1.0 + PI);
            world
                .entity_mut(entity)
                .insert((Position { x: 105.0, ..default() }, CharacterVerticalVelocity(7.0)));
            local.stored_yaw = 2.0;
            for update in [&relocated, &old] {
                assert!(!place_player_body(
                    &mut state.get_mut(&mut world).expect("commands unavailable"),
                    &mut info,
                    &mut local,
                    true,
                    later_tick,
                    update,
                    &Carriers::default(),
                    timing(),
                ));
                state.apply(&mut world);
                assert_eq!(world.get::<Position>(entity).expect("position missing").x, 105.0);
                assert_eq!(
                    world
                        .get::<CharacterVerticalVelocity>(entity)
                        .expect("vertical velocity missing")
                        .0,
                    7.0
                );
                assert_eq!(local.stored_yaw, 2.0);
            }
            assert_eq!(info.spawn_tick, first_tick);
        }
    }

    #[test]
    fn relocation_discards_buffered_motion_from_the_previous_remote_body() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Time::<Fixed>::default());
        world.init_resource::<Carriers>();
        world.insert_resource(test_fixtures::map_settings());
        world.init_resource::<PlayerMap>();
        let old = player();
        let mut buffer = RemotePlayerMotion::new(old.movement, timing());
        buffer.push(PlayerMove {
            id: PlayerId(2),
            generation: old.generation,
            seq: 1,
            portal_crossing: 0,
            movement: old.movement,
        });
        let entity = world
            .spawn((
                PlayerId(2),
                old.movement.pos,
                PlayerMotionBundle::from(&old.movement),
                PlayerAnimationMotion::default(),
                buffer,
            ))
            .id();
        let mut info = PlayerInfo::from_snapshot(entity, &old, 1);
        let mut relocated = old.clone();
        relocated.generation = PlayerGeneration(1);
        relocated.movement.pos.x = 100.0;
        let mut state = SystemState::<Commands>::new(&mut world);
        assert!(place_player_body(
            &mut state.get_mut(&mut world).expect("commands missing"),
            &mut info,
            &mut LocalPlayerInfo::default(),
            false,
            2,
            &relocated,
            &Carriers::default(),
            timing(),
        ));
        state.apply(&mut world);
        world
            .run_system_once(interpolate_remote_players_system)
            .expect("remote interpolation failed");
        assert_eq!(world.get::<Position>(entity).expect("position missing").x, 100.0);
    }

    #[test]
    fn a_snapshot_retires_only_bodies_placed_at_or_before_its_tick() {
        let mut players = PlayerMap::default();
        let stale = PlayerId(1);
        let fresh = PlayerId(2);
        let listed = PlayerId(3);
        for (id, spawn_tick) in [(stale, 10), (fresh, 21), (listed, 5)] {
            players.insert(
                id,
                PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player(), spawn_tick),
            );
        }
        let absent = absent_bodies(&players, 20, &[(listed, player())]);
        assert_eq!(absent.len(), 1);
        assert_eq!(absent[0].0, stale);
        assert_eq!(absent[0].1, PlayerGeneration(0));
        assert!(absent_bodies(&players, 21, &[]).iter().any(|(id, _, _)| *id == fresh));
    }
}
