# Dependencies

## Running the game

The game is built and run from a GitHub checkout using Rust and Cargo. These instructions assume a working development machine; macOS also assumes Homebrew and Xcode are installed.

If Rust is not installed, use [rustup](https://rust-lang.org/tools/install/), the recommended installer on either platform:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accept the default installation and open a new terminal afterward.

Package-manager alternatives: `brew install rust` on macOS or `sudo pacman -S --needed rust` on CachyOS. Use one installation method; skip this step if Rust is already available.

Run from the checkout:

```sh
cargo run --release
```

## Development

These additional tools are needed for editing maps, generating assets, and formatting code.

- Map editor: Rust/Cargo, Python 3.10 or newer, and PySide6. The launcher builds its native Rust map library automatically on first use and refreshes it after source changes; the initial build can take a few minutes. To build it ahead of time, run `cargo build --release -p map_core_py`.
- Model and audio tools: Blender, FFmpeg, and ImageMagick for textures; NumPy and Pillow for the generated ones.
- Formatting: rustfmt, Clippy, Prettier, and Ruff.

**macOS:**

```sh
brew install python pyside numpy pillow imagemagick ffmpeg prettier ruff
brew install --cask blender
```

**CachyOS:**

```sh
sudo pacman -S --needed python pyside6 python-numpy python-pillow \
  imagemagick blender ffmpeg prettier ruff
```

With rustup, add any missing formatting components using `rustup component add rustfmt clippy`.

Optional game review tools: the shared [game-review workflow](tools/game_review/README.md), with platform integrations for [macOS (Peekaboo)](tools/game_review/macos/README.md) and [Linux](tools/game_review/linux/README.md).
