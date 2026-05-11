#!/bin/bash
set -euo pipefail

# Combine metallic and roughness textures for Bevy's PBR workflow
# Creates metallic-roughness.png where:
# - Red channel: unused (set to 0)
# - Green channel: roughness
# - Blue channel: metallic
#
# Usage:
#   ./combine_metallic_roughness.sh <roughness.png> <metallic.png> [output.png]
#
# output.png: optional; defaults to replacing "roughness.png" in the roughness
#             filename with "metallic-roughness.png"
#
# Example:
#   ./combine_metallic_roughness.sh \
#     path/to/texture_roughness.png \
#     path/to/texture_metallic.png

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    echo "Usage: $0 <roughness.png> <metallic.png> [output.png]" >&2
    exit 2
fi

roughness=$1
metallic=$2
output=${3:-}

if [ ! -f "$roughness" ]; then
    echo "Missing roughness texture: $roughness" >&2
    exit 1
fi

if [ ! -f "$metallic" ]; then
    echo "Missing metallic texture: $metallic" >&2
    exit 1
fi

if [ -z "$output" ]; then
    # Strip a trailing `roughness.png` (case-insensitive) and replace with
    # `metallic-roughness.png`. `${var%pat}` is case-sensitive in bash, so we
    # match the suffix manually with a case statement and then strip a fixed
    # number of trailing characters.
    case "$roughness" in
        *[rR][oO][uU][gG][hH][nN][eE][sS][sS].png)
            stem=${roughness%?????????????}  # 13 chars: "roughness.png"
            output="${stem}metallic-roughness.png"
            ;;
        *)
            echo "Could not derive output path from roughness filename: $roughness" >&2
            echo "Pass output.png explicitly, or use a roughness filename ending in roughness.png." >&2
            exit 2
            ;;
    esac
fi

echo "Combining metallic and roughness textures..."
echo "  Roughness: $roughness"
echo "  Metallic:  $metallic"
echo "  Output:    $output"

dimensions=$(magick identify -format '%wx%h' "$roughness")

magick -size "$dimensions" xc:black "$roughness" "$metallic" \
    -channel RGB -combine \
    -strip PNG24:"$output"

echo "Done!"
