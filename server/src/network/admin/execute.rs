use bevy::prelude::*;

use super::{
    command::{AdminCommand, HELP_TEXT, PlayerTarget, parse_admin_command},
    handler::AdminContext,
};
use crate::{
    actors::{ActorMap, PendingActorSpawns, expire_actor_spawn_cooldowns},
    combat::{DeathSource, kill_player},
    config::ServerGameplayConfig,
    map::MapConfig,
    network::{ServerToClient, broadcast_firework_show},
    players::{PlayerInfo, PlayerMap, PlayerStateQuery},
    quests::{QuestBoard, QuestCatalog, complete_quest, unlock_quest},
};
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindId, Health, ItemType, PlayerId, PowerUpKind, QuestGroupProgress, QuestId, QuestScope},
};

pub(super) enum AdminOutcome {
    Private(String),
    Public(String),
}

pub(super) fn run_admin_command(
    commands: &mut Commands,
    players: &mut PlayerMap,
    actors: &ActorMap,
    sender: PlayerId,
    admin: &mut AdminContext,
    player_data: &PlayerStateQuery,
    gameplay_config: &GameplayConfig,
    map_config: &MapConfig,
    pending_actor_spawns: &mut PendingActorSpawns,
    quest_board: &mut QuestBoard,
    command: &str,
) -> AdminOutcome {
    use AdminOutcome::{Private, Public};

    match parse_admin_command(command) {
        AdminCommand::Help => Private(HELP_TEXT.to_owned()),
        AdminCommand::NotACommand => Private("not a command (commands start with /)".to_owned()),
        AdminCommand::Unknown => Private(format!("unknown command {command:?} (try /help)")),
        AdminCommand::MissingTarget(verb) => Private(if verb == "kill" {
            format!("usage: /{verb} <name> or /{verb} @a")
        } else {
            format!("usage: /{verb} <name>")
        }),
        AdminCommand::WeatherRain => match admin.weather.hold_rain() {
            Ok(()) => Public("weather set to rain".to_owned()),
            Err(reason) => Private(reason.to_owned()),
        },
        AdminCommand::WeatherClear => match admin.weather.hold_clear() {
            Ok(()) => Public("weather set to clear".to_owned()),
            Err(reason) => Private(reason.to_owned()),
        },
        AdminCommand::WeatherAuto => match admin.weather.resume_auto() {
            Ok(()) => Public("weather cycle resumed".to_owned()),
            Err(reason) => Private(reason.to_owned()),
        },
        AdminCommand::WeatherStatus => Private(admin.weather.status()),
        AdminCommand::LightPreset(name) => {
            admin.light.hold_preset(name);
            Public(format!("light set to {name}"))
        }
        AdminCommand::LightFraction(fraction) => {
            admin.light.hold_cycle_fraction(fraction);
            Public(admin.light.status())
        }
        AdminCommand::LightBlend(from, to, blend) => {
            admin.light.hold_blend(from, to, blend);
            Public(admin.light.status())
        }
        AdminCommand::LightAuto => match admin.light.resume_auto() {
            Ok(()) => Public("light cycle resumed".to_owned()),
            Err(reason) => Private(reason.to_owned()),
        },
        AdminCommand::LightStatus => Private(admin.light.status()),
        AdminCommand::LightUsage => {
            Private("usage: /light [bright|dim|dark|auto]\n       /light <0..1>|<from> <to> <0..1>".to_owned())
        }
        AdminCommand::God(explicit) => {
            let enabled = explicit.unwrap_or(!admin.invincibility.0);
            admin.invincibility.0 = enabled;
            admin.unlimited_missiles.0 = enabled;
            Public(format!("god mode {}", if enabled { "on" } else { "off" }))
        }
        AdminCommand::KillAllPlayers => {
            let targets = alive_players(players, None);
            let count = kill_targets(commands, players, admin, player_data, gameplay_config, &targets);
            Public(format!("killed {count} player(s)"))
        }
        AdminCommand::KillPlayer(name) => {
            let targets = alive_players(players, Some(&name));
            if targets.is_empty() {
                return Private(format!("unknown player {name:?}"));
            }
            let count = kill_targets(commands, players, admin, player_data, gameplay_config, &targets);
            Public(format!("killed {count} player(s)"))
        }
        AdminCommand::KillActors(kind) => {
            if let Some(error) = actor_kind_error(kind.as_deref(), &admin.server_gameplay_config) {
                return Private(error);
            }
            let mut count = 0usize;
            for (_, info) in actors.iter() {
                if kind.as_deref().is_none_or(|kind| info.spawn_kind == kind) {
                    commands.entity(info.entity).insert(Health(0.0));
                    count += 1;
                }
            }
            Public(format!("killed {count} actor(s)"))
        }
        AdminCommand::RespawnActors(kind) => {
            if let Some(error) = actor_kind_error(kind.as_deref(), &admin.server_gameplay_config) {
                return Private(error);
            }
            let count = expire_actor_spawn_cooldowns(
                actors,
                pending_actor_spawns,
                &mut admin.actor_spawn_throttles,
                map_config,
                &admin.server_gameplay_config,
                kind.as_deref(),
            );
            Public(format!("respawning {count} actor(s)"))
        }
        AdminCommand::Heal(target) => {
            let targets = match &target {
                PlayerTarget::Sender => alive_players(players, None)
                    .into_iter()
                    .filter(|(id, _)| *id == sender)
                    .collect(),
                PlayerTarget::All => alive_players(players, None),
                PlayerTarget::Named(name) => {
                    let targets = alive_players(players, Some(name));
                    if targets.is_empty() {
                        return Private(format!("unknown player {name:?}"));
                    }
                    targets
                }
            };
            let max_health = admin.server_gameplay_config.combat.health.player.max;
            for (_, entity) in &targets {
                commands.entity(*entity).insert(Health(max_health));
            }
            let text = format!("healed {} player(s)", targets.len());
            if targets.iter().any(|(id, _)| *id != sender) {
                Public(text)
            } else {
                Private(text)
            }
        }
        AdminCommand::GiveKeys => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            let mut added = 0usize;
            for index in 0..admin.barrier_kind_table.len() {
                if let Ok(kind) = u16::try_from(index)
                    && info.add_key(BarrierKindId(kind))
                {
                    added += 1;
                }
            }
            Private(format!("gave {added} key(s)"))
        }
        AdminCommand::GiveKey(color) => match admin.barrier_kind_table.index_of(&color) {
            Some(kind) => {
                let Some(info) = players.get_mut(&sender) else {
                    return Private("sender not found".to_owned());
                };
                Private(if info.add_key(kind) {
                    format!("gave the {color} key")
                } else {
                    format!("already holding the {color} key")
                })
            }
            None => Private(format!(
                "unknown key color {color:?} (colors: {})",
                admin.barrier_kind_table.ids().join(", ")
            )),
        },
        AdminCommand::GivePowerups => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            let mut given = 0usize;
            for kind in PowerUpKind::ALL {
                let id = kind.to_item_type().config_id();
                grant_power_up_by_id(info, id, &admin.server_gameplay_config);
                given += 1;
            }
            Private(format!("gave {given} power-ups"))
        }
        AdminCommand::GivePowerup(power_up) => {
            let power_up_ids = PowerUpKind::ALL.map(|kind| kind.to_item_type().config_id());
            if !power_up_ids.contains(&power_up.as_str()) {
                return Private(format!(
                    "unknown power-up {power_up:?} (power-ups: {})",
                    power_up_ids.join(", ")
                ));
            }
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            grant_power_up_by_id(info, &power_up, &admin.server_gameplay_config);
            Private(format!("gave the {power_up} power-up"))
        }
        AdminCommand::GiveMissiles => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            let max = gameplay_config.missiles.max_missiles;
            // Pickup cues would make an admin grant sound like a world pickup.
            let missiles = info.add_missiles(max, max);
            Private(format!("gave missiles ({missiles}/{max})"))
        }
        AdminCommand::Firework => {
            broadcast_firework_show(players);
            Public("launched fireworks".to_owned())
        }
        AdminCommand::QuestStatus => Private(quest_status(players, quest_board, &admin.quest_catalog, sender)),
        AdminCommand::CompleteQuest(id, target) => {
            let config = &admin.server_gameplay_config;
            let catalog = &admin.quest_catalog;
            let quest_id = QuestId(id.clone());
            let Some(quest) = catalog.get(&quest_id) else {
                let ids: Vec<&str> = catalog.iter().map(|quest| quest.id.0.as_str()).collect();
                return Private(format!("unknown quest {id:?} (quests: {})", ids.join(", ")));
            };
            let title = &quest.title;
            if quest_board.is_completed(&quest_id) {
                return Private(format!("{title} is already completed"));
            }
            let targets = match &target {
                PlayerTarget::Sender => vec![sender],
                PlayerTarget::All => logged_in_players(players, None),
                PlayerTarget::Named(name) => {
                    let targets = logged_in_players(players, Some(name));
                    if targets.is_empty() {
                        return Private(format!("unknown player {name:?}"));
                    }
                    targets
                }
            };
            unlock_quest(players, quest_board, catalog, &quest_id);
            let finished = complete_quest(players, quest_board, catalog, &config.feed, quest, &targets);
            match quest.scope {
                QuestScope::Individual if finished == 0 => {
                    Private(format!("{title} is already completed for those players"))
                }
                QuestScope::Individual => {
                    let text = format!("completed {title} for {finished} player(s)");
                    if targets.iter().any(|id| *id != sender) {
                        Public(text)
                    } else {
                        Private(text)
                    }
                }
                QuestScope::Everyone if !quest_board.is_completed(&quest_id) => {
                    Public(format!("finished {title} for {finished} player(s)"))
                }
                QuestScope::Everyone | QuestScope::Shared => Public(format!("completed {title}")),
            }
        }
        AdminCommand::Kick(name) => {
            let mut count = 0usize;
            for (_, info) in players.iter() {
                if info.connection.logged_in && info.connection.name.to_lowercase() == name.to_lowercase() {
                    let _ = info.connection.channel.send(ServerToClient::Close);
                    count += 1;
                }
            }
            if count == 0 {
                Private(format!("unknown player {name:?}"))
            } else {
                Public(format!("kicked {count} player(s)"))
            }
        }
    }
}

