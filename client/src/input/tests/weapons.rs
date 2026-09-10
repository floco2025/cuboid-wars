use super::*;
use crate::{players::PlayerInfo, test_fixtures};
use common::protocol::{
    Health, Player, PlayerGeneration, PlayerId, PlayerMoveIntent, PlayerMovementState, PortalPairId, Position,
};

const BOTH: PortalAccess = PortalAccess::Both { pair: PortalPairId(1) };

fn modes(loadout: WeaponLoadout) -> Vec<WeaponMode> {
    loadout.modes().collect()
}

#[test]
fn loadout_offers_each_collected_weapon_in_cycle_order() {
    assert_eq!(
        modes(WeaponLoadout::new(true, BOTH, 0, 1, true)),
        [WeaponMode::Projectile, WeaponMode::Missile, WeaponMode::Portal]
    );
    assert_eq!(
        modes(WeaponLoadout::new(true, BOTH, 2, 1, true)),
        [
            WeaponMode::Projectile,
            WeaponMode::MultiShot(0),
            WeaponMode::MultiShot(1),
            WeaponMode::Missile,
            WeaponMode::Portal,
        ]
    );
    assert_eq!(
        modes(WeaponLoadout::new(false, BOTH, 2, 1, true)),
        [
            WeaponMode::MultiShot(0),
            WeaponMode::MultiShot(1),
            WeaponMode::Missile,
            WeaponMode::Portal
        ]
    );
    assert!(modes(WeaponLoadout::new(false, PortalAccess::None, 0, 0, true)).is_empty());
}

#[test]
fn losing_portal_gun_removes_mode_and_falls_back() {
    let loadout = WeaponLoadout::new(true, BOTH, 0, 1, false);
    assert_eq!(modes(loadout), [WeaponMode::Projectile, WeaponMode::Missile]);
    assert_eq!(loadout.select(WeaponMode::Portal, false), WeaponMode::Projectile);
    let empty = WeaponLoadout::new(false, BOTH, 0, 0, false);
    assert_eq!(empty.select(WeaponMode::Portal, false), WeaponMode::None);
}

#[test]
fn selection_wraps_and_recovers_from_power_up_changes() {
    let loadout = WeaponLoadout::new(true, BOTH, 0, 1, true);
    assert_eq!(loadout.select(WeaponMode::Projectile, true), WeaponMode::Missile);
    assert_eq!(loadout.select(WeaponMode::Portal, true), WeaponMode::Projectile);
    assert_eq!(loadout.select(WeaponMode::Missile, false), WeaponMode::Missile);
    assert_eq!(loadout.select(WeaponMode::MultiShot(0), false), WeaponMode::Projectile);
    assert_eq!(loadout.select(WeaponMode::MultiShot(0), true), WeaponMode::Projectile);

    let powered = WeaponLoadout::new(true, BOTH, 2, 1, true);
    assert_eq!(powered.select(WeaponMode::Projectile, false), WeaponMode::Projectile);
    assert_eq!(powered.select(WeaponMode::Projectile, true), WeaponMode::MultiShot(0));
    assert_eq!(powered.select(WeaponMode::MultiShot(1), true), WeaponMode::Missile);

    let empty = WeaponLoadout::new(false, PortalAccess::None, 0, 0, true);
    assert_eq!(empty.select(WeaponMode::Portal, true), WeaponMode::None);
}

#[test]
fn empty_missiles_leave_the_cycle_and_fall_back_to_projectiles() {
    let loadout = WeaponLoadout::new(true, BOTH, 0, 0, true);
    assert_eq!(modes(loadout), [WeaponMode::Projectile, WeaponMode::Portal]);
    assert_eq!(loadout.select(WeaponMode::Missile, false), WeaponMode::Projectile);
    assert_eq!(loadout.select(WeaponMode::Projectile, true), WeaponMode::Portal);
    let powered = WeaponLoadout::new(true, BOTH, 2, 0, true);
    assert_eq!(powered.select(WeaponMode::Missile, false), WeaponMode::MultiShot(0));
    let portal_only = WeaponLoadout::new(false, BOTH, 0, 0, true);
    assert_eq!(portal_only.select(WeaponMode::Missile, false), WeaponMode::Portal);
    let empty = WeaponLoadout::new(false, BOTH, 0, 0, false);
    assert_eq!(empty.select(WeaponMode::Missile, false), WeaponMode::None);
}

fn selection_app() -> App {
    let config = test_fixtures::gameplay_config();
    let mut players = PlayerMap::default();
    players.insert(
        PlayerId(1),
        PlayerInfo::from_snapshot(
            Entity::PLACEHOLDER,
            &Player {
                generation: PlayerGeneration(0),
                name: "Alice".to_owned(),
                movement: PlayerMovementState::new(Position::default(), PlayerMoveIntent::default(), 0.0, 0.0),
                health: Health(100.0),
                score: 0,
                power_ups: [false; PowerUpKind::COUNT],
                stunned: false,
                held_keys: Vec::new(),
                missiles: 0,
                portal_access: BOTH,
            },
            0,
        ),
    );
    let mut app = App::new();
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ConsoleState>()
        .init_resource::<SettingsMenuState>()
        .init_resource::<PendingWeaponSelection>()
        .init_resource::<WeaponMode>()
        .insert_resource(BOTH)
        .insert_resource(MyPlayerId(PlayerId(1)))
        .insert_resource(players)
        .insert_resource(config)
        .add_systems(Update, input_weapon_select_system);
    app
}

