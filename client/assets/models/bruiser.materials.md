# Bruiser materials

Run `/opt/homebrew/bin/blender --background --python client/assets/models/bruiser.py -- --preview` from the repository root. This rebuilds the model and bakes its armor atlas; front, rear, and gameplay-distance renders go to `/tmp/bruiser-preview.png`, `/tmp/bruiser-rear.png`, and `/tmp/bruiser-gameplay.png`. Add `--motion` for animation frames.

Edit [bruiser.json](bruiser.json) before rebuilding. Its `materials` section controls rubber, fittings, markings, and optics; [shared controls](MATERIALS.md) describe those fields. The `wear` section configures the armor bake in [model_wear.py](model_wear.py): `edge_width` limits chip depth in metres, `edge_chip_threshold` controls rarity (higher means fewer), and `chip_sites` places localized impact chips with model-space `center` and `extent` vectors. The base paint color, roughness, and metallic value come from `materials.armour`. The `wear` section adds paint variation, exposed-metal color and metallic value, roughness changes, scratch depth, and normal strength. Colors are linear RGB. Tires use the approved FreePBR synthetic rubber with model-specific scale and normal relief.

The three PNGs in [bruiser_textures/](bruiser_textures/) are created here and embedded in `bruiser.glb`:

| Map | Encoding |
| --- | --- |
| `bruiser-albedo.png` | sRGB base color |
| `bruiser-metallic-roughness.png` | Linear data: red = ambient occlusion, green = roughness, blue = metallic |
| `bruiser-normal-gl.png` | Linear tangent-space normal, OpenGL Y convention |

These are model-specific UV atlases, not tiling wall/floor materials. The game loads the embedded maps when it loads the model; the separate PNGs are retained as reproducible asset outputs and are not independently loaded by the client. Small fittings use plain materials rather than extra texture sets.
