use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::ActorMap,
    combat::{PendingExplosions, kill_player},
    config::ServerGameplayConfig,
    map::WeatherState,
    network::ServerToClient,
    players::{Invincibility, PlayerInfo, PlayerMap, UnlimitedMissiles},
};
use common::{
    config::GameplayConfig,
    protocol::{
        BarrierKindTable, CAdmin, FaceDirection, Health, ItemType, PlayerId, PlayerMarker, PlayerMoveIntent, Position,
        SAdminResponse, ServerMessage,
    },
};

// Anything longer is nonsense or abuse; truncated before parsing.
const MAX_COMMAND_CHARS: usize = 256;

// One command per line: the client feed renders each line as its own row.
// Must fit within `hud.message_feed.max_entries` or the top lines evict.
const HELP_TEXT: &str = "/help\n/weather rain|clear\n/god [on|off]\n/kill <name>|@a\n/killall [kind]\n/heal [name|@a]\n/give keys|key <color>\n/give powerups|powerup <type>\n/give missiles\n/kick <name>";

// The four timer power-up config ids, in `PowerUpKind` order.
const POWER_UP_IDS: [&str; 4] = ["speed", "multi_shot", "phasing", "low_gravity"];

// Authorization seam. Deliberately wide open for now — every client is an
// admin; tighten here (role on `PlayerInfo`, config allowlist, …) without
// touching dispatch or the protocol.
fn admin_authorized(_info: &PlayerInfo) -> bool {
    true
}

// Everything admin commands may mutate or inspect, bundled so the message
// dispatch system stays under Bevy's parameter limit.
#[derive(SystemParam)]
pub struct AdminContext<'w> {
    pub weather: ResMut<'w, WeatherState>,
    pub pending_explosions: ResMut<'w, PendingExplosions>,
    pub invincibility: ResMut<'w, Invincibility>,
    pub unlimited_missiles: ResMut<'w, UnlimitedMissiles>,
    pub actors: Res<'w, ActorMap>,
    pub server_gameplay_config: Res<'w, ServerGameplayConfig>,
    pub barrier_kind_table: Res<'w, BarrierKindTable>,
}

