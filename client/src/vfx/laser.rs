use std::time::Duration;

use bevy::{audio::SpatialScale, light::NotShadowCaster, prelude::*};

use crate::{
    actors::{ActorMap, AimJointMarker, AimRig},
    config::{AssetSet, ClientSettings},
    constants::*,
    players::PlayerMap,
};
use common::{
    config::{GameplayConfig, HitboxConfig},
    constants::TICK_SECS,
    physics::CollisionWorld,
    protocol::{ActorId, ActorMarker, PlateState, PlayerId, Position, ServerTick},
};

// Angular speeds (rad/s) of the endpoint wander's per-axis sines —
// incommensurate so the combined path never visibly repeats, ~1–1.5 Hz so
// the drift reads as searching, not strobing.
const WANDER_SPEEDS: Vec3 = Vec3::new(7.3, 9.4, 5.1);
// Golden angle: spreads per-beam phases so simultaneous beams desync.
const WANDER_PHASE_STEP: f32 = 2.399;

#[derive(Component)]
pub struct LaserBeam {
    pub actor: ActorId,
    pub target: PlayerId,
    pub started_tick: u32,
}

// One entity per burst: the cylinder and the fire sound, which stops when
// the beam despawns. `elapsed_secs` into the burst starts the sound
// mid-way so a late cue stays in step.
fn spawn_laser_beam(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    settings: &ClientSettings,
    beam: LaserBeam,
    kind: &str,
    position: Vec3,
    elapsed_secs: f32,
) {
    let brightness = LASER_EMISSIVE;
    // Opaque hot-red emissive core, like the projectile body — this app's
    // Blend materials render wrong, so no translucency; bloom supplies the
    // glow. The base color stays fixed red — only the emissive scales with
    // the brightness knob, so cranking the glow can't wash the surface to
    // white. Per-instance assets free themselves when the beam despawns.
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.15, 0.15),
        emissive: LinearRgba::rgb(brightness, 0.08 * brightness, 0.08 * brightness),
        ..default()
    });
    commands.spawn((
        beam,
        // Unit-height cylinder with the real radius baked in; the update
        // system scales Y to the live beam length.
        Mesh3d(meshes.add(Cylinder::new(LASER_BEAM_RADIUS, 1.0))),
        MeshMaterial3d(material),
        NotShadowCaster,
        Transform::from_translation(position),
        // Hidden until the first update frame anchors it.
        Visibility::Hidden,
        AudioPlayer::new(asset_server.load(asset_set.actor_sound(kind, "fire").to_owned())),
        PlaybackSettings::ONCE
            .with_start_position(Duration::from_secs_f32(elapsed_secs))
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(settings.audio.spatial_distance_scale)),
    ));
}

// Spawn a beam for every burst the snapshot reports and despawn every beam
// whose burst ended; a retargeted burst keeps its entity and sound.
pub fn laser_beams_sync_system(
    mut commands: Commands,
    actors: Res<ActorMap>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    positions: Query<&Position, With<ActorMarker>>,
    mut beams: Query<(Entity, &mut LaserBeam)>,
) {
    if actors.peaceful {
        for (entity, _) in &beams {
            commands.entity(entity).despawn();
        }
        return;
    }
    for (entity, mut beam) in &mut beams {
        let active = actors
            .get(&beam.actor)
            .and_then(|actor| actor.beam.active(tick.0))
            .filter(|active| active.started_tick == beam.started_tick && players.get(&active.target).is_some());
        if let Some(active) = active {
            beam.target = active.target;
        } else {
            commands.entity(entity).despawn();
        }
    }
    for (id, actor) in actors.iter() {
        let Some(active) = actor
            .beam
            .active(tick.0)
            .filter(|beam| players.get(&beam.target).is_some())
        else {
            continue;
        };
        if beams
            .iter()
            .any(|(_, beam)| beam.actor == *id && beam.started_tick == active.started_tick)
        {
            continue;
        }
        let elapsed_ticks = (tick.0.wrapping_sub(active.started_tick) as i32).max(0);
        spawn_laser_beam(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset_server,
            &asset_set,
            &settings,
            LaserBeam {
                actor: *id,
                target: active.target,
                started_tick: active.started_tick,
            },
            &actor.kind,
            positions.get(actor.entity).map_or(Vec3::ZERO, |pos| Vec3::from(*pos)),
            elapsed_ticks as f32 * TICK_SECS,
        );
    }
}

