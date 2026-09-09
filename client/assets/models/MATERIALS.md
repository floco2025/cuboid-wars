# Model material tuning

Each GLB has a matching `.json` beside its generator. Edit that file and rebuild the model; the game reads the resulting embedded materials, not these JSON files. No runtime texture loading is added. Geometry and animation remain in Python.

The `materials` object maps part roles such as `rubber`, `steel`, or `glow` to material definitions. Roles are used by the generator, so keep their keys. `name` labels the exported material. A `source` selects a texture set from `config/client/assets.json`; omitting it creates a plain material with a required `color`.

| Setting | Applies to | Effect |
| --- | --- | --- |
| `color` | Plain | Linear RGB from 0 to 1. |
| `metallic` | Both | Absolute metallic value from 0 to 1; overrides the texture channel when specified. |
| `roughness` | Plain | Roughness from 0 (polished) to 1 (matte); default 0.4. |
| `tile_size` | Textured | Metres per repeat; smaller values make grain finer. Defaults to the catalog value. |
| `tint` | Textured | Linear RGB multipliers from 0 to 1; default `[1, 1, 1]`. |
| `color_contrast` | Textured | Variation around the texture average; 0 is uniform, 1 preserves the source. |
| `normal_strength` | Textured | Surface relief; 0 is smooth, 1 preserves the source, larger values strengthen bumps. |
| `roughness_factor` | Textured | Multiplies source roughness; default 1. |
| `emission` | Both | Glow strength; default 0. |
| `emission_color` | Both | Linear RGB glow color; plain materials default to their base color. Specify for textured glow. |
| `backface_culling` | Both | Hide inward-facing surfaces; default false. |

Textured metallic defaults to the catalog and its packed map. Plain metallic defaults to 0. Overrides are baked into embedded maps where needed for Bevy; the shared source PNGs remain unchanged. Unknown fields and invalid values stop generation.

Actor and player settings include a `wear` section for their geometry-aligned surface bake in [model_wear.py](model_wear.py). Wear follows the material: the bruiser has painted armor, the scuttler/zapper/player have solid plastic shells, and the turret has bare metal with fine scratches and no impact dents. Only painted armor exposes a different material when chipped.

Plastic and bare metal use `style: "plastic"` or `"metal"`. `scuff_sites` and `dents` use model-space `center` and `extent` vectors in the assembled Blender rest pose (Z up). `scratches` are strokes with `start`, `end`, and `width` in metres; the bake projects them onto the nearby shell surface with uneven taper, wandering edges, and gaps. `scuff_color`, `scratch_color`, and `scuff_roughness` control abrasion appearance; `scratch_depth`, `dent_depth`, and `grain_depth` control normal-map relief in metres. Metallic stays at the material's value throughout the surface: zero for plastic and one for bare metal. Dents are shading relief and do not alter the silhouette or collider.

Painted armor uses the [bruiser wear controls](bruiser.materials.md). The [player settings](player.materials.md) keep flexible-part grain finer and shallower than tire rubber. The approved external texture is FreePBR `synth-rubber`; [provenance](../ASSETS.md) records its source. Each actor and the player has its own atlas for its main surfaces; small fittings and wall lights use plain materials. Each `<model>_textures/` folder contains the generated sRGB albedo, linear packed metallic-roughness (R = AO, G = roughness, B = metallic), and OpenGL tangent-space normal maps. The GLB embeds the same maps; standalone PNGs are not loaded separately. These atlases fit their model UVs and are not tiling wall/floor textures. Other external texture use requires discussion first.

Run from the repository root:

```sh
/opt/homebrew/bin/blender --background --python client/assets/models/player.py -- --rear-preview
/opt/homebrew/bin/blender --background --python client/assets/models/bruiser.py -- --preview
/opt/homebrew/bin/blender --background --python client/assets/models/scuttler.py -- --preview
/opt/homebrew/bin/blender --background --python client/assets/models/zapper.py -- --preview
/opt/homebrew/bin/blender --background --python client/assets/models/turret.py
/opt/homebrew/bin/blender --background --python client/assets/models/wall_lights.py
```

`wall_lights.py` reads separate decorative and utility JSON files and rebuilds both models. Other generators rebuild independently. Previews go to `/tmp`; restart the client to inspect regenerated models in-game.

For consistent front, rear, and small previews without rebuilding, run `/opt/homebrew/bin/blender --background --python client/assets/models/model_preview.py -- client/assets/models/player.glb` (multiple GLB paths are accepted). Images go to `/tmp/model-texture-review/`.
