#!/bin/bash

# Adjust metallic-roughness texture intensity
# For ROUGHNESS (green channel): Higher values = rougher/less shiny
# For METALLIC (blue channel): Higher values = more metallic
# 
# Usage: ./multiply_intensity.sh [roughness_adjust] [metallic_multiply]
# roughness_adjust: value to ADD to roughness (e.g., 0.3 to make 30% rougher)
# metallic_multiply: multiplier for metallic (e.g., 0.5 to reduce metallic by half)
# 
# Example: ./multiply_intensity.sh 0.3 0.5
#   - Makes surfaces 30% rougher (less shiny/shimmer)
#   - Reduces metallic effect by half

ROUGHNESS_ADD=${1:-0.3}
METALLIC_MULT=${2:-1.0}

echo "Adjusting metallic-roughness textures..."
echo "  Roughness: +${ROUGHNESS_ADD} (higher = less shiny)"
echo "  Metallic: ×${METALLIC_MULT}"

for dir in cookie item wall roof ground; do
    if [ -f "$dir/metallic-roughness.png" ]; then
        echo "Processing $dir/metallic-roughness.png..."
        
        # Create backup
        if [ ! -f "$dir/metallic-roughness.original.png" ]; then
            cp "$dir/metallic-roughness.png" "$dir/metallic-roughness.original.png"
            echo "  Created backup: $dir/metallic-roughness.original.png"
        fi
        
        # Extract channels, adjust separately, then recombine
        # Red channel: unused (keep as-is)
        # Green channel: roughness - ADD value to make rougher
        # Blue channel: metallic - MULTIPLY to reduce/increase
        magick "$dir/metallic-roughness.original.png" \
            \( -clone 0 -channel R -separate \) \
            \( -clone 0 -channel G -separate -evaluate add ${ROUGHNESS_ADD} \) \
            \( -clone 0 -channel B -separate -evaluate multiply ${METALLIC_MULT} \) \
            -delete 0 -channel RGB -combine \
            "$dir/metallic-roughness.png"
        
        echo "  Adjusted (roughness +${ROUGHNESS_ADD}, metallic ×${METALLIC_MULT})"
    else
        echo "  Skipping $dir (no metallic-roughness.png)"
    fi
done

echo "Done!"
echo "To restore originals: cp */metallic-roughness.original.png */metallic-roughness.png"