// Vanilla-Minecraft-style command grammar: `/verb [argument]`, `@a` = all
// players, missing slash = not a command (the line stays chat-compatible).
#[derive(Debug, Clone, PartialEq, Eq)]
enum AdminCommand {
    Help,
    WeatherRain,
    WeatherClear,
    God(Option<bool>),
    KillAllPlayers,
    KillPlayer(String),
    KillActors(Option<String>),
    Heal(HealTarget),
    GiveKeys,
    GiveKey(String),
    GivePowerups,
    GivePowerup(String),
    GiveMissiles,
    Kick(String),
    MissingTarget(&'static str),
    NotACommand,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HealTarget {
    Sender,
    All,
    Named(String),
}

fn parse_admin_command(input: &str) -> AdminCommand {
    let input: String = input.chars().take(MAX_COMMAND_CHARS).collect();
    let input = input.trim();
    let Some(command) = input.strip_prefix('/') else {
        return AdminCommand::NotACommand;
    };
    let words: Vec<&str> = command.split_whitespace().collect();

    match words.as_slice() {
        [] | ["help"] => AdminCommand::Help,
        ["weather", "rain"] => AdminCommand::WeatherRain,
        ["weather", "clear"] => AdminCommand::WeatherClear,
        ["god"] => AdminCommand::God(None),
        ["god", "on"] => AdminCommand::God(Some(true)),
        ["god", "off"] => AdminCommand::God(Some(false)),
        ["kill"] => AdminCommand::MissingTarget("kill"),
        ["kill", "@a"] => AdminCommand::KillAllPlayers,
        ["kill", name @ ..] => AdminCommand::KillPlayer(name.join(" ")),
        ["killall"] => AdminCommand::KillActors(None),
        ["killall", kind] => AdminCommand::KillActors(Some((*kind).to_owned())),
        ["heal"] => AdminCommand::Heal(HealTarget::Sender),
        ["heal", "@a"] => AdminCommand::Heal(HealTarget::All),
        ["heal", name @ ..] => AdminCommand::Heal(HealTarget::Named(name.join(" "))),
        ["give", "keys"] => AdminCommand::GiveKeys,
        ["give", "key", color] => AdminCommand::GiveKey((*color).to_owned()),
        ["give", "powerups"] => AdminCommand::GivePowerups,
        ["give", "powerup", power_up] => AdminCommand::GivePowerup((*power_up).to_owned()),
        ["give", "missiles"] => AdminCommand::GiveMissiles,
        ["kick"] => AdminCommand::MissingTarget("kick"),
        ["kick", name @ ..] => AdminCommand::Kick(name.join(" ")),
        _ => AdminCommand::Unknown,
    }
}

// Execute one admin command and unicast the outcome text to the sender.
pub fn handle_admin_message(
    commands: &mut Commands,
    players: &mut PlayerMap,
    id: PlayerId,
    admin: &mut AdminContext,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    gameplay_config: &GameplayConfig,
    msg: &CAdmin,
) {
    let Some(info) = players.get(&id) else {
        return;
    };
    let text = if admin_authorized(info) {
        run_admin_command(commands, players, id, admin, player_data, gameplay_config, &msg.command)
    } else {
        "not authorized".to_owned()
    };
    if let Some(info) = players.get(&id) {
        let _ = info
            .channel
            .send(ServerToClient::Send(ServerMessage::AdminResponse(SAdminResponse {
                text,
            })));
    }
}

fn run_admin_command(
    commands: &mut Commands,
    players: &mut PlayerMap,
    sender: PlayerId,
    admin: &mut AdminContext,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    gameplay_config: &GameplayConfig,
    command: &str,
) -> String {
    match parse_admin_command(command) {
        AdminCommand::Help => HELP_TEXT.to_owned(),
        AdminCommand::NotACommand => "not a command (commands start with /)".to_owned(),
        AdminCommand::Unknown => format!("unknown command {command:?} (try /help)"),
        AdminCommand::MissingTarget(verb) => {
            if verb == "kill" {
                format!("usage: /{verb} <name> or /{verb} @a")
            } else {
                format!("usage: /{verb} <name>")
            }
        }
        AdminCommand::WeatherRain => match admin.weather.force_rain_start() {
            Ok(()) => "weather set to rain".to_owned(),
            Err(reason) => reason.to_owned(),
        },
        AdminCommand::WeatherClear => match admin.weather.force_rain_stop() {
            Ok(()) => "weather set to clear".to_owned(),
            Err(reason) => reason.to_owned(),
        },
        AdminCommand::God(explicit) => {
            let enabled = explicit.unwrap_or(!admin.invincibility.0);
            admin.invincibility.0 = enabled;
            admin.unlimited_missiles.0 = enabled;
            format!("god mode {}", if enabled { "on" } else { "off" })
        }
        AdminCommand::KillAllPlayers => {
            let targets = alive_players(players, None);
            let count = kill_targets(commands, players, admin, player_data, gameplay_config, &targets);
            format!("killed {count} player(s)")
        }
        AdminCommand::KillPlayer(name) => {
            let targets = alive_players(players, Some(&name));
            if targets.is_empty() {
                return format!("unknown player {name:?}");
            }
            let count = kill_targets(commands, players, admin, player_data, gameplay_config, &targets);
            format!("killed {count} player(s)")
        }
        AdminCommand::KillActors(kind) => {
            if let Some(kind) = &kind
                && !admin.server_gameplay_config.actors.contains_key(kind)
            {
                let mut kinds: Vec<&str> = admin.server_gameplay_config.actors.keys().map(String::as_str).collect();
                kinds.sort_unstable();
                return format!("unknown actor kind {kind:?} (kinds: {})", kinds.join(", "));
            }
            let mut count = 0usize;
            for (_, info) in admin.actors.iter() {
                if kind.as_deref().is_none_or(|kind| info.spawn_kind == kind) {
                    commands.entity(info.entity).insert(Health(0.0));
                    count += 1;
                }
            }
            format!("killed {count} actor(s)")
        }
        AdminCommand::Heal(target) => {
            let targets = match &target {
                HealTarget::Sender => alive_players(players, None)
                    .into_iter()
                    .filter(|(id, _)| *id == sender)
                    .collect(),
                HealTarget::All => alive_players(players, None),
                HealTarget::Named(name) => {
                    let targets = alive_players(players, Some(name));
                    if targets.is_empty() {
                        return format!("unknown player {name:?}");
                    }
                    targets
                }
            };
            let max_health = gameplay_config.player.health().max;
            for (_, entity) in &targets {
                commands.entity(*entity).insert(Health(max_health));
            }
            format!("healed {} player(s)", targets.len())
        }
        AdminCommand::GiveKeys => {
            let Some(info) = players.get_mut(&sender) else {
                return "sender not found".to_owned();
            };
            let mut added = 0usize;
            for index in 0..admin.barrier_kind_table.len() {
                if let Ok(kind) = u16::try_from(index)
                    && info.add_key(common::protocol::BarrierKindId(kind))
                {
                    added += 1;
                }
            }
            format!("gave {added} key(s)")
        }
        AdminCommand::GiveKey(color) => match admin.barrier_kind_table.index_of(&color) {
            Some(kind) => {
                let Some(info) = players.get_mut(&sender) else {
                    return "sender not found".to_owned();
                };
                if info.add_key(kind) {
                    format!("gave the {color} key")
                } else {
                    format!("already holding the {color} key")
                }
            }
            None => format!(
                "unknown key color {color:?} (colors: {})",
                admin.barrier_kind_table.ids().join(", ")
            ),
        },
        AdminCommand::GivePowerups => {
            let Some(info) = players.get_mut(&sender) else {
                return "sender not found".to_owned();
            };
            // Phasing is defunct (not game-tested): excluded from the bulk
            // give, granted only via the explicit `/give powerup phasing`.
            let ids = POWER_UP_IDS.iter().filter(|id| **id != "phasing");
            let mut given = 0usize;
            for id in ids {
                grant_power_up_by_id(info, id, &admin.server_gameplay_config);
                given += 1;
            }
            format!("gave {given} power-ups")
        }
        AdminCommand::GivePowerup(power_up) => {
            if !POWER_UP_IDS.contains(&power_up.as_str()) {
                return format!("unknown power-up {power_up:?} (power-ups: {})", POWER_UP_IDS.join(", "));
            }
            let Some(info) = players.get_mut(&sender) else {
                return "sender not found".to_owned();
            };
            grant_power_up_by_id(info, &power_up, &admin.server_gameplay_config);
            format!("gave the {power_up} power-up")
        }
        AdminCommand::GiveMissiles => {
            let Some(info) = players.get_mut(&sender) else {
                return "sender not found".to_owned();
            };
            let max = gameplay_config.missiles.max_missiles;
            // No `SMissilesCollected` cue — that would play the pickup sound
            // on the client (admin gives are silent, like keys/power-ups);
            // the next snapshot updates the HUD.
            let missiles = info.add_missiles(max, max);
            format!("gave missiles ({missiles}/{max})")
        }
        AdminCommand::Kick(name) => {
            let mut count = 0usize;
            for (_, info) in players.iter() {
                if info.logged_in && info.name.to_lowercase() == name.to_lowercase() {
                    let _ = info.channel.send(ServerToClient::Close);
                    count += 1;
                }
            }
            if count == 0 {
                format!("unknown player {name:?}")
            } else {
                format!("kicked {count} player(s)")
            }
        }
    }
}

// Logged-in, alive players — all of them, or only those matching `name`
// (case-insensitive; names aren't unique, so several can match).
fn alive_players(players: &PlayerMap, name: Option<&str>) -> Vec<(PlayerId, Entity)> {
    players
        .iter()
        .filter(|(_, info)| info.logged_in && !info.is_dead())
        .filter(|(_, info)| name.is_none_or(|name| info.name.to_lowercase() == name.to_lowercase()))
        .map(|(id, info)| (*id, info.entity))
        .collect()
}

fn kill_targets(
    commands: &mut Commands,
    players: &mut PlayerMap,
    admin: &mut AdminContext,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
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
            gameplay_config.player.respawn_delay_secs,
            None,
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
    use super::*;

    #[test]
    fn parses_every_command_form() {
        assert_eq!(parse_admin_command("/help"), AdminCommand::Help);
        assert_eq!(parse_admin_command("  /  "), AdminCommand::Help);
        assert_eq!(parse_admin_command("/weather rain"), AdminCommand::WeatherRain);
        assert_eq!(parse_admin_command("/weather clear"), AdminCommand::WeatherClear);
        assert_eq!(parse_admin_command("/god"), AdminCommand::God(None));
        assert_eq!(parse_admin_command("/god on"), AdminCommand::God(Some(true)));
        assert_eq!(parse_admin_command("/god off"), AdminCommand::God(Some(false)));
        assert_eq!(parse_admin_command("/kill @a"), AdminCommand::KillAllPlayers);
        assert_eq!(
            parse_admin_command("/kill Bob the Great"),
            AdminCommand::KillPlayer("Bob the Great".to_owned())
        );
        assert_eq!(parse_admin_command("/kill"), AdminCommand::MissingTarget("kill"));
        assert_eq!(parse_admin_command("/killall"), AdminCommand::KillActors(None));
        assert_eq!(
            parse_admin_command("/killall zapper"),
            AdminCommand::KillActors(Some("zapper".to_owned()))
        );
        assert_eq!(parse_admin_command("/heal"), AdminCommand::Heal(HealTarget::Sender));
        assert_eq!(parse_admin_command("/heal @a"), AdminCommand::Heal(HealTarget::All));
        assert_eq!(
            parse_admin_command("/heal Bob"),
            AdminCommand::Heal(HealTarget::Named("Bob".to_owned()))
        );
        assert_eq!(parse_admin_command("/give keys"), AdminCommand::GiveKeys);
        assert_eq!(
            parse_admin_command("/give key lobby"),
            AdminCommand::GiveKey("lobby".to_owned())
        );
        assert_eq!(parse_admin_command("/give powerups"), AdminCommand::GivePowerups);
        assert_eq!(parse_admin_command("/give missiles"), AdminCommand::GiveMissiles);
        assert_eq!(
            parse_admin_command("/give powerup phasing"),
            AdminCommand::GivePowerup("phasing".to_owned())
        );
        assert_eq!(parse_admin_command("/kick Bob"), AdminCommand::Kick("Bob".to_owned()));
        assert_eq!(parse_admin_command("/kick"), AdminCommand::MissingTarget("kick"));
    }

    #[test]
    fn slashless_input_is_not_a_command() {
        assert_eq!(parse_admin_command("hello there"), AdminCommand::NotACommand);
        assert_eq!(parse_admin_command(""), AdminCommand::NotACommand);
        assert_eq!(parse_admin_command("kill @a"), AdminCommand::NotACommand);
    }

    #[test]
    fn unknown_and_overlong_input_parse_safely() {
        assert_eq!(parse_admin_command("/dance"), AdminCommand::Unknown);
        assert_eq!(parse_admin_command("/give"), AdminCommand::Unknown);
        assert_eq!(
            parse_admin_command(&format!("/{}", "x".repeat(10_000))),
            AdminCommand::Unknown
        );
    }

    #[test]
    fn give_key_and_powerup_mutate_sender_state() {
        use tokio::sync::mpsc::unbounded_channel;

        let (tx, _rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        let table = BarrierKindTable::from_ids(vec!["lobby".to_owned(), "basement".to_owned()])
            .expect("test barrier kind table should build");

        let kind = table.index_of("lobby").expect("lobby kind missing from test table");
        assert!(info.add_key(kind));
        assert!(!info.add_key(kind), "second add of the same key must be a no-op");

        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        grant_power_up_by_id(&mut info, "phasing", &config);
        assert!(
            info.power_up_timers[common::protocol::PowerUpKind::Phasing.index()] > 0.0,
            "phasing timer must be armed"
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