fn collect(app: &mut App, item: ItemType) {
    let mut players = app.world_mut().resource_mut::<PlayerMap>();
    let player = players.get_mut(&PlayerId(1)).expect("local player missing");
    match item {
        ItemType::MissilePack => player.missiles += 1,
        item => {
            if let Some(kind) = PowerUpKind::from_item_type(item) {
                player.power_ups[kind.index()] = true;
            }
        }
    }
    app.world_mut().resource_mut::<PendingWeaponSelection>().collect(item);
}

#[test]
fn pickups_select_the_collected_weapon_once_including_recollection() {
    let mut app = selection_app();
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::None);
    collect(&mut app, ItemType::SingleShotPowerUp);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Projectile);
    collect(&mut app, ItemType::MultiShotPowerUp);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::MultiShot(0));
    collect(&mut app, ItemType::MissilePack);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Missile);
    collect(&mut app, ItemType::PortalGunPowerUp);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Portal);

    *app.world_mut().resource_mut::<WeaponMode>() = WeaponMode::Projectile;
    collect(&mut app, ItemType::SpeedPowerUp);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Projectile);

    collect(&mut app, ItemType::PortalGunPowerUp);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Portal);
    collect(&mut app, ItemType::MissilePack);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Missile);

    *app.world_mut().resource_mut::<WeaponMode>() = WeaponMode::Projectile;
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Projectile);
}

#[test]
fn multishot_works_without_single_shot_and_losing_it_does_not_grant_single_shot() {
    let mut app = selection_app();
    collect(&mut app, ItemType::MultiShotPowerUp);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::MultiShot(0));
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&PlayerId(1))
        .expect("local player missing")
        .power_ups[PowerUpKind::MultiShot.index()] = false;
    app.world_mut().resource_mut::<SettingsMenuState>().open = true;
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::None);
}

#[test]
fn single_shot_and_multishot_remain_separately_selectable() {
    let mut app = selection_app();
    collect(&mut app, ItemType::MultiShotPowerUp);
    collect(&mut app, ItemType::SingleShotPowerUp);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Projectile);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyQ);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::MultiShot(0));
}

#[test]
fn fallback_prefers_multishot_then_single_shot_then_missiles_then_portals() {
    for (loadout, expected) in [
        (WeaponLoadout::new(true, BOTH, 2, 1, true), WeaponMode::MultiShot(0)),
        (WeaponLoadout::new(true, BOTH, 0, 1, true), WeaponMode::Projectile),
        (WeaponLoadout::new(false, BOTH, 0, 1, true), WeaponMode::Missile),
        (WeaponLoadout::new(false, BOTH, 0, 0, true), WeaponMode::Portal),
        (WeaponLoadout::new(false, BOTH, 0, 0, false), WeaponMode::None),
    ] {
        assert_eq!(loadout.select(WeaponMode::None, false), expected);
        assert_eq!(loadout.select(WeaponMode::None, true), expected);
    }
}

#[test]
fn single_shot_pickups_preserve_the_current_multishot_pattern() {
    let mut app = selection_app();
    collect(&mut app, ItemType::MultiShotPowerUp);
    app.update();
    *app.world_mut().resource_mut::<WeaponMode>() = WeaponMode::MultiShot(1);
    for _ in 0..2 {
        collect(&mut app, ItemType::SingleShotPowerUp);
        app.update();
        assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::MultiShot(1));
        assert!(
            app.world()
                .resource::<PlayerMap>()
                .get(&PlayerId(1))
                .expect("local player missing")
                .power_up(PowerUpKind::SingleShot)
        );
    }
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&PlayerId(1))
        .expect("local player missing")
        .power_ups[PowerUpKind::MultiShot.index()] = false;
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Projectile);
}

#[test]
fn single_shot_pickup_selects_single_shot_when_multishot_expired() {
    let mut app = selection_app();
    collect(&mut app, ItemType::MultiShotPowerUp);
    app.update();
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&PlayerId(1))
        .expect("local player missing")
        .power_ups[PowerUpKind::MultiShot.index()] = false;
    collect(&mut app, ItemType::SingleShotPowerUp);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Projectile);
}

#[test]
fn empty_ammo_falls_back_even_with_an_overlay_open() {
    let mut app = selection_app();
    collect(&mut app, ItemType::SingleShotPowerUp);
    collect(&mut app, ItemType::MissilePack);
    app.update();
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&PlayerId(1))
        .expect("local player missing")
        .missiles = 0;
    app.world_mut().resource_mut::<ConsoleState>().open = true;
    app.world_mut().resource_mut::<SettingsMenuState>().open = true;
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyQ);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Projectile);
}

#[test]
fn equipment_erased_after_a_pickup_cannot_be_selected() {
    let mut app = selection_app();
    collect(&mut app, ItemType::SingleShotPowerUp);
    collect(&mut app, ItemType::MultiShotPowerUp);
    collect(&mut app, ItemType::PortalGunPowerUp);
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&PlayerId(1))
        .expect("local player missing")
        .power_ups
        .fill(false);
    app.update();
    assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::None);
}
