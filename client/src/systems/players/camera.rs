use bevy::{camera::Viewport, prelude::*};

use super::components::CameraShake;
use crate::{
    config::RenderSettings,
    constants::*,
    markers::*,
    resources::{CameraViewMode, TopDownCameraYaw},
    systems::visual_focus_level,
};
use common::{
    config::GameplayConfig,
    constants::{LEVEL_HEIGHT, MAP_DEPTH, MAP_WIDTH},
    protocol::{Floor, MapLayout, Position},
};

#[derive(Copy, Clone)]
struct FloorBounds {
    min_x: f32,
    max_x: f32,
    min_z: f32,
    max_z: f32,
}

impl FloorBounds {
    const fn fallback() -> Self {
        Self {
            min_x: -MAP_WIDTH / 2.0,
            max_x: MAP_WIDTH / 2.0,
            min_z: -MAP_DEPTH / 2.0,
            max_z: MAP_DEPTH / 2.0,
        }
    }

    fn include_floor(&mut self, floor: &Floor) {
        let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
        self.min_x = self.min_x.min(min_x);
        self.max_x = self.max_x.max(max_x);
        self.min_z = self.min_z.min(min_z);
        self.max_z = self.max_z.max(max_z);
    }

    fn center(self) -> Vec3 {
        Vec3::new(
            f32::midpoint(self.min_x, self.max_x),
            0.0,
            f32::midpoint(self.min_z, self.max_z),
        )
    }

    fn width(self) -> f32 {
        self.max_x - self.min_x
    }

    fn depth(self) -> f32 {
        self.max_z - self.min_z
    }
}

fn floor_bounds_for_level(map_layout: &MapLayout, level: u8) -> FloorBounds {
    let mut bounds = FloorBounds {
        min_x: f32::INFINITY,
        max_x: f32::NEG_INFINITY,
        min_z: f32::INFINITY,
        max_z: f32::NEG_INFINITY,
    };

    for floor in map_layout.floors.iter().filter(|floor| floor.level == level) {
        bounds.include_floor(floor);
    }

    if bounds.min_x.is_finite() {
        bounds
    } else {
        FloorBounds::fallback()
    }
}

fn topdown_camera_offset_to_fit(bounds: FloorBounds, aspect_ratio: f32, fov: f32, view_direction: Vec3) -> Vec3 {
    let tilt = TOPDOWN_CAMERA_TILT_DEGREES.to_radians();
    let half_vertical_fov_tan = (fov / 2.0).tan();
    let half_horizontal_fov_tan = half_vertical_fov_tan * aspect_ratio.max(0.1);
    let view_extent = floor_extent_along_view(bounds, view_direction);
    let cross_extent = floor_extent_across_view(bounds, view_direction);
    let cross_distance = cross_extent * TOPDOWN_CAMERA_MARGIN / (2.0 * half_horizontal_fov_tan);
    let view_distance = view_extent * tilt.cos() * TOPDOWN_CAMERA_MARGIN / (2.0 * half_vertical_fov_tan);
    let view_distance = cross_distance.max(view_distance).max(LEVEL_HEIGHT);

    Vec3::Y * (view_distance * tilt.cos()) + view_direction * (view_distance * tilt.sin())
}

fn projected_center_shift(bounds: FloorBounds, camera_offset: Vec3, view_direction: Vec3) -> f32 {
    let half_view_extent = floor_extent_along_view(bounds, view_direction) / 2.0;
    let view_offset = camera_offset.dot(view_direction);
    if half_view_extent <= 0.0 || view_offset.abs() <= f32::EPSILON {
        return 0.0;
    }

    let distance_squared = camera_offset.length_squared();
    let discriminant = distance_squared.mul_add(
        distance_squared,
        4.0 * view_offset * view_offset * half_view_extent * half_view_extent,
    );
    ((discriminant.sqrt() - distance_squared) / (2.0 * view_offset)).clamp(-half_view_extent, half_view_extent)
}

fn floor_extent_along_view(bounds: FloorBounds, view_direction: Vec3) -> f32 {
    if view_direction.x.abs() > view_direction.z.abs() {
        bounds.width()
    } else {
        bounds.depth()
    }
}

fn floor_extent_across_view(bounds: FloorBounds, view_direction: Vec3) -> f32 {
    if view_direction.x.abs() > view_direction.z.abs() {
        bounds.depth()
    } else {
        bounds.width()
    }
}

fn topdown_view_direction(yaw: f32) -> Vec3 {
    Vec3::new(yaw.sin(), 0.0, yaw.cos())
}

fn window_aspect_ratio(windows: &Query<&Window>) -> f32 {
    windows
        .single()
        .map_or(16.0 / 9.0, |window| window.width() / window.height().max(1.0))
}

fn topdown_camera_transform(
    player_pos: &Position,
    map_layout: Option<&MapLayout>,
    aspect_ratio: f32,
    fov: f32,
    yaw: f32,
) -> Transform {
    let view_direction = topdown_view_direction(yaw);
    let player_level = visual_focus_level(player_pos.y);
    let floor_bounds = map_layout.map_or_else(FloorBounds::fallback, |layout| {
        floor_bounds_for_level(layout, player_level)
    });
    let mut target = floor_bounds.center();
    target.y = f32::from(player_level) * LEVEL_HEIGHT;
    let camera_offset = topdown_camera_offset_to_fit(floor_bounds, aspect_ratio, fov, view_direction);
    let center_shift = projected_center_shift(floor_bounds, camera_offset, view_direction);
    target += view_direction * center_shift;

    Transform::from_translation(target + camera_offset).looking_at(target, Vec3::Y)
}

