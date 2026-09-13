# macOS game review

This is the repeatable setup for controlling and inspecting Cuboid Wars on
macOS. Keep captures and review apps in `/tmp`; screenshots can contain local
HUD or desktop information and do not belong in the repository.

Use Peekaboo for desktop actions. Keep only one game window open, and close
only the review instance you launched. If Peekaboo is unavailable, use the
installed computer-use skill and follow its tool rules; its key taps do not
support timed movement holds.

## Prerequisites

Use the project's existing Rust and Python installations. The launcher uses
only Python's standard library. Install the optional review tool with:

```sh
brew install openclaw/tap/peekaboo
peekaboo permissions status
```

Have the user enable any missing permissions in System Settings > Privacy &
Security. Screen Recording, Accessibility, and Event Synthesizing were granted
for the verified setup. Run desktop commands outside an agent's filesystem
sandbox using its normal approval mechanism.

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

Copy the printed `app` and `bundle_id` into these variables:

```sh
review_app='/tmp/cuboid-macos-review-REPLACE/Cuboid Review.app'
review_bundle='local.cuboid.review.REPLACE'
peekaboo app launch "$review_app" --foreground --wait-for-window --json
peekaboo window list --app "$review_bundle" --json
```

A first build can take longer than the startup wait; inspect the printed log
path and list windows again after Cargo finishes, without launching another
instance. Copy the `Cuboid Wars` window ID into `review_window` below; window
IDs change on each launch. Target that window and bundle in every command.

```sh
review_window=12345
```

Replace `12345` with the actual ID. Input commands use `--foreground` to focus
the game. It captures and recentres the pointer on focus; a background window
cannot validate mouse-look.

## Send input

Capture a baseline first using the next section. Send one short action, then
capture and inspect again before choosing another. These examples walk forward
for two seconds, cycle debug views, and zoom out:

```sh
peekaboo press w --hold 2s --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo press v --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo scroll --direction down --amount 4 --app "$review_bundle" --window-id "$review_window" --foreground
```

`press --hold` releases the key at the end. F toggles facing lock. Close the
console and settings menu before movement tests. A successful dispatch does
not prove that the game handled the input; verify the pixels.

With Peekaboo 4.3.4, timed W movement, release, wheel zoom, console typing, and
Retina screenshots were verified in-game. `move` did not change captured
mouse-look, with either bridge or `--no-remote` delivery. Its window-relative
coordinates are cursor destinations, not relative motion deltas. Have the user
orient the camera when needed, or use a temporary fixed-camera example for a
static comparison. Label static measurements accordingly.

Open the console with Return, capture to verify its prompt, then type:

```sh
peekaboo press Return --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo type '/weather rain' --delay 30ms --app "$review_bundle" --window-id "$review_window" --foreground
```

`type` can return an error after successfully entering the text. The console
has no Accessibility text value to inspect, so check the screenshot before
retrying. Once the text is correct, press Return again and verify the reply.
Use `/weather clear` or `/light bright|dim|dark` for other conditions.

## Capture and inspect

Capture the exact game window and inspect the saved PNG:

```sh
peekaboo see --app "$review_bundle" --window-id "$review_window" \
  --no-elements --retina --path /tmp/cuboid-wars-clear.png --json
```

`--retina` preserves native pixels; without it the capture uses logical points.
Use distinct filenames for each state. The game's pixels are not exposed as
accessibility text, so inspect the image even when the window tree is unchanged.
Capture fresh state after the user interacts with the app before sending input.

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

Quit the review app and confirm its window and process have disappeared:

```sh
peekaboo press cmd+q --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo window list --app "$review_bundle" --json
```

Allow time for shutdown; `APP_NOT_FOUND` is expected after it exits. Copy the
printed `settings_backup` back to `config/client/client_local.json`; if it was
`null`, remove the local settings file created by this review instead. Restore
any temporary renderer/source changes as well, preserving unrelated edits.

Remove temporary examples from the checkout, and finish with `git status --short`
and `git diff --check`. Screenshots, app bundles, and logs stay outside the repo.
Use `cargo run --release` on the next launch so temporary source changes cannot
leave a stale executable in use.
