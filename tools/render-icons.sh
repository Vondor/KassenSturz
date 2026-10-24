#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Draws the app icon's PNG sizes from its SVG (D-163):
#
#   tools/render-icons.sh
#
# The SVG in data/icons/hicolor/scalable/apps/ is the original; the PNGs next
# to it are for places that can't draw SVG. Needs `rsvg-convert` (librsvg).
# Run it after changing the SVG and commit the result; the icon test
# (`app_icon_audit.rs`) checks that every size is there and square.

set -eu

cd "$(dirname "$0")/.."

id=io.github.vondor.KassenSturz
icons=data/icons/hicolor
source=$icons/scalable/apps/$id.svg

command -v rsvg-convert >/dev/null || {
    echo "render-icons: rsvg-convert is missing (package librsvg2-bin or librsvg)" >&2
    exit 1
}

for size in 16 22 24 32 48 64 128 256; do
    mkdir -p "$icons/${size}x${size}/apps"
    rsvg-convert --width "$size" --height "$size" --format png \
        --output "$icons/${size}x${size}/apps/$id.png" "$source"
done
