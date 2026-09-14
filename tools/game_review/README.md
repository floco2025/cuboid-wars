# Game review

This directory contains the repeatable workflow for visual and performance
reviews of Cuboid Wars. Use the platform integration in [linux](linux/README.md)
or [macos](macos/README.md) for window focus, input, and capture.

Keep screenshots, recordings, logs, helper binaries, and temporary app bundles
in `/tmp`; captures can contain local HUD or desktop information and do not
belong in the repository. Keep only one game window open and stop only the
review instance you launched. Desktop control and capture may need approval
outside an agent's filesystem sandbox.

## Deterministic launch

Build or launch the current checkout in release mode. These session-only CLI
overrides take precedence over saved window settings without changing
`client_local.json` or map data:

```text
--windowed
--resolution WIDTHxHEIGHT
--spawn X,Y,Z
--look BEARING,PITCH
```

`--resolution` is the window's logical-pixel size; the physical render target
is larger on a scaled or Retina display. `--spawn` is a world-space feet
position and is server-authoritative for the first body. It is intentionally
single-player-only and cannot be combined with `--host`, `--join`, or
`--serve`. `--look` sets the initial view in degrees: bearing is clockwise from
world north (`+Z`), east is 90 (`+X`), and positive pitch looks up. Spawn and
look apply only at startup; ordinary movement and respawns work normally.

For example, this opens a reproducible exterior Hotel view:

```sh
cargo run --release -- --map hotel --god --peace --name Reviewer --volume 0 \
  --windowed --resolution 1280x720 \
  --spawn=-60,10,-10 --look=270,-10
```

The older `--window-width` and `--window-height` flags remain useful when only
one saved dimension should be overridden. Any window or volume flag also stops
the session from saving local settings, so a review never rewrites
`client_local.json`. Use `--help` for the complete CLI.

## Review sequence

Capture a stable baseline before interacting. Make one controlled change at a
time and capture again. Verify pixels rather than treating successful input
dispatch as proof that the game handled it.

Use the console to make environmental states repeatable:

```text
/weather clear
/weather rain
/time H:MM
/time auto
/moon 0-1
```

For grounds work, inspect walking height, nearby surface detail, grass density
against green and brown patches, tree and grass distance fades while moving,
clear daylight, rain, and night. For camera-dependent rendering, inspect the
main view, rearview, and portals that are in scope. When material renderer
behavior matters, test `rendering.opaque_renderer` as both `forward` and
`deferred` in `config/client/client.json`, restarting between them and restoring
the original value afterward.

Use a recording for motion, shimmer, animation, frame pacing, or LOD
transitions; still images are better for stable side-by-side comparisons. Keep
each capture under a distinct name rather than overwriting evidence.

## Performance sampling

Disable VSync and enable diagnostics in the settings menu when measuring
uncapped performance. Let shaders and scene loading settle, then record several
FPS or frame-time samples over 10–15 seconds. Hold position, view, FOV, logical
and displayed render resolution, renderer, MSAA, rearview setting, and machine
constant across comparisons. Stop builds, tests, and other game instances
during the sample. MangoHud is useful on Linux for frame-time and GPU logging.

## Cleanup

Stop the game through the process that launched it. Restore temporary renderer
or settings changes, preserve unrelated edits, and finish with formatting,
relevant tests, `git status --short`, and `git diff --check`. A review app may
need time to exit before its window disappears. Run through Cargo again after
source changes so a stale executable is not reviewed.
