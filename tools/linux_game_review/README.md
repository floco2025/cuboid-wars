# Linux game review

This is the repeatable setup for controlling and inspecting Cuboid Wars on KDE
Plasma 6 under Wayland. Keep captures in `/tmp`; they can contain local HUD or
desktop information and do not belong in the repository.

Desktop-control commands need explicit approval when the sandbox requests it.
The input helper sends events to the focused application, so focus the game
immediately before sending input and keep only one game window open.

## Prerequisites

The machine needs a C++20 compiler, `pkg-config`, Qt 6 Core and D-Bus headers,
libei headers, `qdbus6`, `rg`, and KDE Spectacle. Check the build dependencies
with:

```sh
pkg-config --modversion Qt6Core Qt6DBus libei-1.0
```

Compile the input helper outside the repository's tracked files:

```sh
c++ -std=c++20 -fPIC -Wall -Wextra -Wpedantic \
  tools/linux_game_review/input.cpp \
  $(pkg-config --cflags --libs Qt6Core Qt6DBus libei-1.0) \
  -o /tmp/cuboid-wars-ei-input
```

## Launch and focus

Launch the review build in a long-running terminal or tool session:

```sh
cargo run --release -- --map hotel --god --peace
```

Find the game window's KWin runner id:

```sh
qdbus6 --literal org.kde.KWin /WindowsRunner \
  org.kde.krunner1.Match cuboid \
  | rg -o '"0_\{[^"}]+\}", "[^"]+"'
```

Choose the entry whose title is exactly `Cuboid Wars`, then activate it using
the id from that entry:

```sh
qdbus6 org.kde.KWin /WindowsRunner \
  org.kde.krunner1.Run '0_{window-id}' ''
```

KWin generates a new id on every launch. Never retain an old id or select a
terminal/editor result whose title merely contains `cuboid`.

## Send input

The helper accepts any number of actions in one invocation:

```text
key CODE
hold CODE MILLISECONDS
move DX DY
scroll UNITS
wait MILLISECONDS
```

Codes are Linux input-event codes. Common ones are Esc `1`, W `17`, Enter
`28`, F `33`, and V `47`. Confirm additional values in
`/usr/include/linux/input-event-codes.h` instead of guessing.

Examples:

```sh
/tmp/cuboid-wars-ei-input key 47 wait 500 move 180 0
/tmp/cuboid-wars-ei-input hold 17 1500 wait 300 scroll -1
```

Combine related actions so the EIS connection is opened only once. Mouse
motion controls the captured game camera; scrolling changes follow/debug zoom.

## Capture and inspect

Refocus the game immediately before each active-window capture:

```sh
spectacle --activewindow --background --nonotify \
  --output /tmp/cuboid-wars-review.png
```

Inspect the resulting PNG with the local image-viewing tool. Use distinct
filenames for each state so comparisons do not overwrite evidence.

For a visual pass, check a stable baseline plus the states relevant to the
change. Grounds work should include walking height, nearby surface detail,
grass and tree distance fades while moving, clear daylight, rain, night, and
both forward and deferred rendering when renderer behavior is in scope. Use
the in-game console commands `/weather clear|rain` and
`/light bright|dim|dark` to make states deterministic.

For performance sampling, disable VSync, let shaders and scene loading settle,
hold the same view and resolution, and record the on-screen FPS rather than a
single transient frame. Restore VSync and any renderer preference afterward.

## Cleanup

Stop the game through its owning terminal/session. Restore every temporary
source or config change, rebuild only if a temporary source edit affected the
executable under review, and finish with `git status --short` and
`git diff --check`. Screenshots and the compiled helper remain temporary.

If `connectToEIS` fails, first confirm this is a KDE Plasma Wayland session and
that the command has desktop-control permission. If Spectacle captures the
wrong window, repeat the KWin activation immediately before capturing.
