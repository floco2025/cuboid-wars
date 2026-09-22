use std::{path::Path, process};

use anyhow::Result;
use bevy::prelude::*;
use client::{
    app::build_client_app,
    network::{ClientToServerChannel, ServerLink, apply_playback_frame, install_playback},
    schedule::ClientSet,
    ui::SettingsMenuState,
};
use crossbeam_channel::unbounded;

use super::{
    playback::{Controls, Playback},
    script::Script,
};
use crate::WindowArgs;

struct Viewer {
    playback: Playback,
    clock: Time,
    error: Option<String>,
}

#[derive(Component)]
struct ExperimentOverlayMarker;

pub fn play_file(path: &Path, window: &WindowArgs) -> Result<()> {
    let playback = Playback::new(Script::load(path)?)?;
    // No server thread or live input channel: the executor owns both simulation
    // halves, and the client displays observations from completed ticks only.
    let (to_server, _) = unbounded();
    let (_, from_server) = unbounded();
    let mut app = build_client_app(
        window.client_options(true),
        ClientToServerChannel::new(to_server),
        ServerLink::Local(from_server),
        playback.executor.session.bootstrap.clone(),
    )?;
    install_playback(&mut app);
    app.insert_non_send(Viewer {
        playback,
        clock: Time::default(),
        error: None,
    });
    app.add_systems(Startup, spawn_overlay);
    app.add_systems(
        Update,
        drive_viewer
            .before(ClientSet::Network)
            .before(ClientSet::Input)
            .before(ClientSet::Sky),
    );
    let exit = app.run();
    process::exit(match exit {
        AppExit::Success => 0,
        AppExit::Error(code) => i32::from(code.get()),
    });
}

fn spawn_overlay(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(80.0),
                max_width: Val::Percent(85.0),
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.025, 0.04, 0.92)),
            GlobalZIndex(5),
        ))
        .with_child((
            ExperimentOverlayMarker,
            Text::new("Experiment paused"),
            TextFont {
                font_size: FontSize::Px(17.0),
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

fn drive_viewer(world: &mut World) {
    let keyboard = world.resource::<ButtonInput<KeyCode>>();
    let menu_open = world.resource::<SettingsMenuState>().open;
    let controls = if menu_open {
        Controls::default()
    } else {
        Controls {
            next: keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter),
            play_pause: keyboard.just_pressed(KeyCode::Space),
            restart: keyboard.just_pressed(KeyCode::KeyR),
        }
    };
    let delta = world.resource::<Time<Real>>().delta();
    let mut viewer = world.remove_non_send::<Viewer>().expect("experiment viewer");
    if menu_open {
        viewer.playback.pause();
    }
    if controls.restart {
        viewer.error = None;
    }
    let simulated = if viewer.error.is_none() {
        match viewer.playback.update(controls, delta) {
            Ok(simulated) => simulated,
            Err(error) => {
                viewer.error = Some(format!("{error:#}"));
                std::time::Duration::ZERO
            }
        }
    } else {
        std::time::Duration::ZERO
    };
    // Time<Virtual> remains paused so the ordinary fixed loop never runs.
    // Presentation timers instead see only time actually simulated by the executor.
    viewer.clock.advance_by(simulated);
    *world.resource_mut::<Time>() = viewer.clock;
    if let Some(frame) = viewer.playback.take_frame() {
        apply_playback_frame(world, frame);
    }
    let executor = &viewer.playback.executor;
    let phase = if viewer.error.is_some() {
        "ERROR"
    } else if executor.finished() {
        "COMPLETE"
    } else if viewer.playback.paused {
        "PAUSED"
    } else {
        "RUNNING"
    };
    let action = executor
        .script
        .actions
        .get(executor.steps.len())
        .map(|action| serde_json::to_string(action).expect("action JSON"))
        .unwrap_or_else(|| "All actions executed".into());
    let last = executor
        .steps
        .last()
        .map(|step| format!("Last result: {}", step["result"]))
        .unwrap_or_default();
    let status = format!(
        "EXPERIMENT {phase} | {}/{} completed | tick {}\n{}: {action}\n{last}\n{}\nEnter: next action   Space: play/pause all   R: restart\nMouse: look   Wheel: zoom   V: camera view   Esc: menu",
        executor.steps.len(),
        executor.script.actions.len(),
        executor.session.tick(),
        if executor.running() { "Current" } else { "Next" },
        viewer.error.as_deref().unwrap_or("")
    );
    for mut text in world
        .query_filtered::<&mut Text, With<ExperimentOverlayMarker>>()
        .iter_mut(world)
    {
        if text.0 != status {
            text.0.clone_from(&status);
        }
    }
    world.insert_non_send(viewer);
}
