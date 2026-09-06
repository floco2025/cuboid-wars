use std::collections::{HashMap, HashSet, VecDeque};

use bevy::{
    camera::{Hdr, RenderTarget, visibility::RenderLayers},
    core_pipeline::{
        prepass::{DeferredPrepass, DepthPrepass},
        tonemapping::Tonemapping,
    },
    image::ImageSampler,
    math::Affine2,
    prelude::*,
    render::render_resource::TextureFormat,
};

use super::{
    PortalMap,
    projection::{PortalProjection, full_aperture, portal_camera_view},
    spawn::{PortalAssets, spawn_portal_visual},
    transform_sync::portal_surfaces_transform_sync_system,
};
use crate::{
    cameras::{
        MainCameraMarker, RearviewCameraMarker, SceneRenderTarget, SkyDiscRenderLayer, scene_render_target_system,
    },
    config::ClientSettings,
    constants::{LOCAL_PLAYER_RENDER_LAYER, REARVIEW_RENDER_LAYER},
    players::{local_player_camera_sync_system, local_player_rearview_viewport_system},
    schedule::ClientSet,
};
use common::{
    map::Carriers,
    physics::PortalFrame,
    protocol::{Portal, PortalEnd, PortalPairId},
};

const FIRST_RECURSIVE_RENDER_LAYER: usize = 5;
const MAX_PORTAL_VIEW_CAMERAS: usize = 64 - FIRST_RECURSIVE_RENDER_LAYER;
const MAX_PORTAL_REPLICAS: usize = 512;
// Texture sizes per axis; a view only ever needs the presenter's pixels.
const PORTAL_VIEW_AXIS_SIZES: [u32; 6] = [64, 128, 256, 512, 1024, 2048];
const RESOLUTION_SHRINK_THRESHOLD: f32 = 0.8;

type PortalKey = (PortalPairId, PortalEnd);

#[derive(Component)]
struct PortalViewCamera {
    presenter: Entity,
    chain: Vec<PortalKey>,
    target_surface: Entity,
    target: PortalViewTarget,
    previous_target: Option<PortalViewTarget>,
}

