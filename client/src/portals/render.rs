use std::collections::VecDeque;

use bevy::{
    camera::{RenderTarget, visibility::RenderLayers},
    core_pipeline::prepass::{DeferredPrepass, DepthPrepass},
    image::ImageSampler,
    prelude::*,
    render::{render_resource::TextureFormat, view::ColorGrading},
};

use super::{
    PortalMap,
    projection::{PortalProjection, portal_camera_view},
    spawn::{PortalAssets, spawn_portal_replica},
};
use crate::{
    cameras::{MainCameraMarker, SceneRenderTarget, scene_render_target_system},
    config::ClientSettings,
    constants::LOCAL_PLAYER_RENDER_LAYER,
    players::local_player_camera_sync_system,
    schedule::ClientSet,
};
use common::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH},
    physics::PortalFrame,
    protocol::{PlayerId, Portal, PortalEnd},
};

const FIRST_RECURSIVE_RENDER_LAYER: usize = 3;
const MAX_PORTAL_VIEW_CAMERAS: usize = 64;
const MAX_PORTAL_REPLICAS: usize = 512;
const PORTAL_VIEW_RESOLUTIONS: [UVec2; 5] = [
    UVec2::new(64, 120),
    UVec2::new(128, 240),
    UVec2::new(256, 480),
    UVec2::new(512, 960),
    UVec2::new(1024, 1920),
];
const RESOLUTION_SHRINK_THRESHOLD: f32 = 0.8;

type PortalKey = (PlayerId, PortalEnd);

#[derive(Component)]
struct PortalViewCamera {
    chain: Vec<PortalKey>,
    visibility_path: Vec<Entity>,
    target_surface: Entity,
    targets: Vec<PortalViewTarget>,
    target_index: usize,
}

