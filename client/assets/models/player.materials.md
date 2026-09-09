# Player material tuning

Edit `player.materials.json`, then rebuild from the repository root:

```sh
/opt/homebrew/bin/blender --background --python client/assets/models/player.py -- --rear-preview
```

This writes `player.glb` with embedded textures and renders `/tmp/player-detail.png` and `/tmp/player-rear-head.png`. Restart the client to inspect the result in-game. These overrides affect only the player; the source textures and the materials used for maps and other actors stay independent.

Material entries live under `materials`; see [the shared settings reference](MATERIALS.md). `joint` selects the FreePBR synthetic rubber texture set. The white shells (`ivory`), steel, visor, chassis, markings, and lights have plain material entries in the same JSON.

`materials.joint` uses a tile size of 0.3 metres and 0.15 normal strength for fine, shallow grain on flexible parts. Tires have independent, coarser settings in the bruiser and scuttler JSON files.

The rubber source is the approved FreePBR `synth-rubber` catalog entry. The generator embeds a darker, lower-contrast variant for flexible parts; see [asset provenance](../ASSETS.md).
