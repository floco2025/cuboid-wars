# Assets License Notice

**The assets in this directory are NOT open source.**

These assets (3D models, textures, sounds, images, etc.) are licensed separately from the game's source code and are provided for use in this game only.

If you are forking or using this codebase, you **must replace all assets** with your own or properly licensed alternatives. You may not redistribute or use these assets in other projects.

The game's source code is licensed under MIT OR Apache-2.0, but this license does NOT apply to the contents of this directory.

## Asset Set

`config/client/assets.json` is the client asset set. The client uses the full file for render/audio assets. The server reads the same file for `material_rules` and ignores client-only sections it does not need, such as `materials`, `models`, `sounds`, and texture file paths.

Asset paths are relative to `client/assets`.

For character models, `visual_y_offset` is relative to the gameplay collider
bottom. `0.0` means the model bottom sits on the collider bottom; negative
values place the model below the collider bottom, and positive values place it
above.

The most specific matching material rule wins. For example, a floor rule with `level` + `cols` + `rows` beats a floor rule with only `level`, and an exact wall edge beats a level-wide wall rule. A rule without selector fields is the fallback for that rule list. Two matching rules with the same specificity but different materials are an error.

Material assignment is part of map segmentation. The server must not merge floors or walls across different resolved material ids. The client should render each received floor/wall as one mesh with one material; if one segment spans multiple material rules, that is a map/merge error rather than a reason for the client to split the segment.

Editor coordinate selectors:

- Floors use cell coordinates: `"cols": [min, max]`, `"rows": [min, max]`, inclusive.
- Walls use grid-line edges: `"from": [col, row]`, `"to": [col, row]`.
- Ramps use their lower editor level.

Examples:

```json
{
  "material": "ground",
  "level": 3,
  "cols": [16, 19],
  "rows": [0, 3]
}
```

```json
{
  "material": "wall",
  "level": 2,
  "from": [4, 8],
  "to": [5, 8]
}
```