struct PortalViewTarget {
    size: UVec2,
    image: Handle<Image>,
    material: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
struct PortalRenderState {
    portals: Vec<Portal>,
    budget: Option<u8>,
    presenters: Vec<Entity>,
    roots: Vec<(Entity, PortalKey)>,
    spawned: Vec<Entity>,
}

struct PendingView {
    presenter: Entity,
    chain: Vec<PortalKey>,
    target_surface: Entity,
    recursion_remaining: u8,
}

// One view whose chain is fully on screen this frame: the camera it maps to
// and its entry aperture's on-screen footprint in pixels.
struct MappedView {
    entity: Entity,
    presenter: Entity,
    chain: Vec<PortalKey>,
    transform: Transform,
    projection: Projection,
    footprint: Vec2,
    rect: Rect,
}

pub fn portal_render_plugin(app: &mut App) {
    app.init_resource::<PortalRenderState>();
    app.add_systems(
        Update,
        (
            // Root selection reads this frame's camera poses, so a portal
            // entering view gets its camera the same frame.
            rebuild_portal_views_system
                .after(ClientSet::Network)
                .after(local_player_camera_sync_system)
                .after(local_player_rearview_viewport_system)
                .after(scene_render_target_system),
            update_portal_view_cameras_system.after(rebuild_portal_views_system),
        )
            .in_set(ClientSet::Camera),
    );
    app.add_systems(
        Update,
        portal_surfaces_transform_sync_system.in_set(ClientSet::CharacterSync),
    );
}

fn rebuild_portal_views_system(
    mut commands: Commands,
    main_camera: Query<(Entity, &Transform, &Projection, &Camera), With<MainCameraMarker>>,
    rearview_camera: Query<
        (Entity, &Transform, &Projection, &Camera),
        (With<RearviewCameraMarker>, Without<MainCameraMarker>),
    >,
    scene_target: Res<SceneRenderTarget>,
    portals: Res<PortalMap>,
    carriers: Res<Carriers>,
    fixed_time: Res<Time<Fixed>>,
    portal_assets: Res<PortalAssets>,
    client_settings: Res<ClientSettings>,
    mut state: ResMut<PortalRenderState>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut surface_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let Ok((main_entity, main_transform, main_projection, main_camera)) = main_camera.single() else {
        return;
    };
    let wire_portals = portals.wire_portals();
    let alpha = fixed_time.overstep_fraction();
    let budget = client_settings.rendering.portal_view_budget;
    let complete_portals: Vec<_> = wire_portals
        .iter()
        .copied()
        .filter(|portal| paired_portal(&portals, portal).is_some())
        .collect();
    let mut presenter_views = Vec::new();
    if main_camera.is_active {
        presenter_views.push((main_entity, main_transform, main_projection, main_camera));
    }
    if let Ok((entity, transform, projection, camera)) = rearview_camera.single()
        && camera.is_active
    {
        presenter_views.push((entity, transform, projection, camera));
    }
    let presenters: Vec<_> = presenter_views.iter().map(|(entity, ..)| *entity).collect();
    // Every complete root is built while they fit the budget, so the graph
    // changes only with the portals themselves; with more roots than budget,
    // each presenter keeps the largest on screen and the graph follows the view.
    let roots: Vec<_> = presenter_views
        .iter()
        .flat_map(|(entity, transform, projection, camera)| {
            let keys: Vec<PortalKey> = if complete_portals.len() <= usize::from(budget) {
                complete_portals
                    .iter()
                    .map(|portal| (portal.pair, portal.end))
                    .collect()
            } else {
                largest_visible_roots(
                    &portals,
                    &complete_portals,
                    &carriers,
                    alpha,
                    transform,
                    projection,
                    presenter_size(camera, scene_target.size),
                    usize::from(budget),
                )
            };
            keys.into_iter().map(|key| (*entity, key))
        })
        .collect();
    if state.portals == wire_portals
        && state.budget == Some(budget)
        && state.presenters == presenters
        && state.roots == roots
    {
        return;
    }

    for entity in state.spawned.drain(..) {
        commands.entity(entity).despawn();
    }
    for portal in &wire_portals {
        if let Some(info) = portals.get(&(portal.pair, portal.end))
            && let Ok(mut material) = surface_materials.get_mut(info.entity)
        {
            material.0 = portal_assets.material(portal.end);
        }
    }
    state.portals = wire_portals;
    state.budget = Some(budget);
    state.presenters = presenters;
    state.roots = roots;
    if budget == 0 || state.roots.is_empty() {
        return;
    }

    let mut over_budget = false;
    let mut replica_count = 0;
    let mut pending = VecDeque::new();
    let rearview_entity = rearview_camera.single().ok().map(|(entity, ..)| entity);
    let mut rearview_surfaces = HashMap::new();
    if let Some(rearview_entity) = rearview_entity.filter(|entity| state.presenters.contains(entity)) {
        let selected: HashSet<_> = state
            .roots
            .iter()
            .filter_map(|(presenter, key)| (*presenter == rearview_entity).then_some(*key))
            .collect();
        let mut rearview_portals = complete_portals.clone();
        rearview_portals.sort_by_key(|portal| {
            (
                !selected.contains(&(portal.pair, portal.end)),
                portal.pair.0,
                portal.end == PortalEnd::B,
            )
        });
        for portal in &rearview_portals {
            if replica_count >= MAX_PORTAL_REPLICAS {
                over_budget = true;
                break;
            }
            let replica = spawn_portal_visual(&mut commands, &portal_assets, portal, &carriers, REARVIEW_RENDER_LAYER);
            state.spawned.push(replica);
            rearview_surfaces.insert((portal.pair, portal.end), replica);
            replica_count += 1;
        }
    }
    for &(presenter, key) in &state.roots {
        if pending.len() >= MAX_PORTAL_VIEW_CAMERAS {
            over_budget = true;
            break;
        }
        let target_surface = if presenter == main_entity {
            portals.get(&key).map(|info| info.entity)
        } else {
            rearview_surfaces.get(&key).copied()
        };
        let Some(target_surface) = target_surface else {
            continue;
        };
        pending.push_back(PendingView {
            presenter,
            chain: vec![key],
            target_surface,
            recursion_remaining: budget.saturating_sub(1),
        });
    }

    let deferred = client_settings.rendering.opaque_renderer.is_deferred();
    let msaa = if deferred {
        Msaa::Off
    } else {
        Msaa::from_samples(client_settings.rendering.msaa_samples)
    };
    let mut camera_count = 0;
    while let Some(view) = pending.pop_front() {
        let hops = view.chain.len() - 1;
        let child_layer = FIRST_RECURSIVE_RENDER_LAYER + camera_count;
        // Bucket images stay immutable because each is both a camera target and a sampled portal texture.
        let target = create_portal_view_target(&mut images, &mut materials, UVec2::splat(PORTAL_VIEW_AXIS_SIZES[0]));
        let initial_image = target.image.clone();
        let mut camera = commands.spawn((
            PortalViewCamera {
                presenter: view.presenter,
                chain: view.chain.clone(),
                target_surface: view.target_surface,
                target,
                previous_target: None,
            },
            SkyDiscRenderLayer(child_layer),
            Camera3d::default(),
            Camera {
                order: -(hops as isize) - 1,
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
            if (portal.pair, portal.end) == own_exit {
                continue;
            }
            if replica_count >= MAX_PORTAL_REPLICAS {
                over_budget = true;
                break;
            }
            let replica = spawn_portal_visual(&mut commands, &portal_assets, portal, &carriers, child_layer);
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
            chain.push((portal.pair, portal.end));
            pending.push_back(PendingView {
                presenter: view.presenter,
                chain,
                target_surface: replica,
                recursion_remaining: view.recursion_remaining - 1,
            });
        }
    }

    if over_budget {
        warn!(
            "portal render graph reached its safety cap ({camera_count} cameras, {replica_count} surfaces); deeper views are omitted"
        );
    }
}

fn presenter_size(camera: &Camera, scene_size: UVec2) -> UVec2 {
    camera
        .viewport
        .as_ref()
        .map_or(scene_size, |viewport| viewport.physical_size)
}

fn largest_visible_roots(
    portals: &PortalMap,
    complete_portals: &[Portal],
    carriers: &Carriers,
    alpha: f32,
    transform: &Transform,
    projection: &Projection,
    size: UVec2,
    budget: usize,
) -> Vec<PortalKey> {
    let mut roots: Vec<_> = complete_portals
        .iter()
        .filter_map(|portal| {
            let key = (portal.pair, portal.end);
            let (_, _, footprint, _) =
                view_through_chain(portals, &[key], carriers, alpha, transform, projection, size)?;
            Some((key, footprint.x * footprint.y))
        })
        .collect();
    roots.sort_by(|(a_key, a_area), (b_key, b_area)| {
        b_area
            .total_cmp(a_area)
            .then(a_key.0.0.cmp(&b_key.0.0))
            .then((a_key.1 == PortalEnd::B).cmp(&(b_key.1 == PortalEnd::B)))
    });
    roots.truncate(budget);
    roots.sort_by_key(|(key, _)| (key.0.0, key.1 == PortalEnd::B));
    roots.into_iter().map(|(key, _)| key).collect()
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
    presenters: Query<
        (&Transform, &Projection, &Camera),
        (
            Or<(With<MainCameraMarker>, With<RearviewCameraMarker>)>,
            Without<PortalViewCamera>,
        ),
    >,
    scene_target: Res<SceneRenderTarget>,
    portals: Res<PortalMap>,
    carriers: Res<Carriers>,
    fixed_time: Res<Time<Fixed>>,
    portal_assets: Res<PortalAssets>,
    client_settings: Res<ClientSettings>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut view_cameras: Query<(
        Entity,
        &mut PortalViewCamera,
        &mut Camera,
        &mut RenderTarget,
        &mut Transform,
        &mut Projection,
    )>,
    mut surface_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let alpha = fixed_time.overstep_fraction();
    let mut mapped: Vec<MappedView> = view_cameras
        .iter()
        .filter_map(|(entity, view, ..)| {
            let (presenter_transform, presenter_projection, presenter_camera) = presenters.get(view.presenter).ok()?;
            if !presenter_camera.is_active {
                return None;
            }
            // The main camera fills the scene image; the rearview draws into its viewport.
            let presenter_size = presenter_size(presenter_camera, scene_target.size);
            let (transform, projection, footprint, rect) = view_through_chain(
                &portals,
                &view.chain,
                &carriers,
                alpha,
                presenter_transform,
                presenter_projection,
                presenter_size,
            )?;
            Some(MappedView {
                entity,
                presenter: view.presenter,
                chain: view.chain.clone(),
                transform,
                projection,
                footprint,
                rect,
            })
        })
        .collect();
    // Each presenting camera spends its own budget, so a deep forward corridor
    // never starves the mirror.
    mapped.sort_by_key(|view| view.presenter);
    let budget = client_settings.rendering.portal_view_budget as usize;
    let mut admitted: HashMap<Entity, usize> = HashMap::new();
    let mut start = 0;
    for group in mapped.chunk_by(|a, b| a.presenter == b.presenter) {
        admitted.extend(
            admit_views(group, budget)
                .into_iter()
                .map(|index| (group[index].entity, start + index)),
        );
        start += group.len();
    }

    for (entity, mut view, mut camera, mut render_target, mut transform, mut projection) in &mut view_cameras {
        let admitted_view = admitted.get(&entity).map(|&index| &mapped[index]);
        camera.is_active = admitted_view.is_some();
        let surface_material = match admitted_view {
            Some(mapped_view) => {
                *transform = mapped_view.transform;
                *projection = mapped_view.projection.clone();
                let desired_size = adaptive_portal_resolution(mapped_view.footprint, view.target.size);
                if desired_size != view.target.size {
                    let next = match view.previous_target.take() {
                        Some(previous) if previous.size == desired_size => previous,
                        _ => create_portal_view_target(&mut images, &mut materials, desired_size),
                    };
                    let previous = std::mem::replace(&mut view.target, next);
                    view.previous_target = Some(previous);
                    *render_target = RenderTarget::Image(view.target.image.clone().into());
                }
                let target = &view.target;
                let uv_transform = aperture_uv_transform(mapped_view.rect);
                if materials
                    .get(&target.material)
                    .is_some_and(|material| material.uv_transform != uv_transform)
                    && let Some(mut material) = materials.get_mut(&target.material)
                {
                    material.uv_transform = uv_transform;
                }
                target.material.clone()
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

// Admits views largest on screen first until the budget is spent. A view is
// only ever seen through its parent view (its chain minus the last hop), so
// it needs that parent admitted too. Returns indices into `views`.
fn admit_views(views: &[MappedView], budget: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..views.len()).collect();
    let area = |index: usize| views[index].footprint.x * views[index].footprint.y;
    order.sort_by(|&a, &b| {
        area(b)
            .total_cmp(&area(a))
            .then(views[a].chain.len().cmp(&views[b].chain.len()))
    });
    let mut admitted_chains: HashSet<&[PortalKey]> = HashSet::new();
    let mut admitted = Vec::new();
    for index in order {
        if admitted.len() >= budget {
            break;
        }
        let chain = views[index].chain.as_slice();
        let Some((_, parent)) = chain.split_last() else {
            continue;
        };
        if !parent.is_empty() && !admitted_chains.contains(parent) {
            continue;
        }
        admitted_chains.insert(chain);
        admitted.push(index);
    }
    admitted
}

// Maps a presenting camera through every hop of `chain`: the eye through each
// entry to its exit, rendering only the part of each aperture that is on
// screen in the view before it. `None` when a hop is invalid or off screen —
// computed from this frame's camera rather than read back from visibility,
// so activation never lags a frame. Returns the final view, the last hop's
// footprint in presenter pixels, and the aperture rectangle it renders.
fn view_through_chain(
    portals: &PortalMap,
    chain: &[PortalKey],
    carriers: &Carriers,
    alpha: f32,
    presenter_transform: &Transform,
    presenter_projection: &Projection,
    presenter_size: UVec2,
) -> Option<(Transform, Projection, Vec2, Rect)> {
    let mut view_transform = *presenter_transform;
    let mut view_projection = presenter_projection.clone();
    let mut footprint = presenter_size.as_vec2();
    let mut rect = full_aperture();
    for key in chain {
        let entry = &portals.get(key)?.portal;
        let exit = paired_portal(portals, entry)?;
        let entry_frame = PortalFrame::from_portal_between(entry, carriers, alpha);
        let exit_frame = PortalFrame::from_portal_between(exit, carriers, alpha);
        let visible = visible_aperture(&entry_frame, &view_transform, &view_projection, footprint)?;
        footprint = visible.footprint;
        rect = visible.rect;
        let (next_transform, next_projection) = portal_camera_view(
            view_transform.translation,
            &entry_frame,
            &exit_frame,
            presenter_projection.far(),
            rect,
        )?;
        view_transform = next_transform;
        view_projection = Projection::custom(next_projection);
    }
    Some((view_transform, view_projection, footprint, rect))
}

struct VisibleAperture {
    // Pixels of the view the visible part covers: ranking and texture size.
    footprint: Vec2,
    // The part of the aperture to render, in aperture units.
    rect: Rect,
}

// The on-screen part of an aperture: the aperture rectangle clipped by the
// view frustum, as its pixel footprint and the aperture-space rectangle it
// spans. Rendering only that rectangle keeps the texture at screen density
// however close the eye gets, since the view never needs more pixels than
// the screen has. `None` when nothing is on screen.
fn visible_aperture(
    frame: &PortalFrame,
    camera_transform: &Transform,
    projection: &Projection,
    view_size: Vec2,
) -> Option<VisibleAperture> {
    let clip_from_world = projection.get_clip_from_view() * camera_transform.to_matrix().inverse();
    let aperture = full_aperture();
    let mut polygon: Vec<Vec3> = [
        Vec2::new(aperture.min.x, aperture.min.y),
        Vec2::new(aperture.max.x, aperture.min.y),
        aperture.max,
        Vec2::new(aperture.min.x, aperture.max.y),
    ]
    .into_iter()
    .map(|corner| frame.center + frame.right * corner.x + frame.up * corner.y)
    .collect();
    for plane in frustum_planes(&clip_from_world) {
        polygon = clip_polygon(&polygon, plane);
        if polygon.is_empty() {
            return None;
        }
    }

    let mut rect = Rect::EMPTY;
    let mut ndc = Rect::EMPTY;
    for vertex in polygon {
        let local = vertex - frame.center;
        rect = rect.union_point(Vec2::new(local.dot(frame.right), local.dot(frame.up)));
        let clip = clip_from_world * vertex.extend(1.0);
        ndc = ndc.union_point((clip.truncate().xy() / clip.w).clamp(Vec2::splat(-1.0), Vec2::splat(1.0)));
    }
    if rect.is_empty() || ndc.is_empty() {
        return None;
    }
    Some(VisibleAperture {
        footprint: ndc.size() * 0.5 * view_size,
        rect,
    })
}

// The view's four side planes and its near plane, read off the clip matrix
// rows; a point is inside where `plane · (p, 1) >= 0`. Bevy's reverse Z puts
// the near plane at clip z = w.
fn frustum_planes(clip_from_world: &Mat4) -> [Vec4; 5] {
    let row = |index| clip_from_world.row(index);
    [
        row(3) + row(0),
        row(3) - row(0),
        row(3) + row(1),
        row(3) - row(1),
        row(3) - row(2),
    ]
}

// Sutherland–Hodgman against one plane.
fn clip_polygon(polygon: &[Vec3], plane: Vec4) -> Vec<Vec3> {
    let mut clipped = Vec::with_capacity(polygon.len() + 1);
    for (index, &start) in polygon.iter().enumerate() {
        let end = polygon[(index + 1) % polygon.len()];
        let start_distance = plane.dot(start.extend(1.0));
        let end_distance = plane.dot(end.extend(1.0));
        if start_distance >= 0.0 {
            clipped.push(start);
        }
        if (start_distance < 0.0) != (end_distance < 0.0) {
            clipped.push(start.lerp(end, start_distance / (start_distance - end_distance)));
        }
    }
    clipped
}

// Maps the disc's UVs (the whole aperture, v downward) onto the rendered `rect`.
fn aperture_uv_transform(rect: Rect) -> Affine2 {
    let aperture = full_aperture();
    let size = rect.size();
    Affine2::from_scale_angle_translation(
        aperture.size() / size,
        0.0,
        Vec2::new(
            (aperture.min.x - rect.min.x) / size.x,
            (rect.max.y - aperture.max.y) / size.y,
        ),
    )
}

fn adaptive_portal_resolution(footprint: Vec2, current: UVec2) -> UVec2 {
    UVec2::new(axis_size(footprint.x, current.x), axis_size(footprint.y, current.y))
}

// Grows straight to the first step covering the demand; shrinks only once the
// demand sits well inside the step below, so a portal hovering around a
// boundary does not flip textures every frame.
fn axis_size(demand: f32, current: u32) -> u32 {
    let desired = PORTAL_VIEW_AXIS_SIZES
        .iter()
        .position(|&size| size as f32 >= demand)
        .unwrap_or(PORTAL_VIEW_AXIS_SIZES.len() - 1);
    let mut index = PORTAL_VIEW_AXIS_SIZES
        .iter()
        .position(|&size| size == current)
        .unwrap_or(0);
    if desired > index {
        index = desired;
    } else {
        while index > desired && demand < PORTAL_VIEW_AXIS_SIZES[index - 1] as f32 * RESOLUTION_SHRINK_THRESHOLD {
            index -= 1;
        }
    }
    PORTAL_VIEW_AXIS_SIZES[index]
}

fn paired_end(end: PortalEnd) -> PortalEnd {
    match end {
        PortalEnd::A => PortalEnd::B,
        PortalEnd::B => PortalEnd::A,
    }
}

fn paired_key((pair, end): PortalKey) -> PortalKey {
    (pair, paired_end(end))
}

fn paired_portal<'a>(portals: &'a PortalMap, portal: &Portal) -> Option<&'a Portal> {
    portals
        .get(&paired_key((portal.pair, portal.end)))
        .map(|info| &info.portal)
}

#[cfg(test)]
mod tests {
    use common::protocol::CarrierId;
    use std::f32::consts::FRAC_PI_2;

    use super::*;
    use crate::portals::PortalInfo;
    use common::constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH};

    fn perspective() -> Projection {
        let mut projection = Projection::Perspective(PerspectiveProjection {
            fov: FRAC_PI_2,
            ..default()
        });
        projection.update(1000.0, 1000.0);
        projection
    }

    fn portal(pair: u32, end: PortalEnd, pos: Vec3, normal: Vec3) -> Portal {
        Portal {
            pair: PortalPairId(pair),
            end,
            pos: pos.into(),
            nx: normal.x,
            ny: normal.y,
            nz: normal.z,
            yaw: 0.0,
            carrier: CarrierId::WORLD,
        }
    }

    fn portal_map(portals: &[Portal]) -> PortalMap {
        let mut map = PortalMap::default();
        for portal in portals {
            map.insert(
                (portal.pair, portal.end),
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
        (portal_map(&[a, b]), (a.pair, a.end), (b.pair, b.end))
    }

    #[test]
    fn projected_portal_footprint_shrinks_with_distance() {
        let camera = Transform::IDENTITY;
        let projection = perspective();
        let near = PortalFrame::from_surface(Vec3::new(0.0, 0.0, -2.0), Vec3::Z, 0.0);
        let far = PortalFrame::from_surface(Vec3::new(0.0, 0.0, -8.0), Vec3::Z, 0.0);

        let near_size = visible_aperture(&near, &camera, &projection, Vec2::splat(1000.0))
            .expect("near portal is on screen")
            .footprint;
        let far_size = visible_aperture(&far, &camera, &projection, Vec2::splat(1000.0))
            .expect("far portal is on screen")
            .footprint;

        assert!(near_size.x > far_size.x);
        assert!(near_size.y > far_size.y);
    }

    #[test]
    fn portal_behind_camera_has_no_projected_footprint() {
        let projection = perspective();
        let portal = PortalFrame::from_surface(Vec3::new(0.0, 0.0, 2.0), Vec3::NEG_Z, 0.0);

        assert!(visible_aperture(&portal, &Transform::IDENTITY, &projection, Vec2::splat(1000.0)).is_none());
    }

    #[test]
    fn distant_aperture_renders_whole_and_close_aperture_renders_only_the_visible_part() {
        let projection = perspective();
        let portal = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);

        let distant = visible_aperture(
            &portal,
            &Transform::from_xyz(0.0, 0.0, 5.0),
            &projection,
            Vec2::splat(1000.0),
        )
        .expect("distant portal is on screen");
        assert!(distant.footprint.y < 1000.0);
        assert!(
            (distant.rect.min - full_aperture().min).length() < 1e-3,
            "{:?}",
            distant.rect
        );
        assert!(
            (distant.rect.max - full_aperture().max).length() < 1e-3,
            "{:?}",
            distant.rect
        );

        // Half a metre out with a 90° lens sees ±0.5 m of the aperture.
        let close = visible_aperture(
            &portal,
            &Transform::from_xyz(0.0, 0.0, 0.5),
            &projection,
            Vec2::splat(1000.0),
        )
        .expect("close portal is on screen");
        assert_eq!(close.footprint, Vec2::splat(1000.0));
        assert!((close.rect.min - Vec2::splat(-0.5)).length() < 1e-3, "{:?}", close.rect);
        assert!((close.rect.max - Vec2::splat(0.5)).length() < 1e-3, "{:?}", close.rect);
    }

    #[test]
    fn aperture_corner_behind_the_eye_is_clipped_not_abandoned() {
        let projection = perspective();
        let portal = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
        // Hugging the wall, turned along it: the near corner is behind the eye plane.
        let camera = Transform::from_xyz(0.5, 0.0, 0.15).looking_to(Vec3::new(-1.0, 0.0, -0.3).normalize(), Vec3::Y);

        let visible = visible_aperture(&portal, &camera, &projection, Vec2::splat(1000.0))
            .expect("far side of the aperture is on screen");
        assert!(visible.rect.max.x < PORTAL_HALF_WIDTH, "{:?}", visible.rect);
        assert!(visible.rect.min.x >= -PORTAL_HALF_WIDTH - 1e-4, "{:?}", visible.rect);
        assert!(
            visible.footprint.x > 0.0 && visible.footprint.x <= 1000.0,
            "{:?}",
            visible.footprint
        );
        assert!(
            visible.footprint.y > 0.0 && visible.footprint.y <= 1000.0,
            "{:?}",
            visible.footprint
        );
    }

    #[test]
    fn uv_transform_maps_the_disc_onto_the_rendered_rect() {
        let identity = aperture_uv_transform(full_aperture());
        assert!((identity.transform_point2(Vec2::new(0.25, 0.75)) - Vec2::new(0.25, 0.75)).length() < 1e-6);

        // The upper-right quadrant of the aperture (disc UV u > 0.5, v < 0.5).
        let quadrant = aperture_uv_transform(Rect::new(0.0, 0.0, PORTAL_HALF_WIDTH, PORTAL_HALF_HEIGHT));
        assert!((quadrant.transform_point2(Vec2::new(0.5, 0.5)) - Vec2::new(0.0, 1.0)).length() < 1e-6);
        assert!((quadrant.transform_point2(Vec2::new(1.0, 0.0)) - Vec2::new(1.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn view_is_active_only_while_the_aperture_is_on_screen() {
        let (portals, key_a, _) = facing_pair();
        let projection = perspective();
        let looking_at_a = Transform::from_xyz(0.0, 1.0, 0.0);
        let looking_aside = Transform::from_xyz(-2.0, 1.0, 0.0).looking_to(Vec3::X, Vec3::Y);

        let carriers = Carriers::default();
        assert!(
            view_through_chain(
                &portals,
                &[key_a],
                &carriers,
                0.0,
                &looking_at_a,
                &projection,
                UVec2::splat(1000)
            )
            .is_some()
        );
        assert!(
            view_through_chain(
                &portals,
                &[key_a],
                &carriers,
                0.0,
                &looking_aside,
                &projection,
                UVec2::splat(1000)
            )
            .is_none()
        );
    }

    #[test]
    fn nested_view_continues_through_the_far_portal_but_never_its_own_exit() {
        let (portals, key_a, key_b) = facing_pair();
        let projection = perspective();
        let camera = Transform::from_xyz(0.0, 1.0, 0.0);

        let carriers = Carriers::default();
        let (mapped, _, _, _) = view_through_chain(
            &portals,
            &[key_a, key_a],
            &carriers,
            0.0,
            &camera,
            &projection,
            UVec2::splat(1000),
        )
        .expect("second look through A is in view");
        assert!(
            mapped.translation.distance(Vec3::new(0.0, 1.0, 16.0)) < 1e-4,
            "{mapped:?}"
        );
        assert!(
            view_through_chain(
                &portals,
                &[key_a, key_b],
                &carriers,
                0.0,
                &camera,
                &projection,
                UVec2::splat(1000)
            )
            .is_none()
        );
    }

    #[test]
    fn texture_size_follows_each_axis_of_the_footprint() {
        assert_eq!(
            adaptive_portal_resolution(Vec2::new(1000.0, 300.0), UVec2::splat(64)),
            UVec2::new(1024, 512)
        );
        assert_eq!(
            adaptive_portal_resolution(Vec2::ZERO, UVec2::splat(2048)),
            UVec2::splat(64)
        );
        assert_eq!(
            adaptive_portal_resolution(Vec2::splat(4000.0), UVec2::splat(64)),
            UVec2::splat(2048)
        );
    }

    fn mapped(chain: &[PortalKey], footprint: Vec2) -> MappedView {
        MappedView {
            entity: Entity::PLACEHOLDER,
            presenter: Entity::PLACEHOLDER,
            chain: chain.to_vec(),
            transform: Transform::IDENTITY,
            projection: Projection::default(),
            footprint,
            rect: full_aperture(),
        }
    }

    #[test]
    fn budget_admits_the_largest_views_first() {
        let views = [
            mapped(&[(PortalPairId(1), PortalEnd::A)], Vec2::new(50.0, 100.0)),
            mapped(&[(PortalPairId(2), PortalEnd::A)], Vec2::new(200.0, 400.0)),
            mapped(&[(PortalPairId(3), PortalEnd::A)], Vec2::new(100.0, 200.0)),
        ];
        assert_eq!(admit_views(&views, 2), vec![1, 2]);
        assert_eq!(admit_views(&views, 0), Vec::<usize>::new());
    }

    #[test]
    fn nested_view_is_admitted_only_under_its_parent() {
        let a = (PortalPairId(1), PortalEnd::A);
        let b = (PortalPairId(2), PortalEnd::A);
        let views = [
            mapped(&[a], Vec2::new(10.0, 20.0)),
            mapped(&[a, a], Vec2::new(10.0, 20.0)),
            mapped(&[b], Vec2::new(100.0, 200.0)),
        ];
        assert_eq!(admit_views(&views, 1), vec![2]);
        assert_eq!(admit_views(&views, 2), vec![2, 0]);
        assert_eq!(admit_views(&views, 3), vec![2, 0, 1]);
    }

    #[test]
    fn orphaned_nested_view_is_skipped() {
        let a = (PortalPairId(1), PortalEnd::A);
        let b = (PortalPairId(2), PortalEnd::A);
        let views = [
            mapped(&[a, a], Vec2::new(300.0, 600.0)),
            mapped(&[b], Vec2::new(10.0, 20.0)),
        ];
        assert_eq!(admit_views(&views, 2), vec![1]);
    }

    #[test]
    fn texture_size_shrink_has_hysteresis() {
        assert_eq!(axis_size(450.0, 1024), 1024);
        assert_eq!(axis_size(400.0, 1024), 512);
        assert_eq!(axis_size(300.0, 128), 512);
    }

    #[test]
    fn root_selection_uses_visible_size_not_portal_pair_order() {
        let portals = [
            portal(1, PortalEnd::A, Vec3::new(0.0, 0.0, -8.0), Vec3::Z),
            portal(1, PortalEnd::B, Vec3::new(0.0, 0.0, 20.0), Vec3::NEG_Z),
            portal(2, PortalEnd::A, Vec3::new(0.0, 0.0, -2.0), Vec3::Z),
            portal(2, PortalEnd::B, Vec3::new(0.0, 0.0, 24.0), Vec3::NEG_Z),
        ];
        let map = portal_map(&portals);

        assert_eq!(
            largest_visible_roots(
                &map,
                &portals,
                &Carriers::default(),
                0.0,
                &Transform::IDENTITY,
                &perspective(),
                UVec2::splat(1000),
                1,
            ),
            vec![(PortalPairId(2), PortalEnd::A)]
        );
    }

    #[test]
    fn recursive_camera_layers_stay_within_render_layer_capacity() {
        assert_eq!(FIRST_RECURSIVE_RENDER_LAYER + MAX_PORTAL_VIEW_CAMERAS - 1, 63);
    }
}
