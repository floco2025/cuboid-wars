use bevy::{ecs::system::SystemParam, prelude::*};

use super::{
    admin::{AdminContext, handle_admin_message},
    handlers::{
        CharacterQueries, SharedWorld, handle_chat_message, handle_jump_message, handle_move_message,
        handle_ping_message,
    },
    login::handle_login_message,
};
use crate::{
    actors::{ActorMap, PendingActorSpawns},
    map::OpenBarrierKinds,
    missiles::{MissileMap, handle_missile_shot_message},
    network::ServerToClient,
    players::PlayerMap,
    portals::{PortalMap, handle_portal_shot_message},
    projectiles::handle_shot_message,
    quests::{QuestBoard, QuestCatalog},
};
use common::physics::PortalSet;
use common::protocol::*;

#[derive(SystemParam)]
pub(super) struct ClientMessageContext<'w, 's> {
    pub(super) players: ResMut<'w, PlayerMap>,
    time: Res<'w, Time>,
    pub(super) world: SharedWorld<'w>,
    queries: CharacterQueries<'w, 's>,
    actors: Res<'w, ActorMap>,
    open_barrier_kinds: Res<'w, OpenBarrierKinds>,
    missiles: ResMut<'w, MissileMap>,
    pending_actor_spawns: ResMut<'w, PendingActorSpawns>,
    pub(super) portals: ResMut<'w, PortalMap>,
    pub(super) portal_set: ResMut<'w, PortalSet>,
    admin: AdminContext<'w>,
    pub(super) quest_board: ResMut<'w, QuestBoard>,
    pub(super) quest_catalog: Res<'w, QuestCatalog>,
}

pub(super) fn route_client_message(
    commands: &mut Commands,
    id: PlayerId,
    message: ClientMessage,
    context: &mut ClientMessageContext,
) {
    let Some(player) = context.players.get(&id) else {
        error!("received message for unknown player#{}", id.0);
        return;
    };
    let logged_in = player.connection.logged_in;
    // `None` while dead: the body-bound arms drop the message until respawn,
    // while Ping/Admin/Chat keep the console and RTT working meanwhile.
    let entity = player.entity();

    match message {
        ClientMessage::Login(message) if !logged_in => {
            let Some(entity) = entity else {
                error!("player#{} reached login without an entity", id.0);
                return;
            };
            handle_login_message(
                commands,
                entity,
                id,
                message,
                &mut context.players,
                &context.world,
                &context.quest_catalog,
                &context.quest_board,
                &context.queries,
            );
        }
        ClientMessage::Login(_) => {
            warn!(
                "{} sent login after already authenticated",
                context.players.describe(&id)
            );
            // Close to enforce a single-login flow.
            if let Some(player) = context.players.get(&id) {
                let _ = player.connection.channel.send(ServerToClient::Close);
            }
        }
        _ if !logged_in => {
            warn!(
                "{:?} sent non-login message before authenticating (likely out-of-order delivery)",
                id
            );
            // Keep the connection: the Login is likely still in flight.
        }
        ClientMessage::Move(message) => {
            let Some(entity) = entity else {
                return;
            };
            trace!("{:?} input: {:?}", id, message);
            handle_move_message(commands, entity, id, message, &context.players, &context.queries);
        }
        ClientMessage::Jump(_) => {
            let Some(entity) = entity else {
                return;
            };
            trace!("{:?} jump", id);
            handle_jump_message(
                commands,
                entity,
                id,
                &context.players,
                &context.queries,
                &context.world.collision_world,
                &context.world.gameplay_config,
            );
        }
        ClientMessage::Shot(message) => {
            let Some(entity) = entity else {
                return;
            };
            debug!("{} shot", context.players.describe(&id));
            handle_shot_message(
                commands,
                entity,
                id,
                &message,
                &mut context.players,
                &context.time,
                &context.queries.player_data,
                &context.world.collision_world,
                &context.world.gameplay_config,
                &context.open_barrier_kinds,
            );
        }
        ClientMessage::MissileShot(message) => {
            let Some(entity) = entity else {
                return;
            };
            debug!("{} missile shot at {:?}", context.players.describe(&id), message.target);
            handle_missile_shot_message(
                commands,
                entity,
                id,
                &message,
                &mut context.players,
                &mut context.missiles,
                &context.queries.player_data,
                &context.actors,
                &context.queries.actor_data,
                &context.world.collision_world,
                &context.world.gameplay_config,
                &context.world.server_gameplay_config,
                &context.open_barrier_kinds,
            );
        }
        ClientMessage::PortalShot(message) => {
            let Some(entity) = entity else {
                return;
            };
            debug!("{} portal shot ({:?})", context.players.describe(&id), message.end);
            handle_portal_shot_message(
                entity,
                id,
                &message,
                &context.players,
                &context.queries.player_data,
                &context.world.collision_world,
                &context.world.gameplay_config,
                &context.world.server_gameplay_config,
                &mut context.portals,
                &mut context.portal_set,
            );
        }
        ClientMessage::Ping(message) => {
            trace!("{:?} ping: {:?}", id, message);
            handle_ping_message(id, message, &context.players);
        }
        ClientMessage::Admin(message) => {
            debug!("{} admin command: {:?}", context.players.describe(&id), message.command);
            handle_admin_message(
                commands,
                &mut context.players,
                &context.actors,
                id,
                &mut context.admin,
                &context.queries.player_data,
                &context.world.gameplay_config,
                &context.world.map_config,
                &mut context.pending_actor_spawns,
                &mut context.quest_board,
                &message,
            );
        }
        ClientMessage::Chat(message) => handle_chat_message(
            id,
            &message,
            &context.players,
            &context.world.server_gameplay_config.feed,
        ),
    }
}
