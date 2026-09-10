use bevy::prelude::*;
use std::f32::consts::PI;

use super::{super::context::ServerMessageContext, snap_player};
use crate::{
    players::{LocalPlayerInfo, PlayerInfo, PlayerSpawnContext, PortalTransitBlend, RemotePlayerMotion, spawn_player},
    ui::BannerMessage,
};
use common::{
    map::Carriers,
    protocol::{Player, PlayerId, SPlayerRelocated, sequence_is_newer},
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
    let absent: Vec<_> = context
        .players
        .iter()
        .filter(|(id, info)| {
            !server_players.iter().any(|(present, _)| present == *id) && !sequence_is_newer(info.spawn_tick, tick)
        })
        .map(|(id, info)| (*id, info.generation, info.entity))
        .collect();
    for (id, generation, entity) in absent {
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
    let body_changed = match context.players.get_mut(&id) {
        Some(info) => place_player_body(
            commands,
            info,
            &mut context.local_player_info,
            is_local,
            tick,
            player,
            &context.carriers,
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

fn place_player_body(
    commands: &mut Commands,
    info: &mut PlayerInfo,
    local: &mut LocalPlayerInfo,
    is_local: bool,
    tick: u32,
    player: &Player,
    carriers: &Carriers,
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
            .insert(RemotePlayerMotion::new(player.movement));
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

    #[test]
    fn relocation_is_applied_once_even_if_the_snapshot_arrives_first() {
        for (first_tick, later_tick) in [(10, 12), (12, 10)] {
            let mut world = World::new();
            let entity = world.spawn(Position::default()).id();
            let old = Player::new(
                "Player".into(),
                Position::default(),
                PlayerMoveIntent::Idle,
                0.0,
                0,
                Health(100.0),
            );
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
        let old = Player::new(
            "Player".into(),
            Position::default(),
            PlayerMoveIntent::Idle,
            0.0,
            0,
            Health(100.0),
        );
        let mut buffer = RemotePlayerMotion::default();
        buffer.push(
            PlayerMove {
                id: PlayerId(2),
                generation: old.generation,
                seq: 1,
                portal_crossing: 0,
                movement: old.movement,
            },
            0.0,
        );
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
        ));
        state.apply(&mut world);
        world
            .run_system_once(interpolate_remote_players_system)
            .expect("remote interpolation failed");
        assert_eq!(world.get::<Position>(entity).expect("position missing").x, 100.0);
    }
}
