use bevy::{ecs::system::EntityCommands, math::Rot2, prelude::*};

use super::settings_menu::SettingsMenuState;
use crate::{
    cameras::CameraViewMode,
    constants::{
        CROSSHAIR_COLOR, CROSSHAIR_LOCK_COLOR, CROSSHAIR_SIZE_PX, CROSSHAIR_THICKNESS_PX, PORTAL_A_COLOR,
        PORTAL_B_COLOR,
    },
    input::WeaponMode,
    missiles::LockOnTarget,
};
use common::{config::GameplayConfig, protocol::*};

const SMALL_CROSSHAIR_SIZE_PX: f32 = 10.0;
const PATTERN_RADIUS_PX: f32 = 24.0;
const PORTAL_RETICLE_WIDTH_PX: f32 = 22.0;
const PORTAL_RETICLE_HEIGHT_PX: f32 = 34.0;
const MISSILE_RETICLE_SIZE_PX: f32 = 34.0;

#[derive(Component)]
pub(crate) struct CrosshairMarker;

// The missile reticle's bars; only their color follows the lock.
#[derive(Component)]
pub(crate) struct MissileReticleMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReticleState {
    mode: WeaponMode,
    portal_access: PortalAccess,
}

pub(crate) fn ui_crosshair_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    lock: Res<LockOnTarget>,
    portal_access: Res<PortalAccess>,
    gameplay_config: Res<GameplayConfig>,
    root: Single<(Entity, Option<&Children>), With<CrosshairMarker>>,
    mut last: Local<Option<ReticleState>>,
) {
    let state = ReticleState {
        mode: *mode,
        portal_access: *portal_access,
    };
    if *last == Some(state) {
        return;
    }
    *last = Some(state);

    let (root_entity, children) = *root;
    if let Some(children) = children {
        for child in children {
            commands.entity(*child).despawn();
        }
    }
    commands.entity(root_entity).with_children(|parent| match state.mode {
        WeaponMode::None => {}
        WeaponMode::Projectile => spawn_crosshair(parent, Vec2::ZERO, CROSSHAIR_SIZE_PX, CROSSHAIR_COLOR),
        WeaponMode::MultiShot(index) => {
            let Some((_, pattern)) = gameplay_config.projectiles.multi_shot.allowed_pattern(index) else {
                return;
            };
            for (offset, center) in pattern_reticle_offsets(pattern.shots()) {
                spawn_crosshair(
                    parent,
                    offset,
                    if center {
                        CROSSHAIR_SIZE_PX
                    } else {
                        SMALL_CROSSHAIR_SIZE_PX
                    },
                    CROSSHAIR_COLOR,
                );
            }
        }
        WeaponMode::Missile => spawn_triangle(parent, lock_color(lock.0.is_some())),
        WeaponMode::Portal => spawn_portal_oval(parent, state.portal_access),
    });
}

pub(crate) fn ui_crosshair_lock_system(
    lock: Res<LockOnTarget>,
    mut bars: Query<&mut BackgroundColor, With<MissileReticleMarker>>,
) {
    if !lock.is_changed() {
        return;
    }
    let color = lock_color(lock.0.is_some());
    for mut bar in &mut bars {
        bar.0 = color;
    }
}

const fn lock_color(locked: bool) -> Color {
    if locked { CROSSHAIR_LOCK_COLOR } else { CROSSHAIR_COLOR }
}

fn pattern_reticle_offsets(shots: &[(f32, f32)]) -> Vec<(Vec2, bool)> {
    let max_offset = shots
        .iter()
        .fold(0.0_f32, |max, (yaw, pitch)| max.max(yaw.abs()).max(pitch.abs()));
    let scale = if max_offset > 0.0 {
        PATTERN_RADIUS_PX / max_offset
    } else {
        0.0
    };
    shots
        .iter()
        .map(|&(yaw, pitch)| {
            (
                Vec2::new(-yaw * scale, -pitch * scale),
                yaw.abs() < f32::EPSILON && pitch.abs() < f32::EPSILON,
            )
        })
        .collect()
}

fn spawn_crosshair(parent: &mut ChildSpawnerCommands, center: Vec2, size: f32, color: Color) {
    spawn_line(parent, center, size, 0.0, color);
    spawn_line(parent, center, size, std::f32::consts::FRAC_PI_2, color);
}

fn spawn_line<'a>(
    parent: &'a mut ChildSpawnerCommands<'_>,
    center: Vec2,
    length: f32,
    angle: f32,
    color: Color,
) -> EntityCommands<'a> {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(center.x - length / 2.0),
            top: Val::Px(center.y - CROSSHAIR_THICKNESS_PX / 2.0),
            width: Val::Px(length),
            height: Val::Px(CROSSHAIR_THICKNESS_PX),
            ..default()
        },
        UiTransform::from_rotation(Rot2::radians(angle)),
        BackgroundColor(color),
    ))
}

