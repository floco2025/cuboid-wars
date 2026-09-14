# Exposed soil

`soil-albedo.png` is an opaque seamless soil-detail source generated with the
built-in OpenAI image-generation tool. The terrain shader applies a small
world-stable warp, continuous stochastic triangular tiling with stable random
offsets and quarter-turns, and slower color variation to hide repetition while
retaining fine relief derived from the texture luminance. The large
non-repeating brown-patch shapes remain procedural.

Generation prompt:

> Use case: photorealistic-natural. Asset type: seamless tileable albedo texture for a high-quality real-time game terrain material. Primary request: exposed temperate meadow soil, viewed perfectly straight down, covering roughly one square metre; dense natural detail comparable to a professional PBR material source. Style/medium: photorealistic diffuse color/albedo texture, not a rendered terrain scene. Composition/framing: square orthographic top-down crop, uniform detail density edge to edge, genuinely seamless/tileable on all four edges, no central focal point. Lighting/mood: completely flat neutral diffuse capture; no directional light, no cast shadows, no ambient-occlusion shading, no highlights, no vignette. Color palette: rich neutral dark-to-medium brown loam, slightly cool and earthy, restrained variation; not orange, beige, yellow, pale, or washed out. Materials/textures: compact fine loam and crumbly earth aggregates, sub-centimetre granules, scattered tiny dull pebbles, sparse fine root fibres and a few short dead-grass fragments, tiny darker organic particles; crisp high-frequency detail with plausible scale, no large repeated blobs. Constraints: color/albedo only; opaque RGB; seamless edges; natural random distribution; no visible lighting baked into the texture; no perspective; no depth of field; no text, logos, border, watermark, footprints, tire tracks, large stones, deep holes, cracks, puddles, leaves, flowers, or obvious repeated motifs. Avoid: smooth mud, plastic clay, blurry procedural noise, stamped pits, exaggerated bumps, game-art stylization, oversaturation.
