use crate::{
    map::WeatherState,
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap},
};
use common::protocol::{CAdmin, PlayerId, SAdminResponse, ServerMessage};

// Anything longer is nonsense or abuse; truncated before parsing.
const MAX_COMMAND_CHARS: usize = 256;

const HELP_TEXT: &str = "commands: help, rain start, rain stop";

// Authorization seam. Deliberately wide open for now — every client is an
// admin; tighten here (role on `PlayerInfo`, config allowlist, …) without
// touching dispatch or the protocol.
fn admin_authorized(_info: &PlayerInfo) -> bool {
    true
}

// Execute one admin command and unicast the outcome text to the sender.
pub fn handle_admin_message(players: &PlayerMap, id: PlayerId, weather: &mut WeatherState, msg: &CAdmin) {
    let Some(info) = players.get(&id) else {
        return;
    };
    let text = if admin_authorized(info) {
        admin_reply(weather, &msg.command)
    } else {
        "not authorized".to_owned()
    };
    let _ = info
        .channel
        .send(ServerToClient::Send(ServerMessage::AdminResponse(SAdminResponse {
            text,
        })));
}

// Parse + execute, returning the reply text. Pure over the weather state so
// the whole command surface is unit-testable without channels.
fn admin_reply(weather: &mut WeatherState, command: &str) -> String {
    let command: String = command.chars().take(MAX_COMMAND_CHARS).collect();
    let command = command.trim();
    let command = command.strip_prefix('/').unwrap_or(command);
    let words: Vec<&str> = command.split_whitespace().collect();

    match words.as_slice() {
        [] | ["help"] => HELP_TEXT.to_owned(),
        ["rain", "start"] => match weather.force_rain_start() {
            Ok(()) => "rain starting".to_owned(),
            Err(reason) => reason.to_owned(),
        },
        ["rain", "stop"] => match weather.force_rain_stop() {
            Ok(()) => "rain stopping".to_owned(),
            Err(reason) => reason.to_owned(),
        },
        _ => format!("unknown command: {command:?} (try: help)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RainScheduleConfig;

    fn weather() -> WeatherState {
        WeatherState::new(Some(RainScheduleConfig {
            auto_start: true,
            auto_end: true,
            min_clear_secs: 10.0,
            max_clear_secs: 20.0,
            min_rain_secs: 5.0,
            max_rain_secs: 8.0,
            ramp_in_secs: 2.0,
            fade_out_secs: 4.0,
        }))
    }

    #[test]
    fn rain_start_and_stop_round_trip() {
        let mut weather = weather();
        assert_eq!(admin_reply(&mut weather, "/rain start"), "rain starting");
        assert_eq!(admin_reply(&mut weather, "rain start"), "already raining");
        assert_eq!(admin_reply(&mut weather, "  rain   stop  "), "rain stopping");
        assert_eq!(admin_reply(&mut weather, "rain stop"), "not raining");
    }

    #[test]
    fn help_and_empty_list_commands() {
        let mut weather = weather();
        assert_eq!(admin_reply(&mut weather, "help"), HELP_TEXT);
        assert_eq!(admin_reply(&mut weather, "/"), HELP_TEXT);
    }

    #[test]
    fn unknown_command_names_itself() {
        let mut weather = weather();
        let reply = admin_reply(&mut weather, "/dance");
        assert!(reply.contains("unknown command"));
        assert!(reply.contains("dance"));
    }

    #[test]
    fn over_long_input_is_truncated_not_panicking() {
        let mut weather = weather();
        let reply = admin_reply(&mut weather, &"x".repeat(10_000));
        assert!(reply.contains("unknown command"));
    }
}