fn spawn_triangle(parent: &mut ChildSpawnerCommands, color: Color) {
    let half_side = MISSILE_RETICLE_SIZE_PX / 2.0;
    let height = MISSILE_RETICLE_SIZE_PX * 3.0_f32.sqrt() / 2.0;
    let apex_y = -2.0 * height / 3.0;
    let base_y = height / 3.0;
    let side_mid_y = (apex_y + base_y) / 2.0;
    let angle = height.atan2(half_side);
    spawn_line(parent, Vec2::new(0.0, base_y), MISSILE_RETICLE_SIZE_PX, 0.0, color).insert(MissileReticleMarker);
    spawn_line(
        parent,
        Vec2::new(-half_side / 2.0, side_mid_y),
        MISSILE_RETICLE_SIZE_PX,
        -angle,
        color,
    )
    .insert(MissileReticleMarker);
    spawn_line(
        parent,
        Vec2::new(half_side / 2.0, side_mid_y),
        MISSILE_RETICLE_SIZE_PX,
        angle,
        color,
    )
    .insert(MissileReticleMarker);
}

fn spawn_portal_oval(parent: &mut ChildSpawnerCommands, access: PortalAccess) {
    match access {
        PortalAccess::None => {}
        PortalAccess::Single { end, .. } => spawn_oval(
            parent,
            0.0,
            -PORTAL_RETICLE_HEIGHT_PX / 2.0,
            PORTAL_RETICLE_WIDTH_PX,
            portal_color(end),
        ),
        PortalAccess::Both { .. } => {
            let half = PORTAL_RETICLE_WIDTH_PX / 2.0;
            spawn_clipped_half(parent, -half, half, PORTAL_A_COLOR);
            spawn_clipped_half(parent, 0.0, 0.0, PORTAL_B_COLOR);
        }
    }
}

fn spawn_clipped_half(parent: &mut ChildSpawnerCommands, container_left: f32, oval_center: f32, color: Color) {
    parent
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(container_left),
            top: Val::Px(-PORTAL_RETICLE_HEIGHT_PX / 2.0),
            width: Val::Px(PORTAL_RETICLE_WIDTH_PX / 2.0),
            height: Val::Px(PORTAL_RETICLE_HEIGHT_PX),
            overflow: Overflow::clip(),
            ..default()
        })
        .with_children(|half| spawn_oval(half, oval_center, 0.0, PORTAL_RETICLE_WIDTH_PX, color));
}

fn spawn_oval(parent: &mut ChildSpawnerCommands, center: f32, top: f32, width: f32, color: Color) {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(center - width / 2.0),
            top: Val::Px(top),
            width: Val::Px(width),
            height: Val::Px(PORTAL_RETICLE_HEIGHT_PX),
            border: UiRect::all(Val::Px(CROSSHAIR_THICKNESS_PX)),
            border_radius: BorderRadius::all(Val::Percent(50.0)),
            ..default()
        },
        BorderColor::all(color),
    ));
}

const fn portal_color(end: PortalEnd) -> Color {
    match end {
        PortalEnd::A => PORTAL_A_COLOR,
        PortalEnd::B => PORTAL_B_COLOR,
    }
}

pub(crate) fn ui_crosshair_visibility_system(
    view_mode: Res<CameraViewMode>,
    menu: Res<SettingsMenuState>,
    mut query: Query<&mut Visibility, With<CrosshairMarker>>,
) {
    if !view_mode.is_changed() && !menu.is_changed() {
        return;
    }

    for mut visibility in &mut query {
        *visibility = if view_mode.is_first_person() && !menu.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_reticle_keeps_one_full_size_center_and_normalizes_the_others() {
        let offsets = pattern_reticle_offsets(&[(0.1, 0.0), (0.0, 0.0), (-0.1, 0.05)]);
        assert_eq!(offsets.iter().filter(|(_, center)| *center).count(), 1);
        assert_eq!(offsets[1], (Vec2::ZERO, true));
        assert_eq!(offsets[0].0, Vec2::new(-PATTERN_RADIUS_PX, 0.0));
        assert_eq!(offsets[2].0, Vec2::new(PATTERN_RADIUS_PX, -PATTERN_RADIUS_PX / 2.0));
    }

    #[test]
    fn portal_reticle_colors_match_the_assigned_ends() {
        assert_eq!(portal_color(PortalEnd::A), PORTAL_A_COLOR);
        assert_eq!(portal_color(PortalEnd::B), PORTAL_B_COLOR);
    }
}
