# Linux game review

This is the KDE Plasma 6 Wayland integration for the shared
[game-review workflow](../README.md). The normal path is unattended: launch,
focus, injected input, screenshots, and restored-portal recordings should not
depend on someone sitting at the desktop.

## Prerequisites

The workflow needs a C++20 compiler, `pkg-config`, Qt 6 Core and D-Bus headers,
libei headers, `qdbus6`, `rg`, and KDE Spectacle. `kdotool` is the preferred
window-control frontend. GPU Screen Recorder, RenderDoc, and `wev` support
motion capture and focused diagnostics.

Check and build the input helper outside the repository's tracked files:

```sh
pkg-config --modversion Qt6Core Qt6DBus libei-1.0
c++ -std=c++20 -fPIC -Wall -Wextra -Wpedantic \
  tools/game_review/linux/input.cpp \
  $(pkg-config --cflags --libs Qt6Core Qt6DBus libei-1.0) \
  -o /tmp/cuboid-wars-ei-input
```

## Focus the game

Launch with the shared guide's deterministic CLI, optionally prefixed by
`mangohud`. Focus the exact title without retaining a launch-specific ID:

```sh
kdotool search --title --case-sensitive --limit 1 '^Cuboid Wars$' \
  windowactivate
```

If `kdotool` is unavailable, find and activate the current KWin runner entry:

```sh
qdbus6 --literal org.kde.KWin /WindowsRunner \
  org.kde.krunner1.Match cuboid \
  | rg -o '"0_\{[^"}]+\}", "[^"]+"'
qdbus6 org.kde.KWin /WindowsRunner \
  org.kde.krunner1.Run '0_{window-id}' ''
```

Choose only the entry titled exactly `Cuboid Wars`. KWin generates a new ID on
every launch.

## Send input

The helper accepts any sequence of these actions:

```text
key CODE
hold CODE MILLISECONDS
move DX DY
scroll CLICKS
click left|right
wait MILLISECONDS
```

Common Linux input-event codes are Esc `1`, W `17`, Enter `28`, F `33`, and V
`47`. Confirm other values in `/usr/include/linux/input-event-codes.h`. The
whole sequence is validated before anything is sent; a malformed action prints
the usage and sends nothing. Every emitted event is followed by a 60 ms settle,
so `hold` lasts its milliseconds plus about 120 ms.

```sh
/tmp/cuboid-wars-ei-input key 47 wait 500 move 180 0
/tmp/cuboid-wars-ei-input hold 17 1500 wait 300 scroll -1
/tmp/cuboid-wars-ei-input click left wait 200 click right
```

Focus immediately before input. Combine related actions so the EIS connection
is opened only once. Mouse motion controls the captured camera, `scroll` moves
one wheel click per unit through follow/debug zoom, and `click` fires or places
portals.

## Capture

Refocus immediately before each active-window capture:

```sh
kdotool search --title --case-sensitive --limit 1 '^Cuboid Wars$' \
  windowactivate
spectacle --activewindow --background --nonotify \
  --output /tmp/cuboid-wars-review.png
```

For motion, use the already-initialized desktop portal and stop the recorder's
owning process with Ctrl+C so it finalizes the file:

```sh
gpu-screen-recorder -w portal -restore-portal-session yes -f 60 \
  -o /tmp/cuboid-wars-review.mp4
```

If a portal chooser unexpectedly appears, stop that attempt and continue with
automated screenshots. `qrenderdoc` is an interactive fallback only after a
normal capture isolates a Vulkan issue. `wev` is for input diagnosis, not the
normal review path.

If `connectToEIS` fails, confirm this is a KDE Plasma Wayland session with
desktop-control permission. If Spectacle captures the wrong window, repeat
activation immediately before capture.
