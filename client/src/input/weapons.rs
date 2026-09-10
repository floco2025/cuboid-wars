use bevy::prelude::*;

use crate::{
    players::{MyPlayerId, PlayerMap},
    ui::{ConsoleState, SettingsMenuState},
};
use common::{
    config::GameplayConfig,
    protocol::{ItemType, PortalAccess, PowerUpKind},
};

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
            ItemType::SingleShotPowerUp => self.0 = Some(WeaponMode::Projectile),
            ItemType::MultiShotPowerUp => self.0 = Some(WeaponMode::MultiShot(0)),
            ItemType::MissilePack => self.0 = Some(WeaponMode::Missile),
            ItemType::PortalGunPowerUp => self.0 = Some(WeaponMode::Portal),
            _ => {}
        }
    }
}

// The weapons the local player can cycle through right now, in Q order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WeaponLoadout {
    single_shot: bool,
    multi_shot_patterns: usize,
    missiles: bool,
    portals: bool,
}

impl WeaponLoadout {
    const fn new(
        single_shot: bool,
        portal_access: PortalAccess,
        multi_shot_patterns: usize,
        missiles: u32,
        has_portal_gun: bool,
    ) -> Self {
        Self {
            single_shot,
            multi_shot_patterns,
            missiles: missiles > 0,
            portals: has_portal_gun && !matches!(portal_access, PortalAccess::None),
        }
    }

    fn modes(self) -> impl Iterator<Item = WeaponMode> {
        let plain = self.single_shot.then_some(WeaponMode::Projectile);
        plain
            .into_iter()
            .chain((0..self.multi_shot_patterns).map(WeaponMode::MultiShot))
            .chain(self.missiles.then_some(WeaponMode::Missile))
            .chain(self.portals.then_some(WeaponMode::Portal))
    }

    fn contains(self, mode: WeaponMode) -> bool {
        self.modes().any(|candidate| candidate == mode)
    }

    fn fallback(self) -> WeaponMode {
        if self.multi_shot_patterns > 0 {
            WeaponMode::MultiShot(0)
        } else {
            self.modes().next().unwrap_or(WeaponMode::None)
        }
    }

    fn select(self, current: WeaponMode, advance: bool) -> WeaponMode {
        if !self.contains(current) {
            return self.fallback();
        }
        if !advance {
            return current;
        }
        self.modes()
            .skip_while(|mode| *mode != current)
            .nth(1)
            .or_else(|| self.modes().next())
            .unwrap_or(WeaponMode::None)
    }
}

// Runs even while a text or menu overlay is open, so a power-up expiring
// mid-chat still re-selects; only the Q press is gated.
pub fn input_weapon_select_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    console: Res<ConsoleState>,
    menu: Res<SettingsMenuState>,
    portal_access: Res<PortalAccess>,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    mut mode: ResMut<WeaponMode>,
    mut pending: ResMut<PendingWeaponSelection>,
) {
    let player = players.get(&my_player_id.0);
    let has_single_shot = player.is_some_and(|info| info.power_up(PowerUpKind::SingleShot));
    let has_multi_shot = player.is_some_and(|info| info.power_up(PowerUpKind::MultiShot));
    let multi_shot_patterns = if has_multi_shot {
        gameplay_config.projectiles.multi_shot.allowed_patterns().len()
    } else {
        0
    };
    let has_portal_gun = player.is_some_and(|info| info.power_up(PowerUpKind::PortalGun));
    let loadout = WeaponLoadout::new(
        has_single_shot,
        *portal_access,
        multi_shot_patterns,
        player.map_or(0, |info| info.missiles),
        has_portal_gun,
    );
    let advance = keyboard.just_pressed(KeyCode::KeyQ) && !console.open && !menu.open;
    let keep_multishot = matches!(*mode, WeaponMode::MultiShot(_)) && loadout.contains(*mode);
    let requested = pending
        .0
        .take()
        .filter(|requested| loadout.contains(*requested) && !(*requested == WeaponMode::Projectile && keep_multishot));
    let selected = loadout.select(requested.unwrap_or(*mode), advance);
    *mode = selected;
}

#[cfg(test)]
#[path = "tests/weapons.rs"]
mod tests;
