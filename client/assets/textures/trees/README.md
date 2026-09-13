# Tree foliage

`oak-leaf-spray.png` is an RGBA foliage cutout generated with the built-in OpenAI image-generation tool. Preserve its alpha channel. `client/src/map/trees.rs` builds the branching tree meshes and their three levels of detail at startup; instances share the meshes and materials.

Generation prompt:

> Use case: photorealistic-natural. Asset type: alpha-cutout foliage texture for efficient real-time 3D deciduous trees. Create one botanical leaf spray on an actually TRANSPARENT background, square 1024 by 1024. A slender brown oak twig starts at bottom center, forks into five smaller curved twigs, bearing about 35 small mature oak leaves with natural lobed edges. Spread fan-shaped, irregular airy silhouette, width about 85% of canvas and height 90%, entirely contained with clear transparent padding on all sides. Natural muted medium olive greens, a few younger yellow-green leaves, fine leaf veins, subtle variation and some turned leaf undersides. Leaves individually distinct, some overlap in small groups, many transparent gaps. Orthographic front view, diffuse even ambient light, no cast shadow, no dramatic directional highlights, no background haze, no pot, no ground, no whole tree, no text, no border. This will be repeated on small planes as tree foliage, so keep stems fine and foliage detailed, crisp, natural, not cartoon and not a solid green blob.
