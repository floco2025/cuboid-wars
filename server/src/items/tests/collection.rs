use crate::config::fixtures;
use crossbeam_channel::unbounded;

use super::*;

fn player() -> PlayerInfo {
    let (tx, _rx) = unbounded();
    PlayerInfo::new(Entity::PLACEHOLDER, tx)
}

#[test]
fn pickups_without_effect_stay_in_the_world() {
    let server_config = fixtures::server_config();
    let config = server_config.gameplay_config();
    let max_health = server_config.combat.health.player.max;
    let mut player = player();

    assert!(!pickup_has_effect(
        ItemType::HealthPotion,
        &player,
        Some(&Health(max_health)),
        &config,
        &server_config
    ));
    assert!(pickup_has_effect(
        ItemType::HealthPotion,
        &player,
        Some(&Health(max_health / 2.0)),
        &config,
        &server_config
    ));

    assert!(pickup_has_effect(
        ItemType::MissilePack,
        &player,
        None,
        &config,
        &server_config
    ));
    player.add_missiles(config.missiles.max_missiles, config.missiles.max_missiles);
    assert!(!pickup_has_effect(
        ItemType::MissilePack,
        &player,
        None,
        &config,
        &server_config
    ));

    assert!(player.add_key(BarrierKindId(0)));
    assert!(!pickup_has_effect(
        ItemType::Key(BarrierKindId(0)),
        &player,
        None,
        &config,
        &server_config
    ));
    assert!(pickup_has_effect(
        ItemType::Key(BarrierKindId(1)),
        &player,
        None,
        &config,
        &server_config
    ));
}

#[test]
fn active_power_ups_are_still_collected_to_reset_their_timer() {
    let server_config = fixtures::server_config();
    let config = server_config.gameplay_config();
    let mut player = player();
    player.grant_power_up(ItemType::SpeedPowerUp, &server_config.maps["hotel"].power_ups);
    assert!(player.has_speed());

    assert!(pickup_has_effect(
        ItemType::SpeedPowerUp,
        &player,
        None,
        &config,
        &server_config
    ));
    assert!(pickup_has_effect(
        ItemType::Gold,
        &player,
        None,
        &config,
        &server_config
    ));
}