struct PortalViewTarget {
    size: UVec2,
    image: Handle<Image>,
    material: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
struct PortalRenderState {
    portals: Vec<Portal>,
    recursion_depth: Option<u8>,
    spawned: Vec<Entity>,
}

struct PendingView {
    chain: Vec<PortalKey>,
    visibility_path: Vec<Entity>,
    target_surface: Entity,
    recursion_remaining: u8,
}

pub fn portal_render_plugin(app: &mut App) {
    app.init_resource::<PortalRenderState>();
    app.add_systems(
        Update,
        rebuild_portal_views_system
            .after(ClientSet::Network)
            .before(ClientSet::Camera),
    );
    app.add_systems(
        Update,
        update_portal_view_cameras_system
            .in_set(ClientSet::Camera)
            .after(local_player_camera_sync_system)
            .after(scene_render_target_system),
    );
}

fn rebuild_portal_views_system(
    mut commands: Commands,
    portals: Res<PortalMap>,
    portal_assets: Res<PortalAssets>,
    client_settings: Res<ClientSettings>,
    mut state: ResMut<PortalRenderState>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wire_portals = portals.wire_portals();
    let recursion_depth = client_settings.rendering.portal_recursion_depth;
    if state.portals == wire_portals && state.recursion_depth == Some(recursion_depth) {
        return;
    }

    for entity in state.spawned.drain(..) {
        commands.entity(entity).despawn();
    }
    for portal in &wire_portals {
        if let Some(info) = portals.get(&(portal.owner, portal.end)) {
            commands
                .entity(info.entity)
                .insert(MeshMaterial3d(portal_assets.material(portal.end)));
        }
    }

    let complete_portals: Vec<_> = wire_portals
        .iter()
        .copied()
        .filter(|portal| paired_portal(&portals, portal).is_some())
        .collect();
    let mut pending = VecDeque::new();
    for portal in &complete_portals {
        let Some(info) = portals.get(&(portal.owner, portal.end)) else {
            continue;
        };
        pending.push_back(PendingView {
            chain: vec![(portal.owner, portal.end)],
            visibility_path: vec![info.entity],
            target_surface: info.entity,
            recursion_remaining: recursion_depth,
        });
    }

    let deferred = client_settings.rendering.opaque_renderer.is_deferred();
    let msaa = if deferred {
        Msaa::Off
    } else {
        Msaa::from_samples(client_settings.rendering.msaa_samples)
    };
    let mut camera_count = 0;
    let mut replica_count = 0;
    while let Some(view) = pending.pop_front() {
        if camera_count >= MAX_PORTAL_VIEW_CAMERAS {
            break;
        }

        let recursion_depth = view.chain.len() - 1;
        let child_layer = FIRST_RECURSIVE_RENDER_LAYER + camera_count;
        // Bucket images stay immutable because each is both a camera target and a sampled portal texture.
        let targets = vec![create_portal_view_target(
            &mut images,
            &mut materials,
            PORTAL_VIEW_RESOLUTIONS[0],
        )];
        let initial_image = targets[0].image.clone();
        commands
            .entity(view.target_surface)
            .insert(MeshMaterial3d(targets[0].material.clone()));

        let mut camera = commands.spawn((
            PortalViewCamera {
                chain: view.chain.clone(),
                visibility_path: view.visibility_path.clone(),
                target_surface: view.target_surface,
                targets,
                target_index: 0,
            },
            Camera3d::default(),
            Camera {
                order: -(recursion_depth as isize) - 1,
                is_active: false,
                ..default()
            },
            RenderTarget::Image(initial_image.into()),
            Projection::custom(PortalProjection::default()),
            ColorGrading::default(),
            RenderLayers::layer(0).with(LOCAL_PLAYER_RENDER_LAYER).with(child_layer),
            msaa,
            Transform::default(),
        ));
        if deferred {
            camera.insert((DepthPrepass, DeferredPrepass));
        }
        state.spawned.push(camera.id());
        camera_count += 1;

        for portal in &complete_portals {
            if replica_count >= MAX_PORTAL_REPLICAS {
                break;
            }
            let replica = spawn_portal_replica(&mut commands, &portal_assets, portal, child_layer);
            state.spawned.push(replica);
            replica_count += 1;

            if view.recursion_remaining > 0 && camera_count + pending.len() < MAX_PORTAL_VIEW_CAMERAS {
                let mut chain = view.chain.clone();
                chain.push((portal.owner, portal.end));
                let mut visibility_path = view.visibility_path.clone();
                visibility_path.push(replica);
                pending.push_back(PendingView {
                    chain,
                    visibility_path,
                    target_surface: replica,
                    recursion_remaining: view.recursion_remaining - 1,
                });
            }
        }
    }

    if !pending.is_empty() || replica_count >= MAX_PORTAL_REPLICAS {
        warn!(
            "portal recursion reached its render budget ({} cameras, {} surfaces)",
            camera_count, replica_count
        );
    }
    state.portals = wire_portals;
    state.recursion_depth = Some(recursion_depth);
}

fn create_portal_view_image(images: &mut Assets<Image>, size: UVec2) -> Handle<Image> {
    let mut image = Image::new_target_texture(
        size.x,
        size.y,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    );
    image.sampler = ImageSampler::linear();
    images.add(image)
}

fn create_portal_view_target(
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
    size: UVec2,
) -> PortalViewTarget {
    let image = create_portal_view_image(images, size);
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(image.clone()),
        unlit: true,
        ..default()
    });
    PortalViewTarget { size, image, material }
}

