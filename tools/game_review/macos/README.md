# macOS game review

This is the macOS integration for the shared
[game-review workflow](../README.md). Use Peekaboo for desktop actions. If it is
unavailable, use the installed computer-use integration and follow its tool
rules; its key taps do not support timed movement holds.

## Prerequisites

The launcher uses the project's Rust installation and Python's standard
library. Install and authorize the optional desktop tool with:

```sh
brew install openclaw/tap/peekaboo
peekaboo permissions status
```

Enable missing Screen Recording, Accessibility, and Event Synthesizing
permissions in System Settings > Privacy & Security.

## Prepare and launch

Create a temporary app identity and log:

```sh
python3 tools/game_review/macos/prepare_app.py
```

The default app runs the shared deterministic Hotel launch at 1200x800 logical
points. It preserves the shell's PATH, backs up `client_local.json` when
present, and does not launch anything. Override game arguments after `--`:

```sh
python3 tools/game_review/macos/prepare_app.py -- \
  --map obby --god --peace --name Reviewer --windowed \
  --resolution 1200x800 --look 180,-10
```

Copy the printed `app` and `bundle_id` values:

```sh
review_app='/tmp/cuboid-macos-review-REPLACE/Cuboid Review.app'
review_bundle='local.cuboid.review.REPLACE'
peekaboo app launch "$review_app" --foreground --wait-for-window --json
peekaboo window list --app "$review_bundle" --json
```

A first build can outlast the startup wait. Inspect the printed log and list
windows again instead of launching a second instance. Store the current
`Cuboid Wars` window ID; IDs change on every launch:

```sh
review_window=12345
```

## Send input

Target the bundle and window in every command, with `--foreground` so the game
captures and recentres the pointer:

```sh
peekaboo press w --hold 2s --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo press v --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo scroll --direction down --amount 4 --app "$review_bundle" --window-id "$review_window" --foreground
```

Peekaboo 4.3.4 was verified for timed movement, release, wheel zoom, console
typing, and Retina screenshots. Its `move` command uses cursor destinations,
not relative mouse deltas; use the shared `--look` launch flag for a stable
initial camera direction.

Open the console, type a shared deterministic-state command, then press Return
again. `type` can report an error after successfully inserting the text, so
inspect the pixels before retrying.

```sh
peekaboo press Return --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo type '/weather rain' --delay 30ms --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo press Return --app "$review_bundle" --window-id "$review_window" --foreground
```

## Capture and cleanup

Capture the exact game window at native Retina pixels:

```sh
peekaboo see --app "$review_bundle" --window-id "$review_window" \
  --no-elements --retina --path /tmp/cuboid-wars-review.png --json
```

Without `--retina`, Peekaboo captures logical points. Record the render
resolution displayed by the game; preview image dimensions alone are not
evidence of scene resolution.

Quit only this review app and wait for its process to disappear:

```sh
peekaboo press cmd+q --app "$review_bundle" --window-id "$review_window" --foreground
peekaboo window list --app "$review_bundle" --json
```

`APP_NOT_FOUND` is expected after exit. Restore the printed `settings_backup`;
if it is `null`, remove only the local settings file created by this review.