fn sync_first_person_camera(
    camera_transform: &mut Transform,
    player_pos: &Position,
    player_eye_height: f32,
    maybe_shake: Option<&CameraShake>,
) {
    camera_transform.translation.x = player_pos.x;
    camera_transform.translation.z = player_pos.z;
    camera_transform.translation.y = player_pos.y + player_eye_height;

    if let Some(shake) = maybe_shake {
        camera_transform.translation.x += shake.offset_x;
        camera_transform.translation.y += shake.offset_y;
        camera_transform.translation.z += shake.offset_z;
    }
}

// ============================================================================
// Camera Sync Systems
// ============================================================================

// Update camera position to follow local player
pub fn local_player_camera_sync_system(
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    map_layout: Option<Res<MapLayout>>,
    windows: Query<&Window>,
    mut camera_query: Query<
        (&mut Transform, &mut Projection, Option<&CameraShake>),
        (With<Camera3d>, With<MainCameraMarker>),
    >,
    view_mode: Res<CameraViewMode>,
    top_down_camera_yaw: Res<TopDownCameraYaw>,
    render_settings: Res<RenderSettings>,
    gameplay_config: Res<GameplayConfig>,
) {
    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    let Ok((mut camera_transform, mut projection, maybe_shake)) = camera_query.single_mut() else {
        return;
    };

    let Projection::Perspective(persp) = projection.as_mut() else {
        return;
    };

    match *view_mode {
        CameraViewMode::FirstPerson => {
            persp.fov = render_settings.fov_first_person_degrees.to_radians();
            sync_first_person_camera(
                &mut camera_transform,
                player_pos,
                gameplay_config.player.eye_height(),
                maybe_shake,
            );
        }
        CameraViewMode::TopDown => {
            persp.fov = render_settings.fov_top_down_degrees.to_radians();
            *camera_transform = topdown_camera_transform(
                player_pos,
                map_layout.as_deref(),
                window_aspect_ratio(&windows),
                persp.fov,
                top_down_camera_yaw.0,
            );
        }
    }
}

// Update local player visibility based on camera view mode
pub fn local_player_visibility_sync_system(
    view_mode: Res<CameraViewMode>,
    mut local_player_query: Query<&mut Visibility, With<LocalPlayerMarker>>,
) {
    // Always check and update, not just when changed, to ensure it's correct
    for mut visibility in &mut local_player_query {
        let desired_visibility = match *view_mode {
            CameraViewMode::FirstPerson => Visibility::Hidden,
            CameraViewMode::TopDown => Visibility::Visible,
        };

        if *visibility != desired_visibility {
            *visibility = desired_visibility;
        }
    }
}

// Update rearview camera to look backwards from local player
pub fn local_player_rearview_sync_system(
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    main_camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>, Without<RearviewCameraMarker>)>,
    mut rearview_query: Query<&mut Transform, (With<RearviewCameraMarker>, Without<MainCameraMarker>)>,
    view_mode: Res<CameraViewMode>,
    render_settings: Res<RenderSettings>,
    gameplay_config: Res<GameplayConfig>,
) {
    if !render_settings.rearview_enabled || !view_mode.is_first_person() {
        return;
    }

    let Some(player_pos) = local_player_query.iter().next() else {
        return;
    };

    let Ok(mut rearview_transform) = rearview_query.single_mut() else {
        return;
    };

    rearview_transform.translation.x = player_pos.x;
    rearview_transform.translation.z = player_pos.z;
    rearview_transform.translation.y = player_pos.y + gameplay_config.player.eye_height();

    // Get the main camera's rotation and rotate 180 degrees.
    if let Ok(main_transform) = main_camera_query.single() {
        let main_yaw = main_transform.rotation.to_euler(EulerRot::YXZ).0;
        let backwards_yaw = main_yaw + std::f32::consts::PI;
        rearview_transform.rotation = Quat::from_rotation_y(backwards_yaw);
    }
}

// Update rearview camera viewport based on window size
pub fn local_player_rearview_system(
    windows: Query<&Window>,
    mut rearview_query: Query<&mut Camera, With<RearviewCameraMarker>>,
    view_mode: Res<CameraViewMode>,
    render_settings: Res<RenderSettings>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Ok(mut camera) = rearview_query.single_mut() else {
        return;
    };

    // Only show rearview in first-person mode
    let is_active = render_settings.rearview_enabled && view_mode.is_first_person();
    camera.is_active = is_active;

    if !is_active {
        return;
    }

    let window_width = window.physical_width();
    let window_height = window.physical_height();

    let viewport_width = (window_width as f32 * REARVIEW_WIDTH_RATIO) as u32;
    let viewport_height = (window_height as f32 * REARVIEW_HEIGHT_RATIO) as u32;

    let margin_x = (window_width as f32 * REARVIEW_MARGIN) as u32;
    let margin_y = (window_height as f32 * REARVIEW_MARGIN) as u32;

    // Position in lower-right corner
    let x = window_width.saturating_sub(viewport_width + margin_x);
    let y = margin_y;

    camera.viewport = Some(Viewport {
        physical_position: UVec2::new(x, y),
        physical_size: UVec2::new(viewport_width, viewport_height),
        depth: 0.0..1.0,
    });
}