fn update_portal_view_cameras_system(
    main_camera: Query<(&Transform, &Projection), (With<Camera3d>, With<MainCameraMarker>, Without<PortalViewCamera>)>,
    scene_target: Res<SceneRenderTarget>,
    portals: Res<PortalMap>,
    visibility: Query<&ViewVisibility>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut view_cameras: Query<
        (
            &mut PortalViewCamera,
            &mut Camera,
            &mut RenderTarget,
            &mut Transform,
            &mut Projection,
        ),
        Without<MainCameraMarker>,
    >,
    mut portal_materials: Query<&mut MeshMaterial3d<StandardMaterial>, Without<PortalViewCamera>>,
) {
    let Ok((main_transform, main_projection)) = main_camera.single() else {
        return;
    };

    for (mut view, mut camera, mut render_target, mut transform, mut projection) in &mut view_cameras {
        let mut view_transform = *main_transform;
        let mut view_projection = main_projection.clone();
        let mut parent_target_size = scene_target.size;
        let mut projected_size = Vec2::ZERO;
        let mut valid = false;
        for key in &view.chain {
            let Some(entry) = portals.get(key).map(|info| &info.portal) else {
                valid = false;
                break;
            };
            let Some(exit) = paired_portal(&portals, entry) else {
                valid = false;
                break;
            };
            let entry_frame = PortalFrame::from_portal(entry);
            let exit_frame = PortalFrame::from_portal(exit);
            projected_size = projected_portal_size(&entry_frame, &view_transform, &view_projection, parent_target_size);
            parent_target_size = portal_resolution(projected_size);
            let Some((next_transform, next_projection)) = portal_camera_view(
                view_transform.translation,
                &entry_frame,
                &exit_frame,
                main_projection.far(),
            ) else {
                valid = false;
                break;
            };
            view_transform = next_transform;
            view_projection = Projection::custom(next_projection);
            valid = true;
        }

        camera.is_active = view
            .visibility_path
            .iter()
            .all(|entity| visibility.get(*entity).is_ok_and(|visible| visible.get()))
            && valid;
        let desired_size = if camera.is_active {
            adaptive_portal_resolution(projected_size, view.targets[view.target_index].size)
        } else {
            PORTAL_VIEW_RESOLUTIONS[0]
        };
        if desired_size != view.targets[view.target_index].size {
            let target_index = view
                .targets
                .iter()
                .position(|target| target.size == desired_size)
                .unwrap_or_else(|| {
                    view.targets
                        .push(create_portal_view_target(&mut images, &mut materials, desired_size));
                    view.targets.len() - 1
                });
            let target_image = view.targets[target_index].image.clone();
            let target_material = view.targets[target_index].material.clone();
            *render_target = RenderTarget::Image(target_image.into());
            if let Ok(mut material) = portal_materials.get_mut(view.target_surface) {
                *material = MeshMaterial3d(target_material);
            }
            view.target_index = target_index;
        }
        if camera.is_active {
            *transform = view_transform;
            *projection = view_projection;
        }
    }
}

fn projected_portal_size(
    frame: &PortalFrame,
    camera_transform: &Transform,
    projection: &Projection,
    target_size: UVec2,
) -> Vec2 {
    let clip_from_world = projection.get_clip_from_view() * camera_transform.to_matrix().inverse();
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut behind = 0;
    for horizontal in [-PORTAL_HALF_WIDTH, PORTAL_HALF_WIDTH] {
        for vertical in [-PORTAL_HALF_HEIGHT, PORTAL_HALF_HEIGHT] {
            let corner = frame.center + frame.right * horizontal + frame.up * vertical;
            let clip = clip_from_world * corner.extend(1.0);
            if clip.w <= f32::EPSILON {
                behind += 1;
                continue;
            }
            let ndc = clip.truncate().xy() / clip.w;
            min = min.min(ndc);
            max = max.max(ndc);
        }
    }
    if behind == 4 {
        return Vec2::ZERO;
    }
    if behind > 0 {
        return target_size.as_vec2();
    }
    min = min.max(Vec2::splat(-1.0));
    max = max.min(Vec2::splat(1.0));
    if min.x >= max.x || min.y >= max.y {
        return Vec2::ZERO;
    }
    (max - min) * 0.5 * target_size.as_vec2()
}

fn portal_resolution(projected_size: Vec2) -> UVec2 {
    PORTAL_VIEW_RESOLUTIONS[resolution_index(projected_size)]
}

