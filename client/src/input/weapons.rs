use bevy::prelude::*;

use crate::{
    players::{MyPlayerId, PlayerMap},
    ui::{ConsoleState, SettingsMenuState},
};
use common::{config::GameplayConfig, protocol::*};

// Which weapon the mouse buttons drive. Client-only presentation state, like
// `CameraViewMode`; the server just receives whichever shot message results.
// `input_weapon_select_system` keeps it inside the map's loadout every
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
    const fn new(weapons: MapWeaponSettings, portal_access: PortalAccess, multi_shot_patterns: usize) -> Self {
        Self {
            projectiles: weapons.projectiles,
            multi_shot_patterns,
            missiles: weapons.missiles,
            portals: !matches!(portal_access, PortalAccess::None),
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
) {
    let has_multi_shot = players
        .get(&my_player_id.0)
        .is_some_and(|info| info.power_up(PowerUpKind::MultiShot));
    let multi_shot_patterns = if has_multi_shot {
        gameplay_config.projectiles.multi_shot.allowed_patterns().len()
    } else {
        0
    };
    let loadout = WeaponLoadout::new(map_settings.weapons, *portal_access, multi_shot_patterns);
    let advance = keyboard.just_pressed(KeyCode::KeyQ) && !console.open && !menu.open;
    if !advance && loadout.contains(*mode) {
        return;
    }
    let selected = loadout.select(*mode, advance);
    *mode = selected;
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: PortalAccess = PortalAccess::Both { pair: PortalPairId(1) };

    const fn weapons(projectiles: bool, missiles: bool) -> MapWeaponSettings {
        MapWeaponSettings {
            projectiles,
            missiles,
            portals: PortalMode::Both,
        }
    }

    fn modes(loadout: WeaponLoadout) -> Vec<WeaponMode> {
        loadout.modes().collect()
    }

    #[test]
    fn loadout_follows_map_and_power_up_in_cycle_order() {
        assert_eq!(
            modes(WeaponLoadout::new(weapons(true, true), BOTH, 0)),
            [WeaponMode::Projectile, WeaponMode::Missile, WeaponMode::Portal]
        );
        assert_eq!(
            modes(WeaponLoadout::new(weapons(true, true), BOTH, 2)),
            [
                WeaponMode::MultiShot(0),
                WeaponMode::MultiShot(1),
                WeaponMode::Missile,
                WeaponMode::Portal,
            ]
        );
        assert_eq!(
            modes(WeaponLoadout::new(weapons(false, true), BOTH, 2)),
            [WeaponMode::Missile, WeaponMode::Portal]
        );
        assert!(modes(WeaponLoadout::new(weapons(false, false), PortalAccess::None, 0)).is_empty());
    }

    #[test]
    fn selection_wraps_and_recovers_from_power_up_changes() {
        let loadout = WeaponLoadout::new(weapons(true, true), BOTH, 0);
        assert_eq!(loadout.select(WeaponMode::Projectile, true), WeaponMode::Missile);
        assert_eq!(loadout.select(WeaponMode::Portal, true), WeaponMode::Projectile);
        assert_eq!(loadout.select(WeaponMode::Missile, false), WeaponMode::Missile);
        assert_eq!(loadout.select(WeaponMode::MultiShot(0), false), WeaponMode::Projectile);
        assert_eq!(loadout.select(WeaponMode::MultiShot(0), true), WeaponMode::Projectile);

        let powered = WeaponLoadout::new(weapons(true, true), BOTH, 2);
        assert_eq!(powered.select(WeaponMode::Projectile, false), WeaponMode::MultiShot(0));
        assert_eq!(powered.select(WeaponMode::MultiShot(1), true), WeaponMode::Missile);

        let empty = WeaponLoadout::new(weapons(false, false), PortalAccess::None, 0);
        assert_eq!(empty.select(WeaponMode::Portal, true), WeaponMode::None);
    }
}