// Beam anchors use this frame's interpolated character positions.
pub fn laser_beam_update_system(
    mut commands: Commands,
    time: Res<Time>,
    actors: Res<ActorMap>,
    players: Res<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    collision_world: Res<CollisionWorld>,
    plates: Res<PlateState>,
    endpoints: Query<(&Transform, Option<&AimRig>), (Without<LaserBeam>, Without<AimJointMarker>)>,
    mut joints: Query<&mut Transform, With<AimJointMarker>>,
    mut beams: Query<(Entity, &LaserBeam, &mut Transform, &mut Visibility), Without<AimJointMarker>>,
) {
    let target_hitbox = gameplay_config.player.physics().hitbox;
    for (entity, beam, mut transform, mut visibility) in &mut beams {
        let anchors = actors
            .get(&beam.actor)
            .zip(players.get(&beam.target))
            .and_then(|(actor, target)| {
                Some((
                    endpoints.get(actor.entity).ok()?,
                    endpoints.get(target.entity).ok()?,
                    gameplay_config.expect_actor(&actor.kind),
                ))
            });
        let Some(((actor_transform, aim_rig), (target_transform, _), actor_config)) = anchors else {
            commands.entity(entity).despawn();
            continue;
        };
        let frame = aim_rig.and_then(|rig| {
            rig.frame(actor_transform, |entity| {
                endpoints.get(entity).ok().map(|(transform, _)| *transform)
            })
        });
        if aim_rig.is_some() && frame.is_none() {
            *visibility = Visibility::Hidden;
            continue;
        }
        let articulated = aim_rig.zip(frame.as_ref());
        let origin = articulated.map_or_else(
            || actor_transform.translation + Vec3::Y * actor_config.beam_origin_y_offset(),
            |(rig, frame)| rig.pivot(frame),
        );
        let aim_local = beam_target_local(beam.actor, &target_hitbox, time.elapsed_secs());
        let target = target_transform.translation + target_transform.rotation * aim_local;
        let full_length = origin.distance(target);
        if full_length <= f32::EPSILON {
            *visibility = Visibility::Hidden;
            continue;
        }
        let direction = (target - origin) / full_length;
        if let Some((rig, frame)) = articulated {
            rig.aim(frame, direction, &mut joints);
        }
        // Damage and beam clipping share the active-field filter.
        let length = collision_world
            .attack_surface_along_ray(origin, direction, full_length, &plates.open_barrier_kinds)
            .map_or(full_length, |hit| hit.point.distance(origin));
        let muzzle_distance = articulated.map_or(0.0, |(rig, frame)| rig.muzzle_distance(frame));
        let Some(pose) = beam_pose(origin, direction, length, muzzle_distance) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *transform = pose;
        *visibility = Visibility::Visible;
    }
}

// Where the beam lands in the target's frame: `LASER_AIM_HEIGHT_FRACTION` up
// the hitbox, plus the wander. Damage is unaffected — the server burns the
// hitbox center.
pub(crate) fn beam_target_local(actor: ActorId, hitbox: &HitboxConfig, elapsed_secs: f32) -> Vec3 {
    let anchor_y = hitbox.bottom_offset + LASER_AIM_HEIGHT_FRACTION * hitbox.height;
    Vec3::new(0.0, anchor_y, 0.0) + wander_offset(actor, hitbox, elapsed_secs)
}

