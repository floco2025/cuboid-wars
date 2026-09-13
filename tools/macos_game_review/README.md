# macOS game review

This is the repeatable setup for controlling and inspecting Cuboid Wars on
macOS. Keep captures and review apps in `/tmp`; screenshots can contain local
HUD or desktop information and do not belong in the repository.

Use the installed computer-use skill for desktop actions. The examples below
use its `node_repl` and `@oai/sky` interface. Follow that skill's permission and
tool rules; shell commands here prepare files and inspect logs. Keep only one
game window open, and close only the review instance you launched.

## Prerequisites

Use the project's existing Rust and Python installations. The launcher uses
only Python's standard library. Desktop control needs Accessibility and Screen
Recording access for the controlling app; if either is missing, have the user
enable it in System Settings > Privacy & Security.

## Launch and focus

Prepare an app bundle with its own identity and log:

```sh
python3 tools/macos_game_review/prepare_app.py
```

It creates a temporary `.app` that runs `cargo run --release` from this checkout
with `--map hotel --god --peace --name Reviewer --volume 0 --window-width 1200
--window-height 800`. It preserves the shell's PATH for Cargo and saves a copy
of `config/client/client_local.json` when present. It does not launch anything.
Saved fullscreen and VSync settings still apply; select Windowed in the game
settings if needed.
To replace the game arguments, pass them after `--`:

```sh
python3 tools/macos_game_review/prepare_app.py -- --map obby --god --peace --name Reviewer
```

Copy the printed `app` path into `reviewApp` in `node_repl`:

```js
var sky = (await import('@oai/sky')).sky;
var reviewApp = '/tmp/cuboid-macos-review-REPLACE/Cuboid Review.app';
var state = await sky.get_app_state({ app: reviewApp });
nodeRepl.write(state.text);
```

`get_app_state` launches the app if needed. A first build can take longer than
the tool's startup wait; inspect the printed log path and retry the same app
after Cargo finishes, without launching another instance. If targeting by
name fails, use the printed bundle identifier.

Read the current accessibility tree and raise the `Cuboid Wars` window using
its current `Raise` action. Fetch state again afterward. The game captures and
recentres the pointer on focus; a background window cannot validate mouse-look.

## Send input

Use short actions, then inspect the resulting state. For example, V cycles
debug views, F toggles facing lock, and scrolling changes follow/debug zoom:

```js
await sky.press_key({ app: reviewApp, key: 'v' });
nodeRepl.write((await sky.get_app_state({ app: reviewApp })).text);
```

For scrolling, pass `x` and `y` inside the game window using the desktop tool's
coordinates. Do not derive them from the rendered image's pixel dimensions.
Keep the game focused and verify that the camera actually changed.

`press_key` is a tap, not a timed hold. Do not treat repeated taps as a walking
test or assume a drag generates captured relative mouse motion. If the desktop
tool cannot perform the required movement, have the user move to the review
position, or use a temporary fixed-camera example for a static comparison.
Label static measurements accordingly; they do not test movement or distance
transitions.

Open the console with Return or `/`, verify it is visible, then type
`/weather clear`, `/weather rain`, or `/light bright|dim|dark` as needed. If `/`
already inserted the slash, enter only the remainder. Verify the command's
reply before capturing; a key event alone does not prove the console opened.

## Capture and inspect

Capture through the desktop tool and preserve its original PNG:

```js
var state = await sky.get_app_state({ app: reviewApp });
nodeRepl.write(state.text);
if (!state.screenshot) throw new Error('Game screenshot missing');
var fs = await import('node:fs/promises');
var shot = (await import('node:url')).fileURLToPath(state.screenshot.url);
await fs.copyFile(shot, '/tmp/cuboid-wars-clear.png');
await nodeRepl.emitImage({ bytes: await fs.readFile(shot), mimeType: 'image/png' });
```

Use distinct filenames for each state. The game's pixels are not exposed as
accessibility text, so inspect the image even when the window tree is unchanged.
If the tool reports that the user changed the app, fetch fresh state before
sending another action.

For grounds work, check walking height, nearby detail, grass/tree distance
fades during movement, clear daylight, rain, and night. When renderer behavior
is in scope, check `rendering.opaque_renderer` set to `forward` and `deferred`
in `config/client/client.json`, restarting between them.

## Performance sampling

Disable VSync and enable diagnostics in the settings menu. Let shaders, asset
loading, and mipmap generation settle before sampling several FPS readings
over 10–15 seconds. Hold the same position, view direction, FOV, resolution,
renderer, MSAA, and rearview setting for comparisons. Keep builds, tests, and
other game instances stopped while measuring.

macOS window sizes use logical points: a 1200×800 window can have 2400×1600
physical pixels on Retina. Record the resolution displayed by the game; its
render-resolution preference can cap the actual scene size. Screenshot preview
dimensions are not evidence of render resolution. Record the machine and review
conditions, and compare against a baseline on that machine.

## Cleanup

Fetch fresh state and quit the review app with `super+q`, or use its window's
close button. Confirm it has stopped using the process/app list, allowing time
for shutdown; `get_app_state` would launch it again. Copy the printed
`settings_backup` back to `config/client/client_local.json`; if it was `null`,
remove the local settings file created by this review instead. Restore any
temporary renderer/source changes as well, preserving unrelated edits.

Remove temporary examples from the checkout, and finish with `git status --short`
and `git diff --check`. Screenshots, app bundles, and logs stay outside the repo.
Use `cargo run --release` on the next launch so temporary source changes cannot
leave a stale executable in use.
