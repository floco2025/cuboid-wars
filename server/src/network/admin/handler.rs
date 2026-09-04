use bevy::{ecs::system::SystemParam, prelude::*};

use super::execute::{AdminOutcome, run_admin_command};
use crate::{
    actors::{ActorMap, ActorRespawnTimers, PendingActorSpawns},
    combat::PendingExplosions,
    config::ServerGameplayConfig,
    map::{LightState, MapConfig, WeatherState},
    network::{FeedAudience, FeedEvent, emit_feed},
    players::{Invincibility, PlayerInfo, PlayerMap, PlayerStateQuery, UnlimitedMissiles},
    quests::{QuestBoard, QuestCatalog},
};
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindTable, CAdmin, MapSettings, PlayerId},
};

fn admin_authorized(_info: &PlayerInfo) -> bool {
    true
}

// Bundled so the routing system stays under Bevy's system-parameter limit.
#[derive(SystemParam)]
pub struct AdminContext<'w> {
    pub weather: ResMut<'w, WeatherState>,
    pub light: ResMut<'w, LightState>,
    pub pending_explosions: ResMut<'w, PendingExplosions>,
    pub invincibility: ResMut<'w, Invincibility>,
    pub unlimited_missiles: ResMut<'w, UnlimitedMissiles>,
    pub actor_respawn_timers: ResMut<'w, ActorRespawnTimers>,
    pub server_gameplay_config: Res<'w, ServerGameplayConfig>,
    pub map_settings: Res<'w, MapSettings>,
    pub barrier_kind_table: Res<'w, BarrierKindTable>,
    pub quest_catalog: Res<'w, QuestCatalog>,
}

pub fn handle_admin_message(
    commands: &mut Commands,
    players: &mut PlayerMap,
    actors: &ActorMap,
    id: PlayerId,
    admin: &mut AdminContext,
    player_data: &PlayerStateQuery,
    gameplay_config: &GameplayConfig,
    map_config: &MapConfig,
    pending_actor_spawns: &mut PendingActorSpawns,
    quest_board: &mut QuestBoard,
    msg: &CAdmin,
) {
    let Some(info) = players.get(&id) else {
        return;
    };
    let outcome = if admin_authorized(info) {
        run_admin_command(
            commands,
            players,
            actors,
            id,
            admin,
            player_data,
            gameplay_config,
            map_config,
            pending_actor_spawns,
            quest_board,
            &msg.command,
        )
    } else {
        AdminOutcome::Private("not authorized".to_owned())
    };
    let feed = &admin.server_gameplay_config.feed;
    match outcome {
        AdminOutcome::Public(text) if feed.admin_action => emit_feed(
            players,
            feed,
            FeedAudience::Everyone,
            FeedEvent::AdminAction {
                name: players.display_name(&id),
                text,
            },
        ),
        // A disabled public announcement must not hide the outcome from its issuer.
        AdminOutcome::Public(text) | AdminOutcome::Private(text) => {
            emit_feed(players, feed, FeedAudience::Player(id), FeedEvent::AdminReply { text });
        }
    }
}
