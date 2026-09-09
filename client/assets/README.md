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
- `ladder` and `pressure_plate.panel` / `pressure_plate.frame` — fixture materials.
- `player` and `actors.<kind>` — `model` (`scene`, `scale`, `x_offset` / `y_offset` / `z_offset`, `x_rotation_degrees`, `animation_index`, `animation_speed`, optional `wheels` and `aim_rig`, `rotate_with_facing`) and `sounds`. Positions are feet-based, so `y_offset` is the model origin's offset from the character's feet.
- `wall_lights.<kind>` — `scene`, `scale`, `offset_from_wall`, `brightness`, `range`, `radius`, `emissive_luminance`, `color`, and `flicker`.
- `skyboxes.<name>` — `image`, `brightness`, `rotation_period_secs`, `sun_step_degrees`, and `sun_disc`.

The generators and material JSON beside the GLBs in `models/` are described in [models/MATERIALS.md](models/MATERIALS.md).
