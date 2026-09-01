use std::collections::VecDeque;

use bevy::{
    camera::{Hdr, RenderTarget, visibility::RenderLayers},
    core_pipeline::{
        prepass::{DeferredPrepass, DepthPrepass},
        tonemapping::Tonemapping,
    },
    image::ImageSampler,
    prelude::*,
    render::render_resource::TextureFormat,
};

use super::{
    PortalMap,
    projection::{PortalProjection, portal_camera_view},
    spawn::{PortalAssets, spawn_portal_visual},
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

    let complete_portals: Vec<_> = wire_portals
        .iter()
        .copied()
        .filter(|portal| paired_portal(&portals, portal).is_some())
        .collect();
    let mut over_budget = false;
    let mut pending = VecDeque::new();
    for portal in &complete_portals {
        let Some(info) = portals.get(&(portal.owner, portal.end)) else {
            continue;
        };
        if pending.len() >= MAX_PORTAL_VIEW_CAMERAS {
            over_budget = true;
            break;
        }
        pending.push_back(PendingView {
            chain: vec![(portal.owner, portal.end)],
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
        let recursion_depth = view.chain.len() - 1;
        let child_layer = FIRST_RECURSIVE_RENDER_LAYER + camera_count;
        // Bucket images stay immutable because each is both a camera target and a sampled portal texture.
        let targets = vec![create_portal_view_target(
            &mut images,
            &mut materials,
            PORTAL_VIEW_RESOLUTIONS[0],
        )];
        let initial_image = targets[0].image.clone();
        let mut camera = commands.spawn((
            PortalViewCamera {
                chain: view.chain.clone(),
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
            // Linear HDR straight into the texture: the presenting camera tonemaps once.
            Hdr,
            Tonemapping::None,
            RenderTarget::Image(initial_image.into()),
            Projection::custom(PortalProjection::default()),
            RenderLayers::layer(0).with(LOCAL_PLAYER_RENDER_LAYER).with(child_layer),
            msaa,
            Transform::default(),
        ));
        if deferred {
            camera.insert((DepthPrepass, DeferredPrepass));
        }
        state.spawned.push(camera.id());
        camera_count += 1;

        // The exit of this view's own hop sits behind its near plane: never
        // visible, never a valid view.
        let own_exit = paired_key(*view.chain.last().expect("portal view chain is empty"));
        for portal in &complete_portals {
            if (portal.owner, portal.end) == own_exit {
                continue;
            }
            if replica_count >= MAX_PORTAL_REPLICAS {
                over_budget = true;
                break;
            }
            let replica = spawn_portal_visual(&mut commands, &portal_assets, portal, child_layer);
            state.spawned.push(replica);
            replica_count += 1;

            if view.recursion_remaining == 0 {
                continue;
            }
            if camera_count + pending.len() >= MAX_PORTAL_VIEW_CAMERAS {
                over_budget = true;
                continue;
            }
            let mut chain = view.chain.clone();
            chain.push((portal.owner, portal.end));
            pending.push_back(PendingView {
                chain,
                target_surface: replica,
                recursion_remaining: view.recursion_remaining - 1,
            });
        }
    }

    if over_budget {
        warn!(
            "portal views exceed the render budget ({camera_count} cameras, {replica_count} surfaces); deeper views are omitted"
        );
    }
    state.portals = wire_portals;
    state.recursion_depth = Some(recursion_depth);
}

fn create_portal_view_image(images: &mut Assets<Image>, size: UVec2) -> Handle<Image> {
    let mut image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba16Float, None);
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
    portal_assets: Res<PortalAssets>,
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
    mut surface_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let Ok((main_transform, main_projection)) = main_camera.single() else {
        return;
    };

    for (mut view, mut camera, mut render_target, mut transform, mut projection) in &mut view_cameras {
        let mapped = view_through_chain(
            &portals,
            &view.chain,
            main_transform,
            main_projection,
            scene_target.size,
        );
        camera.is_active = mapped.is_some();
        let surface_material = match mapped {
            Some((view_transform, view_projection, projected_size)) => {
                *transform = view_transform;
                *projection = view_projection;
                let desired_size = adaptive_portal_resolution(projected_size, view.targets[view.target_index].size);
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
                    *render_target = RenderTarget::Image(view.targets[target_index].image.clone().into());
                    view.target_index = target_index;
                }
                view.targets[view.target_index].material.clone()
            }
            // A surface shows a view only while that view renders this frame;
            // otherwise its own glow, never stale pixels.
            None => portal_assets.material(view.chain.last().expect("portal view chain is empty").1),
        };
        if let Ok(mut material) = surface_materials.get_mut(view.target_surface)
            && material.0 != surface_material
        {
            material.0 = surface_material;
        }
    }
}

// Maps the main camera through every hop of `chain`: the eye through each
// entry to its exit, and the entry's footprint in the view before it. `None`
// when a hop is invalid or off screen — computed from this frame's camera
// rather than read back from visibility, so activation never lags a frame.
// Returns the final view and the last hop's footprint.
fn view_through_chain(
    portals: &PortalMap,
    chain: &[PortalKey],
    main_transform: &Transform,
    main_projection: &Projection,
    scene_size: UVec2,
) -> Option<(Transform, Projection, Vec2)> {
    let mut view_transform = *main_transform;
    let mut view_projection = main_projection.clone();
    let mut parent_target_size = scene_size;
    let mut projected_size = Vec2::ZERO;
    for key in chain {
        let entry = &portals.get(key)?.portal;
        let exit = paired_portal(portals, entry)?;
        let entry_frame = PortalFrame::from_portal(entry);
        let exit_frame = PortalFrame::from_portal(exit);
        projected_size = projected_portal_size(&entry_frame, &view_transform, &view_projection, parent_target_size);
        if projected_size.x <= 0.0 || projected_size.y <= 0.0 {
            return None;
        }
        parent_target_size = portal_resolution(projected_size);
        let (next_transform, next_projection) = portal_camera_view(
            view_transform.translation,
            &entry_frame,
            &exit_frame,
            main_projection.far(),
        )?;
        view_transform = next_transform;
        view_projection = Projection::custom(next_projection);
    }
    Some((view_transform, view_projection, projected_size))
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

fn paired_end(end: PortalEnd) -> PortalEnd {
    match end {
        PortalEnd::A => PortalEnd::B,
        PortalEnd::B => PortalEnd::A,
    }
}

fn paired_key((owner, end): PortalKey) -> PortalKey {
    (owner, paired_end(end))
}

fn paired_portal<'a>(portals: &'a PortalMap, portal: &Portal) -> Option<&'a Portal> {
    portals
        .get(&paired_key((portal.owner, portal.end)))
        .map(|info| &info.portal)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;
    use crate::portals::PortalInfo;

    fn perspective() -> Projection {
        let mut projection = Projection::Perspective(PerspectiveProjection {
            fov: FRAC_PI_2,
            ..default()
        });
        projection.update(1000.0, 1000.0);
        projection
    }

    fn portal(owner: u32, end: PortalEnd, pos: Vec3, normal: Vec3) -> Portal {
        Portal {
            owner: PlayerId(owner),
            end,
            pos: pos.into(),
            nx: normal.x,
            ny: normal.y,
            nz: normal.z,
            yaw: 0.0,
        }
    }

    fn portal_map(portals: &[Portal]) -> PortalMap {
        let mut map = PortalMap::default();
        for portal in portals {
            map.insert(
                (portal.owner, portal.end),
                PortalInfo {
                    entity: Entity::PLACEHOLDER,
                    portal: *portal,
                },
            );
        }
        map
    }

    // A facing pair 8 m apart on the z axis, the camera between them looking at A.
    fn facing_pair() -> (PortalMap, PortalKey, PortalKey) {
        let a = portal(1, PortalEnd::A, Vec3::new(0.0, 1.0, -4.0), Vec3::Z);
        let b = portal(1, PortalEnd::B, Vec3::new(0.0, 1.0, 4.0), Vec3::NEG_Z);
        (portal_map(&[a, b]), (a.owner, a.end), (b.owner, b.end))
    }

    #[test]
    fn projected_portal_footprint_shrinks_with_distance() {
        let camera = Transform::IDENTITY;
        let projection = perspective();
        let near = PortalFrame::from_surface(Vec3::new(0.0, 0.0, -2.0), Vec3::Z, 0.0);
        let far = PortalFrame::from_surface(Vec3::new(0.0, 0.0, -8.0), Vec3::Z, 0.0);

        let near_size = projected_portal_size(&near, &camera, &projection, UVec2::splat(1000));
        let far_size = projected_portal_size(&far, &camera, &projection, UVec2::splat(1000));

        assert!(near_size.x > far_size.x);
        assert!(near_size.y > far_size.y);
    }

    #[test]
    fn portal_behind_camera_has_no_projected_footprint() {
        let projection = perspective();
        let portal = PortalFrame::from_surface(Vec3::new(0.0, 0.0, 2.0), Vec3::NEG_Z, 0.0);

        assert_eq!(
            projected_portal_size(&portal, &Transform::IDENTITY, &projection, UVec2::splat(1000)),
            Vec2::ZERO
        );
    }

    #[test]
    fn view_is_active_only_while_the_aperture_is_on_screen() {
        let (portals, key_a, _) = facing_pair();
        let projection = perspective();
        let looking_at_a = Transform::from_xyz(0.0, 1.0, 0.0);
        let looking_aside = Transform::from_xyz(-2.0, 1.0, 0.0).looking_to(Vec3::X, Vec3::Y);

        assert!(view_through_chain(&portals, &[key_a], &looking_at_a, &projection, UVec2::splat(1000)).is_some());
        assert!(view_through_chain(&portals, &[key_a], &looking_aside, &projection, UVec2::splat(1000)).is_none());
    }

    #[test]
    fn nested_view_continues_through_the_far_portal_but_never_its_own_exit() {
        let (portals, key_a, key_b) = facing_pair();
        let projection = perspective();
        let camera = Transform::from_xyz(0.0, 1.0, 0.0);

        let (mapped, _, _) = view_through_chain(&portals, &[key_a, key_a], &camera, &projection, UVec2::splat(1000))
            .expect("second look through A is in view");
        assert!(
            mapped.translation.distance(Vec3::new(0.0, 1.0, 16.0)) < 1e-4,
            "{mapped:?}"
        );
        assert!(view_through_chain(&portals, &[key_a, key_b], &camera, &projection, UVec2::splat(1000)).is_none());
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
