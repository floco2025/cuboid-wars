#!/bin/bash

# Combine metallic and roughness textures for Bevy's PBR workflow
# Creates metallic-roughness.png where:
# - Red channel: unused (set to 0)
# - Green channel: roughness
# - Blue channel: metallic

echo "Combining metallic and roughness textures..."

for dir in cookie item wall roof ground; do
    if [ -d "$dir" ] && [ -f "$dir/roughness.png" ] && [ -f "$dir/metallic.png" ]; then
        echo "Processing $dir..."
        magick "$dir/roughness.png" "$dir/metallic.png" \
            \( +clone -evaluate set 0 \) +swap \
            -channel RGB -combine \
            "$dir/metallic-roughness.png"
        echo "  Created $dir/metallic-roughness.png"
    else
        echo "  Skipping $dir (missing files or directory)"
    fi
done

echo "Done!"
