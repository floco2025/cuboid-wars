# Meadow ground

`meadow-albedo.png` is an opaque repeating grass texture generated with the built-in OpenAI image-generation tool. The terrain material blends it with the existing soil texture. `client/src/map/terrain_surface.rs` generates the grass, soil, and dry-cover mask shared with the nearby grass tufts.

Generation prompt:

> Use case: photorealistic-natural. Asset type: seamless tileable albedo texture for a real-time 3D temperate meadow ground material. Square 1024 by 1024. Perfect orthographic straight-down view of about one square metre of dense short meadow grass, natural fine narrow blades bending in varied directions. Muted mid-green and olive-green blades, scattered subdued straw-coloured dead grass and tiny dark earthy gaps beneath. Fine-scale irregular botanical detail; no giant blades, no clumps taller than lawn grass. Even diffuse neutral lighting, NO cast shadows, NO sunlit highlights, NO vignette, NO directional lighting, NO perspective. Color texture only, no large-scale patches or obvious repeated motifs, similar density and average colour throughout so it repeats unobtrusively. Natural healthy early-autumn park lawn, not saturated lime, not moss, not plastic, no flowers, no stones, no leaves or twigs, no objects, no text, no border. All four edges must tile seamlessly. Opaque RGB image.
