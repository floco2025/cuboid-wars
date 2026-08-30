use crate::map::light_preset_from_str;
use common::constants::COMMAND_MAX_CHARS;

pub(super) const HELP_TEXT: &str = "/help\n/weather [rain|clear|auto]\n/light [bright|dim|dark|auto]\n/light <0..1>|<from> <to> <0..1>\n/god [on|off]\n/kill <name>|@a\n/killall [kind]\n/respawn [kind]\n/heal [name|@a]\n/give keys|key <color>\n/give powerups|powerup <type>\n/give missiles\n/firework\n/quest\n/quest <id> [name|@a]\n/kick <name>";

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AdminCommand {
    Help,
    WeatherRain,
    WeatherClear,
    WeatherAuto,
    WeatherStatus,
    LightPreset(&'static str),
    LightFraction(f32),
    LightBlend(&'static str, &'static str, f32),
    LightAuto,
    LightStatus,
    LightUsage,
    God(Option<bool>),
    KillAllPlayers,
    KillPlayer(String),
    KillActors(Option<String>),
    RespawnActors(Option<String>),
    Heal(PlayerTarget),
    GiveKeys,
    GiveKey(String),
    GivePowerups,
    GivePowerup(String),
    GiveMissiles,
    Firework,
    QuestStatus,
    CompleteQuest(String, PlayerTarget),
    Kick(String),
    MissingTarget(&'static str),
    NotACommand,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PlayerTarget {
    Sender,
    All,
    Named(String),
}

fn parse_unit_fraction(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))
}

pub(super) fn parse_admin_command(input: &str) -> AdminCommand {
    let input: String = input.chars().take(COMMAND_MAX_CHARS).collect();
    let input = input.trim();
    let Some(command) = input.strip_prefix('/') else {
        return AdminCommand::NotACommand;
    };
    let words: Vec<&str> = command.split_whitespace().collect();

    match words.as_slice() {
        [] | ["help"] => AdminCommand::Help,
        ["weather"] => AdminCommand::WeatherStatus,
        ["weather", "rain"] => AdminCommand::WeatherRain,
        ["weather", "clear"] => AdminCommand::WeatherClear,
        ["weather", "auto"] => AdminCommand::WeatherAuto,
        ["light"] => AdminCommand::LightStatus,
        ["light", "auto"] => AdminCommand::LightAuto,
        ["light", value] => match (light_preset_from_str(value), parse_unit_fraction(value)) {
            (Some(preset), _) => AdminCommand::LightPreset(preset),
            (None, Some(fraction)) => AdminCommand::LightFraction(fraction),
            (None, None) => AdminCommand::LightUsage,
        },
        ["light", from, to, value] => {
            match (
                light_preset_from_str(from),
                light_preset_from_str(to),
                parse_unit_fraction(value),
            ) {
                (Some(from), Some(to), Some(blend)) => AdminCommand::LightBlend(from, to, blend),
                _ => AdminCommand::LightUsage,
            }
        }
        ["light", ..] => AdminCommand::LightUsage,
        ["god"] => AdminCommand::God(None),
        ["god", "on"] => AdminCommand::God(Some(true)),
        ["god", "off"] => AdminCommand::God(Some(false)),
        ["kill"] => AdminCommand::MissingTarget("kill"),
        ["kill", "@a"] => AdminCommand::KillAllPlayers,
        ["kill", name @ ..] => AdminCommand::KillPlayer(name.join(" ")),
        ["killall"] => AdminCommand::KillActors(None),
        ["killall", kind] => AdminCommand::KillActors(Some((*kind).to_owned())),
        ["respawn"] => AdminCommand::RespawnActors(None),
        ["respawn", kind] => AdminCommand::RespawnActors(Some((*kind).to_owned())),
        ["heal"] => AdminCommand::Heal(PlayerTarget::Sender),
        ["heal", "@a"] => AdminCommand::Heal(PlayerTarget::All),
        ["heal", name @ ..] => AdminCommand::Heal(PlayerTarget::Named(name.join(" "))),
        ["give", "keys"] => AdminCommand::GiveKeys,
        ["give", "key", color] => AdminCommand::GiveKey((*color).to_owned()),
        ["give", "powerups"] => AdminCommand::GivePowerups,
        ["give", "powerup", power_up] => AdminCommand::GivePowerup((*power_up).to_owned()),
        ["give", "missiles"] => AdminCommand::GiveMissiles,
        ["firework"] => AdminCommand::Firework,
        ["quest"] => AdminCommand::QuestStatus,
        ["quest", id] => AdminCommand::CompleteQuest((*id).to_owned(), PlayerTarget::Sender),
        ["quest", id, "@a"] => AdminCommand::CompleteQuest((*id).to_owned(), PlayerTarget::All),
        ["quest", id, name @ ..] => AdminCommand::CompleteQuest((*id).to_owned(), PlayerTarget::Named(name.join(" "))),
        ["kick"] => AdminCommand::MissingTarget("kick"),
        ["kick", name @ ..] => AdminCommand::Kick(name.join(" ")),
        _ => AdminCommand::Unknown,
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
        assert_eq!(parse_admin_command("/weather auto"), AdminCommand::WeatherAuto);
        assert_eq!(parse_admin_command("/weather"), AdminCommand::WeatherStatus);
        assert_eq!(
            parse_admin_command("/light bright"),
            AdminCommand::LightPreset("bright")
        );
        assert_eq!(parse_admin_command("/light dim"), AdminCommand::LightPreset("dim"));
        assert_eq!(parse_admin_command("/light dark"), AdminCommand::LightPreset("dark"));
        assert_eq!(parse_admin_command("/light auto"), AdminCommand::LightAuto);
        assert_eq!(parse_admin_command("/light"), AdminCommand::LightStatus);
        assert_eq!(parse_admin_command("/light 0.7"), AdminCommand::LightFraction(0.7));
        assert_eq!(
            parse_admin_command("/light dim dark 0.3"),
            AdminCommand::LightBlend("dim", "dark", 0.3)
        );
        assert_eq!(parse_admin_command("/light 1.5"), AdminCommand::LightUsage);
        assert_eq!(parse_admin_command("/light banana"), AdminCommand::LightUsage);
        assert_eq!(parse_admin_command("/light dim banana 0.3"), AdminCommand::LightUsage);
        assert_eq!(parse_admin_command("/light dim dark"), AdminCommand::LightUsage);
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
        assert_eq!(parse_admin_command("/respawn"), AdminCommand::RespawnActors(None));
        assert_eq!(
            parse_admin_command("/respawn sentry"),
            AdminCommand::RespawnActors(Some("sentry".to_owned()))
        );
        assert_eq!(parse_admin_command("/heal"), AdminCommand::Heal(PlayerTarget::Sender));
        assert_eq!(parse_admin_command("/heal @a"), AdminCommand::Heal(PlayerTarget::All));
        assert_eq!(
            parse_admin_command("/heal Bob"),
            AdminCommand::Heal(PlayerTarget::Named("Bob".to_owned()))
        );
        assert_eq!(parse_admin_command("/give keys"), AdminCommand::GiveKeys);
        assert_eq!(
            parse_admin_command("/give key lobby"),
            AdminCommand::GiveKey("lobby".to_owned())
        );
        assert_eq!(parse_admin_command("/give powerups"), AdminCommand::GivePowerups);
        assert_eq!(parse_admin_command("/give missiles"), AdminCommand::GiveMissiles);
        assert_eq!(
            parse_admin_command("/give powerup speed"),
            AdminCommand::GivePowerup("speed".to_owned())
        );
        assert_eq!(parse_admin_command("/quest"), AdminCommand::QuestStatus);
        assert_eq!(
            parse_admin_command("/quest collect_gold"),
            AdminCommand::CompleteQuest("collect_gold".to_owned(), PlayerTarget::Sender)
        );
        assert_eq!(
            parse_admin_command("/quest collect_gold @a"),
            AdminCommand::CompleteQuest("collect_gold".to_owned(), PlayerTarget::All)
        );
        assert_eq!(
            parse_admin_command("/quest collect_gold Bob the Great"),
            AdminCommand::CompleteQuest(
                "collect_gold".to_owned(),
                PlayerTarget::Named("Bob the Great".to_owned())
            )
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
}
