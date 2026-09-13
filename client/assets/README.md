# Assets License Notice

**Assets are licensed separately from the game's source code.**

Project-created assets (3D models, textures, sounds, images, etc.) are provided for use in this game only. Third-party assets retain their own licenses, including permissive licenses where explicitly documented.

If you are forking or using this codebase, replace project-restricted assets with your own or properly licensed alternatives. Use and redistribution of third-party assets must follow their individual terms.

The game's source code is licensed under MIT OR Apache-2.0, but this license does NOT apply to the contents of this directory.

See [ASSETS.md](ASSETS.md) for the provenance register: authors, sources, licenses, generators, embedded dependencies, and unresolved origins. Supplied license notices are linked from their entries.

## Asset Set

`config/client/assets.json` is the client asset set. Only the client reads it; the server takes map materials from each map's layout and `settings.json::textures`. Asset paths are relative to `client/assets`.

- `materials` — texture sets: `textures` paths (`base_color`, `normal`, `occlusion`, `metallic_roughness`), `tile_size` in metres, `metallic`, `roughness`, `repeat`, and `linear_data_textures`. Normal maps carry their Y convention in the file name (`-normal-dx` or `-normal-gl`).
- `aliases` — map texture alias → material. A map enables an alias in its `settings.json::textures` with a `portalable` flag, and its layout names aliases per face.
- `ladder` — fixture material.
- `pressure_plate` — model `scene`, fallback `default_color`, `light_color`, and `emissive_luminance`. Treads inherit barrier/bridge colors or a switch's `plate_color` override.
- `player` and `actors.<kind>` — `model` (`scene`, `scale`, `x_offset` / `y_offset` / `z_offset`, `x_rotation_degrees`, `animation_index`, `animation_speed`, optional `wheels` and `aim_rig`, `rotate_with_facing`) and `sounds`. Positions are feet-based, so `y_offset` is the model origin's offset from the character's feet.
- `wall_lights.<kind>` — `scene`, `scale`, `offset_from_wall`, `brightness`, `range`, `radius`, `emissive_luminance`, `color`, and `flicker`.
- `skyboxes.<name>` — `image`, `brightness`, `rotation_period_secs`, `celestial_step_degrees`, and `celestial_disc`.

The generators and material JSON beside the GLBs in `models/` are described in [models/MATERIALS.md](models/MATERIALS.md).

## Skybox conversion

`convert_skybox.py` converts 2:1 equirectangular PNG/JPEG/TIFF or Radiance `.hdr` panoramas to the game's 4×3 RGBA PNG cube cross. The default 1024-pixel faces produce a 4096×3072 image. Install its dependencies through [the machine setup guide](../../DEPENDENCIES.md).

From the repository root, set `SKYBOX_PACK` to the directory containing the original pack, then convert Obby's sky:

```sh
SKYBOX_PACK="/path/to/InfinitySkyboxesPack2_Universal"
python3 client/assets/convert_skybox.py \
  "$SKYBOX_PACK/Textures/AnotherPlanet/Skybox_AnotherPlanet_Day_3.png" \
  client/assets/Skybox_AnotherPlanet_Day_3.png \
  --light-pixel 1417 1278
```

The face layout and initial panorama orientation match the cross export from [HDRI to CubeMap](https://matheowis.github.io/HDRI-to-CubeMap/). Conversion uses bilinear sampling with longitude wrapping and 2×2 samples per pixel. `--face-size` sets resolution, `--samples 1|2|4` controls sampling, and `--yaw-degrees` rotates the sky around Bevy's +Y axis. PNG colors are preserved; HDR uses the web tool's default exposure of 4 and Reinhard tone mapping into the game's 8-bit format. Override with `--exposure` and `--tone-map clip|reinhard`.

`--light-pixel X Y` prints `celestial_disc.direction` for a pixel in the original panorama, measured from its top-left corner, including the requested yaw. The panorama's center faces world −Z at zero yaw. The cross tiles are `+Y` above, `-X +Z +X -Z` across, and `-Y` below, with Bevy converting cube Z to world −Z.

For batch conversion, pass an input directory and an output directory outside it. Subdirectories are preserved; files with conflicting output names are rejected. Existing output PNGs are replaced when rerunning a conversion. Keep unused converted skies outside `client/assets`.

```sh
python3 client/assets/convert_skybox.py "$SKYBOX_PACK/Textures" /tmp/converted-skies
```

Register each used PNG under `config/client/assets.json::skyboxes`, then select its name with `"skybox": "another_planet_day"` in the map's `settings.json`. All disc settings belong to the asset entry's `celestial_disc`:

```json
{
  "direction": [-0.735561, 0.556526, 0.386301],
  "show": true,
  "distance": 800.0,
  "radius": 24.0,
  "bright": { "luminance": 100.0, "phase_percent": 100.0 },
  "dim": { "luminance": 7.0, "phase_percent": 100.0 },
  "dark": { "luminance": 7.0, "phase_percent": 30.0 }
}
```

`direction` is a nonzero `[x, y, z]` vector in the game's Y-up world; its length does not matter. The sun and moon share this direction as lighting changes between bright, dim, and dark. `show: false` hides their rendered disc while preserving illumination and shadows. It cannot remove celestial bodies baked into the panorama. Each look owns the disc's luminance and lit percentage; world illumination remains in `client.json::lighting`. For rotating skies, the direction rotates with the sky; `rotation_period_secs: 0` in the asset entry holds both still. Hotel retains its cloudy rotating sky, light direction, and visible disc. Obby uses the stationary planet sky with a visible disc aligned to its bright sun.
