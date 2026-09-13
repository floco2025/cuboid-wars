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

- Map editor: Python and PySide6.
- Skybox converter: Python, NumPy, Pillow, and ImageMagick for HDR input.
- Model and audio tools: Blender, FFmpeg, and ImageMagick for textures.
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