fn alive_players(players: &PlayerMap, name: Option<&str>) -> Vec<(PlayerId, Entity)> {
    players
        .iter()
        .filter(|(_, info)| info.connection.logged_in)
        .filter(|(_, info)| name.is_none_or(|name| info.connection.name.to_lowercase() == name.to_lowercase()))
        .filter_map(|(id, info)| info.entity().map(|entity| (*id, entity)))
        .collect()
}

fn logged_in_players(players: &PlayerMap, name: Option<&str>) -> Vec<PlayerId> {
    players
        .iter()
        .filter(|(_, info)| info.connection.logged_in)
        .filter(|(_, info)| name.is_none_or(|name| info.connection.name.to_lowercase() == name.to_lowercase()))
        .map(|(id, _)| *id)
        .collect()
}

fn quest_status(players: &PlayerMap, board: &QuestBoard, catalog: &QuestCatalog, sender: PlayerId) -> String {
    let statuses = board.group_statuses(catalog, players);
    let own_states = players.get(&sender).map(|info| &info.session.quest_states);
    catalog
        .iter()
        .map(|quest| {
            let scope = match quest.scope {
                QuestScope::Individual => "individual",
                QuestScope::Shared => "shared",
                QuestScope::Everyone => "everyone",
            };
            let own = own_states.and_then(|states| states.get(&quest.id)).map_or_else(
                || "not assigned".to_owned(),
                |state| {
                    state.own_progress().map_or_else(
                        || "assigned".to_owned(),
                        |progress| format!("you {progress}/{}", quest.threshold),
                    )
                },
            );
            let group = statuses
                .iter()
                .find(|status| status.id == quest.id)
                .map(|status| &status.progress);
            let state = if !board.is_unlocked(&quest.id) {
                "locked".to_owned()
            } else if board.is_completed(&quest.id) {
                "completed".to_owned()
            } else {
                match group {
                    Some(QuestGroupProgress::Shared { progress }) => format!("{progress}/{}", quest.threshold),
                    Some(QuestGroupProgress::Everyone {
                        players_done,
                        players_total,
                    }) => format!("{players_done}/{players_total} players done, {own}"),
                    None => own,
                }
            };
            format!("{} ({scope}): {state}", quest.id.0)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn actor_kind_error(kind: Option<&str>, config: &ServerGameplayConfig) -> Option<String> {
    let kind = kind?;
    if config.actors.contains_key(kind) {
        return None;
    }
    let mut kinds: Vec<&str> = config.actors.keys().map(String::as_str).collect();
    kinds.sort_unstable();
    Some(format!("unknown actor kind {kind:?} (kinds: {})", kinds.join(", ")))
}

fn kill_targets(
    commands: &mut Commands,
    players: &mut PlayerMap,
    admin: &mut AdminContext,
    player_data: &PlayerStateQuery,
    gameplay_config: &GameplayConfig,
    targets: &[(PlayerId, Entity)],
) -> usize {
    let mut count = 0usize;
    for (id, entity) in targets {
        let Ok((pos, _, _, _)) = player_data.get(*entity) else {
            continue;
        };
        kill_player(
            commands,
            players,
            *id,
            *entity,
            *pos,
            gameplay_config.player.respawn_secs,
            DeathSource::Admin,
            &admin.server_gameplay_config.feed,
            &mut admin.pending_explosions,
        );
        count += 1;
    }
    count
}

fn grant_power_up_by_id(info: &mut PlayerInfo, id: &str, config: &ServerGameplayConfig) {
    if let Some(item_type) = ItemType::from_config_id(id) {
        info.grant_power_up(item_type, &config.power_ups);
    }
}

#[cfg(test)]
mod tests {
    use common::protocol::BarrierKindTable;

    use super::*;

    #[test]
    fn give_key_and_powerup_mutate_sender_state() {
        use tokio::sync::mpsc::unbounded_channel;

        let (tx, _rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        let table = BarrierKindTable::from_ids(vec!["lobby".to_owned(), "basement".to_owned()])
            .expect("test barrier kind table failed to build");

        let kind = table.index_of("lobby").expect("lobby kind missing from test table");
        assert!(info.add_key(kind));
        assert!(!info.add_key(kind), "second add of the same key must be a no-op");

        let config = ServerGameplayConfig::load_default().expect("default server gameplay config failed to load");
        grant_power_up_by_id(&mut info, "speed", &config);
        assert!(
            info.life.power_up_timers[common::protocol::PowerUpKind::Speed.index()] > 0.0,
            "speed timer must be armed"
        );
    }

    #[test]
    fn god_toggles_and_sets() {
        fn apply(current: bool, explicit: Option<bool>) -> bool {
            explicit.unwrap_or(!current)
        }
        assert!(apply(false, None), "bare god must toggle on");
        assert!(!apply(true, None), "bare god must toggle off");
        assert!(apply(false, Some(true)), "god on must set on");
        assert!(!apply(true, Some(false)), "god off must set off");
    }
}