// Drift the hit point smoothly so the beam reads as searching rather than
// pinned, sized per axis so it stays inside the fraction-scaled hitbox
// (width and depth differ). Applied before the length computation and the
// wall clip so a blocked beam wanders too.
fn wander_offset(actor: ActorId, hitbox: &HitboxConfig, elapsed_secs: f32) -> Vec3 {
    let phase = actor.0 as f32 * WANDER_PHASE_STEP;
    let half_extents = Vec3::new(hitbox.width / 2.0, hitbox.height / 2.0, hitbox.depth / 2.0);
    Vec3::new(
        (elapsed_secs * WANDER_SPEEDS.x + phase).sin(),
        (elapsed_secs * WANDER_SPEEDS.y + 2.0 * phase).sin(),
        (elapsed_secs * WANDER_SPEEDS.z + 3.0 * phase).sin(),
    ) * half_extents
        * Vec3::new(
            LASER_ENDPOINT_WANDER_WIDTH_FRACTION,
            LASER_ENDPOINT_WANDER_HEIGHT_FRACTION,
            LASER_ENDPOINT_WANDER_WIDTH_FRACTION,
        )
}

// The unit cylinder's pose from the muzzle to the clip point, `None` when
// the clip lands before the muzzle. Lengths are measured from the pivot so
// a muzzle extending through cover cannot bypass it.
fn beam_pose(origin: Vec3, direction: Vec3, length: f32, muzzle_distance: f32) -> Option<Transform> {
    (length > muzzle_distance).then(|| Transform {
        translation: origin + direction * ((length + muzzle_distance) / 2.0),
        rotation: Quat::from_rotation_arc(Vec3::Y, direction),
        scale: Vec3::new(1.0, length - muzzle_distance, 1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actors::ActorInfo, players::PlayerInfo};
    use bevy::audio::PlaybackMode;
    use common::protocol::{ActorBeam, Health, Player, PlayerMoveIntent};

    #[test]
    fn wander_offset_stays_inside_the_fraction_scaled_hitbox() {
        let hitbox = HitboxConfig {
            width: 1.0,
            height: 2.0,
            depth: 0.5,
            bottom_offset: 0.0,
        };
        let bound = Vec3::new(
            0.5 * LASER_ENDPOINT_WANDER_WIDTH_FRACTION,
            LASER_ENDPOINT_WANDER_HEIGHT_FRACTION,
            0.25 * LASER_ENDPOINT_WANDER_WIDTH_FRACTION,
        );
        for step in 0..500 {
            let offset = wander_offset(ActorId(3), &hitbox, step as f32 * 0.01);
            assert!(offset.abs().cmple(bound).all(), "wander left the hitbox: {offset}");
        }
        assert_ne!(
            wander_offset(ActorId(1), &hitbox, 0.0),
            wander_offset(ActorId(2), &hitbox, 0.0)
        );
    }

    #[test]
    fn beam_pose_spans_the_muzzle_to_the_clip_point() {
        let origin = Vec3::new(1.0, 2.0, 3.0);
        let direction = Vec3::new(0.0, 0.6, 0.8);
        let pose = beam_pose(origin, direction, 5.0, 0.5).expect("beam pose missing");
        assert!(
            pose.transform_point(Vec3::NEG_Y * 0.5)
                .abs_diff_eq(origin + direction * 0.5, 1e-5)
        );
        assert!(
            pose.transform_point(Vec3::Y * 0.5)
                .abs_diff_eq(origin + direction * 5.0, 1e-5)
        );
        assert!(beam_pose(origin, direction, 0.4, 0.5).is_none());
    }

    fn sync_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<AudioSource>()
            .insert_resource(AssetSet::load_default().expect("asset set rejected"))
            .insert_resource(ClientSettings::load_default().expect("client settings rejected"))
            .init_resource::<ServerTick>()
            .init_resource::<ActorMap>()
            .init_resource::<PlayerMap>()
            .add_systems(Update, laser_beams_sync_system);
        app
    }

    #[test]
    fn beam_keeps_one_effect_and_sound_through_retargeting() {
        let mut app = sync_app();
        for id in [PlayerId(1), PlayerId(2)] {
            let entity = app.world_mut().spawn_empty().id();
            let player = Player::new(
                "Player".into(),
                Position::default(),
                PlayerMoveIntent::default(),
                0.0,
                0,
                Health(500.0),
            );
            app.world_mut()
                .resource_mut::<PlayerMap>()
                .insert(id, PlayerInfo::from_snapshot(entity, &player, 0));
        }
        let id = ActorId(1);
        let entity = app.world_mut().spawn((ActorMarker, Position::default())).id();
        let mut actor = ActorInfo {
            entity,
            kind: "turret".into(),
            anchor: None,
            beam: Default::default(),
        };
        actor.beam.apply(
            1,
            Some(ActorBeam {
                target: PlayerId(1),
                started_tick: 1,
                remaining_secs: 2.0,
            }),
        );
        app.world_mut().resource_mut::<ActorMap>().insert(id, actor);
        app.world_mut().resource_mut::<ServerTick>().0 = 16;
        app.update();
        let beam = app
            .world_mut()
            .query_filtered::<Entity, With<LaserBeam>>()
            .single(app.world())
            .expect("beam missing");
        assert!(app.world().get::<AudioPlayer>(beam).is_some());
        let playback = app
            .world()
            .get::<PlaybackSettings>(beam)
            .expect("beam playback missing");
        assert!(matches!(playback.mode, PlaybackMode::Once));
        assert_eq!(playback.start_position, Some(Duration::from_secs_f32(0.5)));
        for tick in 2..5 {
            app.world_mut()
                .resource_mut::<ActorMap>()
                .get_mut(&id)
                .expect("turret missing")
                .beam
                .apply(
                    tick,
                    Some(ActorBeam {
                        target: PlayerId(2),
                        started_tick: 1,
                        remaining_secs: 2.0,
                    }),
                );
            app.update();
            assert_eq!(
                app.world_mut()
                    .query_filtered::<Entity, With<LaserBeam>>()
                    .single(app.world())
                    .expect("beam missing"),
                beam
            );
            assert_eq!(
                app.world().get::<LaserBeam>(beam).expect("beam missing").target,
                PlayerId(2)
            );
        }
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&id)
            .expect("turret missing")
            .beam
            .apply(5, None);
        app.update();
        assert!(app.world().get_entity(beam).is_err());
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&id)
            .expect("turret missing")
            .beam
            .apply(
                6,
                Some(ActorBeam {
                    target: PlayerId(1),
                    started_tick: 6,
                    remaining_secs: 2.0,
                }),
            );
        app.update();
        let second_beam = app
            .world_mut()
            .query_filtered::<Entity, With<LaserBeam>>()
            .single(app.world())
            .expect("beam missing");
        // A snapshot can skip the short cooldown and show another burst at the same player.
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&id)
            .expect("turret missing")
            .beam
            .apply(
                70,
                Some(ActorBeam {
                    target: PlayerId(1),
                    started_tick: 70,
                    remaining_secs: 2.0,
                }),
            );
        app.world_mut().resource_mut::<ServerTick>().0 = 70;
        app.update();
        assert!(app.world().get_entity(second_beam).is_err());
        let third_beam = app
            .world_mut()
            .query_filtered::<Entity, With<LaserBeam>>()
            .single(app.world())
            .expect("beam missing");
        assert!(app.world().get::<AudioPlayer>(third_beam).is_some());
        app.world_mut().resource_mut::<ServerTick>().0 = 131;
        app.update();
        assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
        app.update();
        assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
        app.world_mut().resource_mut::<ActorMap>().remove(&id);
        app.update();
        assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
    }

    #[test]
    fn peace_mode_removes_beams_including_late_cues() {
        let mut app = sync_app();
        app.world_mut().resource_mut::<ActorMap>().peaceful = true;
        for _ in 0..2 {
            for started_tick in [0, 10] {
                app.world_mut().spawn(LaserBeam {
                    actor: ActorId(1),
                    target: PlayerId(1),
                    started_tick,
                });
            }
            app.update();
            assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
        }
    }
}
