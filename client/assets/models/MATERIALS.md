# Model material tuning

Each GLB has a matching `.json` beside its generator. Edit that file and rebuild the model; the game reads the resulting embedded materials, not these JSON files. No runtime texture loading is added. Geometry and animation remain in Python.

The generators import the shared [modelkit](modelkit/) package: catalog materials and metre-scaled UVs (`materials.py`), the wear bake (`wear.py`), mesh primitives (`primitives.py`), the preview studio (`preview.py`), GLB clip post-processing (`glb.py`), and what the wheeled actors share (`wheeled_actor.py`).

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

Actor and player settings include a `wear` section for their geometry-aligned surface bake in [modelkit/wear.py](modelkit/wear.py). `style` is required and selects the bake: `painted` (the bruiser's armour), `plastic` (the scuttler, zapper, and player shells), or `metal` (the turret's bare steel, with fine scratches and no impact dents). Only painted armour exposes a different material when chipped.

Plastic and metal share the surface controls. `scuff_sites` and `dents` use model-space `center` and `extent` vectors in the assembled Blender rest pose (Z up). `scratches` are strokes with `start`, `end`, and `width` in metres; the bake projects them onto the nearby shell surface with uneven taper, wandering edges, and gaps. `scuff_color`, `scratch_color`, and `scuff_roughness` control abrasion appearance; `scratch_depth`, `dent_depth`, and `grain_depth` control normal-map relief in metres. Metallic stays at the material's value throughout the surface: zero for plastic and one for bare metal. Dents are shading relief and do not alter the silhouette or collider.

Painted armour takes the controls in the bruiser section below; the player section says how its flexible-part grain differs from tire rubber. The approved external texture is FreePBR `synth-rubber`; [provenance](../ASSETS.md) records its source. Catalog textures embedded in a model are downscaled to 1024 × 1024 first (`EMBEDDED_TEXTURE_SIZE` in [modelkit/materials.py](modelkit/materials.py)); baked atlases are 2048 × 2048 (`RESOLUTION` in `modelkit/wear.py`). Each actor and the player has its own atlas for its main surfaces; small fittings and wall lights use plain materials. Each `<model>_textures/` folder contains the generated sRGB albedo, linear packed metallic-roughness (R = AO, G = roughness, B = metallic), and OpenGL tangent-space normal maps. The GLB embeds the same maps; standalone PNGs are not loaded separately. These atlases fit their model UVs and are not tiling wall/floor textures. Other external texture use requires discussion first.

## Bruiser

The `wear` section of [bruiser.json](bruiser.json) configures the painted armour bake: `edge_width` is the distance from a panel edge within which paint chips, `edge_chip_threshold` controls rarity (higher means fewer), and `chip_sites` places localized impact chips with model-space `center` and `extent` vectors. The base paint colour, roughness, and metallic value come from `materials.armour`; `paint_variation_color`, `exposed_metal_color`, `exposed_metallic`, `roughness_variation`, `wear_roughness_reduction`, `scratch_depth`, and `normal_strength` add paint variation, exposed metal, roughness changes, and scratch relief. Colours are linear RGB. Tires use the approved FreePBR synthetic rubber with model-specific scale and normal relief. `--preview` renders `/tmp/bruiser-preview.png`, `/tmp/bruiser-rear.png`, and `/tmp/bruiser-gameplay.png`; `--motion` adds animation frames.

## Player

`materials.joint` in [player.json](player.json) selects the FreePBR synthetic rubber with a 0.3 m tile size and 0.15 normal strength for fine, shallow grain on flexible parts; the bruiser and scuttler tires keep independent, coarser settings. The generator embeds a darker, lower-contrast variant of the rubber, while the white shells (`ivory`), steel, visor, chassis, markings, and lights are plain materials. The `wear` section controls scuffs, scratches, and shallow dent relief on the solid ivory plastic, lighter than on the wheeled actors and concentrated on shoulders, cuffs, lower legs, and feet; it has no paint or exposed metal. `--preview` renders one still per clip and `/tmp/player-detail.png`; `--rear` adds `/tmp/player-rear-head.png`.

## Rebuilding

Run from the repository root:

```sh
/opt/homebrew/bin/blender --background --python client/assets/models/player.py -- --preview --rear
/opt/homebrew/bin/blender --background --python client/assets/models/bruiser.py -- --preview --motion
/opt/homebrew/bin/blender --background --python client/assets/models/scuttler.py -- --preview --motion
/opt/homebrew/bin/blender --background --python client/assets/models/zapper.py -- --preview --motion
/opt/homebrew/bin/blender --background --python client/assets/models/turret.py -- --preview
/opt/homebrew/bin/blender --background --python client/assets/models/wall_lights.py -- --preview
```

Everything after `--` is optional: `--preview` renders stills to `/tmp`, and `--motion` adds a frame sequence. `wall_lights.py` reads separate decorative and utility JSON files and rebuilds both models. Other generators rebuild independently. Restart the client to inspect regenerated models in-game.

For consistent front, rear, and small previews without rebuilding, run `/opt/homebrew/bin/blender --background --python client/assets/models/modelkit/preview.py -- client/assets/models/player.glb` (multiple GLB paths are accepted). Images go to `/tmp/model-texture-review/`.
