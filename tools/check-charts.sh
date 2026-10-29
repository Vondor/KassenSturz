#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks that KQuickCharts is installed and draws (M6a.4, D-275): runs
# crates/kassensturz-ui-bridge/qml-tests/ChartsSmoke.qml, which shows every
# kind of chart with made-up values, as a picture and as a table, under Xvfb
# with Mesa's software renderer, and then looks at the pictures. Under Qt's
# `offscreen` platform the chart items stay blank, so this needs `xvfb-run`,
# `qml6`, `python3`, the `org.kde.quickcharts` QML module and Mesa's DRI
# drivers (Debian: xvfb, qml-qt6, python3-minimal, qml6-module-org-kde-quickcharts,
# libgl1-mesa-dri).
#
#   tools/check-charts.sh
#
# It fails if the module can't be imported, if a picture is missing, or if a
# chart's first series color, which no text or line in the picture has, is not
# in the picture.

set -eu

cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
qml=crates/kassensturz-ui-bridge/qml
cp "$qml"/ChartView.qml "$qml"/ChartTable.qml "$qml"/ChartPalette.qml \
    crates/kassensturz-ui-bridge/qml-tests/ChartsSmoke.qml "$work"
printf 'singleton ChartPalette 1.0 ChartPalette.qml\nChartView 1.0 ChartView.qml\nChartTable 1.0 ChartTable.qml\n' \
    > "$work/qmldir"
mkdir "$work/out"

(
    cd "$work"
    LIBGL_ALWAYS_SOFTWARE=1 QSG_RHI_BACKEND=opengl QT_QPA_PLATFORM=xcb \
        timeout 120 xvfb-run -a -s "-screen 0 1280x1024x24" qml6 ChartsSmoke.qml
)

python3 - "$work/out" <<'PY'
import os
import struct
import sys
import zlib

out = sys.argv[1]


def pixels(path):
    """The RGB triples of an 8-bit RGB or RGBA PNG, row by row."""
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", path
    pos, idat, width, height, kind = 8, b"", 0, 0, 0
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        name = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        if name == b"IHDR":
            width, height, depth, kind = struct.unpack(">IIBB", body[:10])
            assert depth == 8 and kind in (2, 6), (depth, kind)
        elif name == b"IDAT":
            idat += body
        pos += 12 + length
    step = 4 if kind == 6 else 3
    raw = zlib.decompress(idat)
    stride = width * step
    rows, previous = [], bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        filt, line = raw[start], bytearray(raw[start + 1:start + 1 + stride])
        for x in range(stride):
            left = line[x - step] if x >= step else 0
            up = previous[x]
            upleft = previous[x - step] if x >= step else 0
            if filt == 1:
                line[x] = (line[x] + left) & 255
            elif filt == 2:
                line[x] = (line[x] + up) & 255
            elif filt == 3:
                line[x] = (line[x] + (left + up) // 2) & 255
            elif filt == 4:
                p = left + up - upleft
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - upleft)
                best = left if pa <= pb and pa <= pc else (up if pb <= pc else upleft)
                line[x] = (line[x] + best) & 255
        rows.append(line)
        previous = line
    return [[tuple(row[i:i + 3]) for i in range(0, stride, step)] for row in rows]


def count(picture, color):
    return sum(row.count(color) for row in picture)


# The Okabe-Ito colors of ChartPalette.qml: the first two series.
blue, orange = (0x00, 0x72, 0xB2), (0xD5, 0x5E, 0x00)
expected = {
    "0-line": [blue, orange],
    "1-area": [blue, orange],
    "2-bar": [blue, orange],
    "3-stacked-bar": [blue, orange],
    "4-bar": [blue],
    "5-pie": [blue, orange],
    "6-donut": [blue, orange],
}
failed = False
for name, colors in expected.items():
    for suffix in ("", "-table"):
        path = os.path.join(out, name + suffix + ".png")
        if not os.path.exists(path):
            print("missing picture:", name + suffix)
            failed = True
    picture = pixels(os.path.join(out, name + ".png"))
    for color in colors:
        found = count(picture, color)
        print(f"{name}: {found} pixels of #{color[0]:02X}{color[1]:02X}{color[2]:02X}")
        # A line is thin, and an area's fill is see-through; bars and slices
        # are solid.
        if found < (300 if name in ("0-line", "1-area") else 3000):
            print("  too few: the chart was not drawn")
            failed = True
sys.exit(1 if failed else 0)
PY
