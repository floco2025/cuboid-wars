use bevy::prelude::*;

use crate::{
    players::{MyPlayerId, PlayerMap},
    ui::{ConsoleState, SettingsMenuState},
};
use common::{config::GameplayConfig, protocol::*};

// Which weapon the mouse buttons drive. Client-only presentation state, like
// `CameraViewMode`; the server just receives whichever shot message results.
// `input_weapon_select_system` keeps it inside the player's loadout every
// frame, so the fire, lock-on, and crosshair systems trust it as-is.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponMode {
    #[default]
    None,
    Projectile,
    // Index into `MultiShotConfig::allowed_patterns`.
    MultiShot(usize),
    Missile,
    Portal,
}

#[derive(Resource, Default)]
pub struct PendingWeaponSelection(Option<WeaponMode>);

impl PendingWeaponSelection {
    pub fn collect(&mut self, item: ItemType) {
        match item {
            ItemType::MissilePack => self.0 = Some(WeaponMode::Missile),
            ItemType::PortalGunPowerUp => self.0 = Some(WeaponMode::Portal),
            _ => {}
        }
    }
}

// The weapons the local player can cycle through right now, in Q order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WeaponLoadout {
    projectiles: bool,
    // The multi-shot power-up replaces the plain projectile with one mode per
    // allowed pattern; zero while it is inactive.
    multi_shot_patterns: usize,
    missiles: bool,
    portals: bool,
}

impl WeaponLoadout {
    const fn new(
        projectiles: bool,
        portal_access: PortalAccess,
        multi_shot_patterns: usize,
        missiles: u32,
        has_portal_gun: bool,
    ) -> Self {
        Self {
            projectiles,
            multi_shot_patterns,
            missiles: missiles > 0,
            portals: has_portal_gun && !matches!(portal_access, PortalAccess::None),
        }
    }

    fn modes(self) -> impl Iterator<Item = WeaponMode> {
        let plain = (self.projectiles && self.multi_shot_patterns == 0).then_some(WeaponMode::Projectile);
        let patterns = if self.projectiles { self.multi_shot_patterns } else { 0 };
        plain
            .into_iter()
            .chain((0..patterns).map(WeaponMode::MultiShot))
            .chain(self.missiles.then_some(WeaponMode::Missile))
            .chain(self.portals.then_some(WeaponMode::Portal))
    }

    fn contains(self, mode: WeaponMode) -> bool {
        self.modes().any(|candidate| candidate == mode)
    }

    // Keeps `current` while it is still offered, otherwise falls back to the
    // first mode; `advance` steps to the next mode, wrapping.
    fn select(self, current: WeaponMode, advance: bool) -> WeaponMode {
        let selected = if advance {
            self.modes()
                .skip_while(|mode| *mode != current)
                .nth(1)
                .or_else(|| self.modes().next())
        } else if self.contains(current) {
            Some(current)
        } else {
            self.modes().next()
        };
        selected.unwrap_or(WeaponMode::None)
    }
}

// Runs even while a text or menu overlay is open, so a power-up expiring
// mid-chat still re-selects; only the Q press is gated.
pub fn input_weapon_select_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    console: Res<ConsoleState>,
    menu: Res<SettingsMenuState>,
    map_settings: Res<MapSettings>,
    portal_access: Res<PortalAccess>,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    mut mode: ResMut<WeaponMode>,
    mut pending: ResMut<PendingWeaponSelection>,
) {
    let player = players.get(&my_player_id.0);
    let has_multi_shot = player.is_some_and(|info| info.power_up(PowerUpKind::MultiShot));
    let multi_shot_patterns = if has_multi_shot {
        gameplay_config.projectiles.multi_shot.allowed_patterns().len()
    } else {
        0
    };
    let has_portal_gun = player.is_some_and(|info| info.power_up(PowerUpKind::PortalGun));
    let loadout = WeaponLoadout::new(
        map_settings.weapons.projectiles,
        *portal_access,
        multi_shot_patterns,
        player.map_or(0, |info| info.missiles),
        has_portal_gun,
    );
    let advance = keyboard.just_pressed(KeyCode::KeyQ) && !console.open && !menu.open;
    let requested = pending.0.take().filter(|requested| loadout.contains(*requested));
    let selected = loadout.select(requested.unwrap_or(*mode), advance);
    *mode = selected;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::PlayerInfo;

    const BOTH: PortalAccess = PortalAccess::Both { pair: PortalPairId(1) };

    fn modes(loadout: WeaponLoadout) -> Vec<WeaponMode> {
        loadout.modes().collect()
    }

    #[test]
    fn loadout_follows_map_and_power_up_in_cycle_order() {
        assert_eq!(
            modes(WeaponLoadout::new(true, BOTH, 0, 1, true)),
            [WeaponMode::Projectile, WeaponMode::Missile, WeaponMode::Portal]
        );
        assert_eq!(
            modes(WeaponLoadout::new(true, BOTH, 2, 1, true)),
            [
                WeaponMode::MultiShot(0),
                WeaponMode::MultiShot(1),
                WeaponMode::Missile,
                WeaponMode::Portal,
            ]
        );
        assert_eq!(
            modes(WeaponLoadout::new(false, BOTH, 2, 1, true)),
            [WeaponMode::Missile, WeaponMode::Portal]
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
        assert_eq!(powered.select(WeaponMode::Projectile, false), WeaponMode::MultiShot(0));
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
        let source: serde_json::Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
            .expect("server gameplay JSON is invalid");
        let config: GameplayConfig = serde_json::from_value(serde_json::json!({
            "player": source["player"],
            "projectiles": source["weapons"]["projectiles"],
            "missiles": source["weapons"]["missiles"],
            "portals": source["weapons"]["portals"],
            "actors": source["actors"]["kinds"],
        }))
        .expect("client gameplay config is invalid");
        let mut players = PlayerMap::default();
        players.insert(
            PlayerId(1),
            PlayerInfo::from_snapshot(
                Entity::PLACEHOLDER,
                &Player {
                    name: "Alice".to_owned(),
                    movement: PlayerMovementState::new(Position::default(), PlayerMoveIntent::default(), 0.0, 0.0),
                    health: Health(100.0),
                    score: 0,
                    power_ups: [false; PowerUpKind::COUNT],
                    stunned: false,
                    held_keys: Vec::new(),
                    missiles: 0,
                    portal_access: BOTH,
                    hops: 0,
                },
                0,
            ),
        );
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ConsoleState>()
            .init_resource::<SettingsMenuState>()
            .init_resource::<PendingWeaponSelection>()
            .insert_resource(WeaponMode::Projectile)
            .insert_resource(crate::test_geometry::map_settings())
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
            ItemType::PortalGunPowerUp => player.power_ups[PowerUpKind::PortalGun.index()] = true,
            _ => {}
        }
        app.world_mut().resource_mut::<PendingWeaponSelection>().collect(item);
    }

    #[test]
    fn pickups_select_the_collected_weapon_once_including_recollection() {
        let mut app = selection_app();
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
    fn empty_ammo_falls_back_even_with_an_overlay_open() {
        let mut app = selection_app();
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
        app.world_mut()
            .resource_mut::<PendingWeaponSelection>()
            .collect(ItemType::PortalGunPowerUp);
        app.update();
        assert_eq!(*app.world().resource::<WeaponMode>(), WeaponMode::Projectile);
    }
}