fn adaptive_portal_resolution(projected_size: Vec2, current: UVec2) -> UVec2 {
    let desired_index = resolution_index(projected_size);
    let mut index = PORTAL_VIEW_RESOLUTIONS
        .iter()
        .position(|resolution| *resolution == current)
        .unwrap_or(0);
    if desired_index > index {
        index = desired_index;
    } else {
        let demand = resolution_demand(projected_size);
        while index > desired_index
            && demand < PORTAL_VIEW_RESOLUTIONS[index - 1].y as f32 * RESOLUTION_SHRINK_THRESHOLD
        {
            index -= 1;
        }
    }
    PORTAL_VIEW_RESOLUTIONS[index]
}

fn resolution_index(projected_size: Vec2) -> usize {
    let demand = resolution_demand(projected_size);
    PORTAL_VIEW_RESOLUTIONS
        .iter()
        .position(|resolution| resolution.y as f32 >= demand)
        .unwrap_or(PORTAL_VIEW_RESOLUTIONS.len() - 1)
}

fn resolution_demand(projected_size: Vec2) -> f32 {
    let aspect_height = PORTAL_VIEW_RESOLUTIONS[0].y as f32 / PORTAL_VIEW_RESOLUTIONS[0].x as f32;
    projected_size.y.max(projected_size.x * aspect_height)
}

fn paired_portal<'a>(portals: &'a PortalMap, portal: &Portal) -> Option<&'a Portal> {
    let paired_end = match portal.end {
        PortalEnd::A => PortalEnd::B,
        PortalEnd::B => PortalEnd::A,
    };
    portals.get(&(portal.owner, paired_end)).map(|info| &info.portal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_portal_footprint_shrinks_with_distance() {
        let camera = Transform::IDENTITY;
        let mut projection = Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            ..default()
        });
        projection.update(1000.0, 1000.0);
        let near = PortalFrame::from_surface(Vec3::new(0.0, 0.0, -2.0), Vec3::Z, 0.0);
        let far = PortalFrame::from_surface(Vec3::new(0.0, 0.0, -8.0), Vec3::Z, 0.0);

        let near_size = projected_portal_size(&near, &camera, &projection, UVec2::splat(1000));
        let far_size = projected_portal_size(&far, &camera, &projection, UVec2::splat(1000));

        assert!(near_size.x > far_size.x);
        assert!(near_size.y > far_size.y);
    }

    #[test]
    fn portal_behind_camera_has_no_projected_footprint() {
        let mut projection = Projection::Perspective(PerspectiveProjection::default());
        projection.update(1000.0, 1000.0);
        let portal = PortalFrame::from_surface(Vec3::new(0.0, 0.0, 2.0), Vec3::NEG_Z, 0.0);

        assert_eq!(
            projected_portal_size(&portal, &Transform::IDENTITY, &projection, UVec2::splat(1000)),
            Vec2::ZERO
        );
    }

    #[test]
    fn portal_resolution_grows_with_projected_footprint() {
        assert_eq!(
            adaptive_portal_resolution(Vec2::new(120.0, 300.0), PORTAL_VIEW_RESOLUTIONS[1]),
            PORTAL_VIEW_RESOLUTIONS[2]
        );
        assert_eq!(
            adaptive_portal_resolution(Vec2::new(400.0, 800.0), PORTAL_VIEW_RESOLUTIONS[2]),
            PORTAL_VIEW_RESOLUTIONS[3]
        );
    }

    #[test]
    fn distant_portal_uses_minimum_resolution() {
        assert_eq!(portal_resolution(Vec2::ZERO), UVec2::new(64, 120));
    }

    #[test]
    fn very_close_portal_uses_maximum_resolution() {
        assert_eq!(portal_resolution(Vec2::splat(4000.0)), UVec2::new(1024, 1920));
    }

    #[test]
    fn portal_resolution_shrink_has_hysteresis() {
        assert_eq!(
            adaptive_portal_resolution(Vec2::new(200.0, 400.0), PORTAL_VIEW_RESOLUTIONS[3]),
            PORTAL_VIEW_RESOLUTIONS[3]
        );
        assert_eq!(
            adaptive_portal_resolution(Vec2::new(150.0, 300.0), PORTAL_VIEW_RESOLUTIONS[3]),
            PORTAL_VIEW_RESOLUTIONS[2]
        );
    }
}
