# Player material tuning

Edit `player.materials.json`, then rebuild from the repository root:

```sh
/opt/homebrew/bin/blender --background --python client/assets/models/player.py -- --rear-preview
```

This writes `player.glb` with embedded textures and renders `/tmp/player-detail.png` and `/tmp/player-rear-head.png`. Restart the client to inspect the result in-game. These overrides affect only the player; the source textures and the materials used for maps and other actors stay independent.

| Setting | Effect |
| --- | --- |
| `tile_size` | Metres per texture repeat. Smaller values make grain finer. Must be greater than zero. |
| `tint` | RGB multipliers from 0 to 1, applied to linear colour. `[1, 1, 1]` keeps source colour; lower values darken it. |
| `color_contrast` | Strength of colour variation around the texture's average. `0` is uniform, `1` preserves the source. |
| `normal_strength` | Surface bump strength. `0` is smooth, `1` preserves the source. |
| `roughness_factor` | Multiplies the source roughness. Lower is shinier; higher is more matte, capped at 1 per pixel. |

White shells use `scuffed-plastic`, exposed mechanisms use `brushed-metal`, and flexible joints and fingertips use `synth-rubber`. The visor, chassis, markings, and lights use plain materials defined near the top of `player.py`.
