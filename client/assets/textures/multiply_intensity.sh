#!/bin/bash
set -euo pipefail

# Adjust metallic-roughness texture intensity
# For ROUGHNESS (green channel): Higher values = rougher/less shiny
# For METALLIC (blue channel): Higher values = more metallic
# 
# Usage:
#   ./multiply_intensity.sh <metallic-roughness.png> [roughness_adjust] [metallic_multiply] [output.png]
#
# roughness_adjust: value to ADD to roughness (e.g., 0.3 to make 30% rougher)
# metallic_multiply: multiplier for metallic (e.g., 0.5 to reduce metallic by half)
# output.png: optional; defaults to overwriting the input file after creating a
#             .original.png backup next to it
# 
# Example:
#   ./multiply_intensity.sh path/to/texture_metallic-roughness.png 0.3 0.5
#   - Makes surfaces 30% rougher (less shiny/shimmer)
#   - Reduces metallic effect by half

if [ "$#" -lt 1 ] || [ "$#" -gt 4 ]; then
    echo "Usage: $0 <metallic-roughness.png> [roughness_adjust] [metallic_multiply] [output.png]" >&2
    exit 2
fi

input=$1
roughness_add=${2:-0.3}
metallic_mult=${3:-1.0}
output=${4:-$input}

if [ ! -f "$input" ]; then
    echo "Missing metallic-roughness texture: $input" >&2
    exit 1
fi

source=$input
if [ "$output" = "$input" ]; then
    backup="${input%.png}.original.png"
    if [ ! -f "$backup" ]; then
        cp "$input" "$backup"
        echo "Created backup: $backup"
    fi
    source=$backup
fi

echo "Adjusting metallic-roughness textures..."
echo "  Input:     $input"
echo "  Source:    $source"
echo "  Output:    $output"
echo "  Roughness: +${roughness_add} (higher = less shiny)"
echo "  Metallic:  ×${metallic_mult}"

# Extract channels, adjust separately, then recombine:
# - Red channel: unused, kept as-is
# - Green channel: roughness, add value to make rougher
# - Blue channel: metallic, multiply to reduce/increase
magick "$source" \
    \( -clone 0 -channel R -separate \) \
    \( -clone 0 -channel G -separate -evaluate add "${roughness_add}" \) \
    \( -clone 0 -channel B -separate -evaluate multiply "${metallic_mult}" \) \
    -delete 0 -channel RGB -combine \
    "$output"

echo "Done!"
if [ "$output" = "$input" ]; then
    echo "To restore original: cp \"$backup\" \"$input\""
fi
