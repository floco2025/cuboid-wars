use bevy::prelude::*;

use super::laser::{LaserBeam, attach_laser_audio, spawn_laser_beam};
use crate::{
    actors::ActorMap,
    config::{AssetSet, ClientSettings},
    players::PlayerMap,
};
use common::{
    constants::TICK_SECS,
    protocol::{ActorMarker, Position, ServerTick},
};

pub(super) fn laser_beams_sync_system(
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
        let beam = spawn_laser_beam(
            &mut commands,
            &mut meshes,
            &mut materials,
            *id,
            active.target,
            active.started_tick,
        );
        let elapsed_ticks = (tick.0.wrapping_sub(active.started_tick) as i32).max(0);
        attach_laser_audio(
            &mut commands,
            beam,
            &actor.kind,
            &asset_server,
            &asset_set,
            &settings,
            positions.get(actor.entity).ok().copied(),
            elapsed_ticks as f32 * TICK_SECS,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actors::ActorInfo, players::PlayerInfo};
    use bevy::audio::PlaybackMode;
    use common::protocol::{ActorBeam, ActorId, Health, Player, PlayerId, PlayerMoveIntent};
    use std::time::Duration;

    #[test]
    fn beam_keeps_one_effect_and_sound_through_retargeting() {
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
        app.world_mut().resource_mut::<ActorMap>().peaceful = true;
        for _ in 0..2 {
            for started_tick in [0, 10] {
                app.world_mut().spawn(LaserBeam {
                    actor: ActorId(1),
                    target: PlayerId(1),
                    started_tick,
                    wander_width_fraction: 0.4,
                    wander_height_fraction: 0.2,
                    aim_height_fraction: 0.8,
                });
            }
            app.update();
            assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
        }
    }
}
