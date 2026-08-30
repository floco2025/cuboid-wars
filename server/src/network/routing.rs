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
    items::ItemMap,
    map::OpenBarrierKinds,
    missiles::{MissileMap, handle_missile_shot_message},
    network::ServerToClient,
    players::PlayerMap,
    projectiles::handle_shot_message,
    quests::{QuestBoard, QuestCatalog},
};
use common::protocol::{ItemMarker, *};

#[derive(SystemParam)]
pub(super) struct ClientMessageContext<'w, 's> {
    pub(super) players: ResMut<'w, PlayerMap>,
    pub(super) time: Res<'w, Time>,
    pub(super) world: SharedWorld<'w>,
    pub(super) queries: CharacterQueries<'w, 's>,
    pub(super) items: Res<'w, ItemMap>,
    pub(super) actors: Res<'w, ActorMap>,
    pub(super) item_data: Query<'w, 's, &'static Position, With<ItemMarker>>,
    pub(super) open_barrier_kinds: Res<'w, OpenBarrierKinds>,
    pub(super) missiles: ResMut<'w, MissileMap>,
    pub(super) pending_actor_spawns: ResMut<'w, PendingActorSpawns>,
    pub(super) admin: AdminContext<'w>,
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
    let is_dead = player.is_dead();
    let entity = player.entity();

    // Dead players retain console and diagnostic access through respawn.
    if is_dead
        && !matches!(
            &message,
            ClientMessage::Ping(_) | ClientMessage::Admin(_) | ClientMessage::Chat(_)
        )
    {
        return;
    }

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
                &context.items,
                &context.actors,
                &context.pending_actor_spawns,
                &context.queries,
                &context.item_data,
                context.admin.weather.intensity(),
                context.admin.light.blend(),
            );
        }
        ClientMessage::Login(_) => {
            warn!(
                "{} sent login after already authenticated",
                context.players.describe(&id)
            );
            if let Some(player) = context.players.get(&id) {
                let _ = player.connection.channel.send(ServerToClient::Close);
            }
        }
        _ if !logged_in => {
            warn!("{:?} sent non-login message before authenticating", id);
        }
        ClientMessage::Move(message) => {
            let Some(entity) = entity else {
                return;
            };
            handle_move_message(commands, entity, id, message, &context.players, &context.queries);
        }
        ClientMessage::Jump(message) => {
            let Some(entity) = entity else {
                return;
            };
            handle_jump_message(
                commands,
                entity,
                id,
                message,
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
        ClientMessage::Ping(message) => handle_ping_message(id, message, &context.players),
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
