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
    assert_eq!(parse_admin_command("/peace"), AdminCommand::Peace(None));
    assert_eq!(parse_admin_command("/peace on"), AdminCommand::Peace(Some(true)));
    assert_eq!(parse_admin_command("/peace off"), AdminCommand::Peace(Some(false)));
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
        parse_admin_command("/respawn bruiser"),
        AdminCommand::RespawnActors(Some("bruiser".to_owned()))
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
