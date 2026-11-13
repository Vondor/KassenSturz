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
trap '[ -n "${KEEP_CHARTS:-}" ] && cp -r "$work/out" "$KEEP_CHARTS"; rm -rf "$work"' EXIT
qml=crates/kassensturz-ui-bridge/qml
cp "$qml"/FlowChart.qml crates/kassensturz-ui-bridge/qml-tests/FlowSmoke.qml "$work"
cp "$qml"/ChartView.qml "$qml"/ChartOverlays.qml "$qml"/ChartTable.qml "$qml"/ChartPalette.qml \
    "$qml"/DataTable.qml "$qml"/AmountLabel.qml \
    crates/kassensturz-ui-bridge/qml-tests/ChartsSmoke.qml "$work"
printf 'singleton ChartPalette 1.0 ChartPalette.qml\nChartView 1.0 ChartView.qml\nChartOverlays 1.0 ChartOverlays.qml\nChartTable 1.0 ChartTable.qml\nDataTable 1.0 DataTable.qml\nAmountLabel 1.0 AmountLabel.qml\nFlowChart 1.0 FlowChart.qml\n' \
    > "$work/qmldir"
mkdir "$work/out"

(
    cd "$work"
    LIBGL_ALWAYS_SOFTWARE=1 QSG_RHI_BACKEND=opengl QT_QPA_PLATFORM=xcb \
        timeout 120 xvfb-run -a -s "-screen 0 1280x1024x24" qml6 ChartsSmoke.qml 2>&1 | tee run.log
    # A layout that sizes itself from its own width loops (M6-B11).
    if grep -E 'recursive rearrange|Binding loop' run.log; then
        echo "check-charts: QML printed a layout or binding loop (above)" >&2
        exit 1
    fi
    # The names on the slices of a pie or donut never cover each other.
    if grep -E 'name-overlaps .* [1-9][0-9]*$' run.log; then
        echo "check-charts: names on slices cover each other (above)" >&2
        exit 1
    fi
    # Plain bars go in rows when a name's word would not fit under its bar,
    # and stay upright otherwise (M6-B3, M6-B229).
    for want in '2-bar false' '4-bar true' '18-bar true' '19-bar false'; do
        grep -q "bar-rows $want\$" run.log || { echo "check-charts: expected bar-rows $want" >&2; exit 1; }
    done
    # The average bar beside a series that is zero everywhere has the slot
    # to itself, in the middle of its name (#709, US4 P3 R3b).
    grep -q 'bar-middle 19-bar 500$' run.log || { echo "check-charts: expected the lone bar of 19-bar in the middle" >&2; exit 1; }
    # The candle legend follows the chosen boxes (D-748).
    for want in 'up-hollow hollow,filled' 'down-hollow filled,hollow' 'all-filled filled,filled' 'all-hollow hollow,hollow'; do
        grep -q "candle-legend $want\$" run.log || { echo "check-charts: expected candle-legend $want" >&2; exit 1; }
    done
    # *Show the legend* switched off hides the legend (US5 Phase 3).
    grep -q 'legend-shown off false$' run.log || { echo "check-charts: the legend stayed with the switch off" >&2; exit 1; }
    grep -q 'legend-shown on true$' run.log || { echo "check-charts: the legend is missing with the switch on" >&2; exit 1; }
    # The candles' explanation is shown only where it is asked for.
    grep -q 'candle-help off false$' run.log || { echo "check-charts: the candle explanation stayed with explain off" >&2; exit 1; }
    grep -q 'candle-help on true$' run.log || { echo "check-charts: the candle explanation is missing with explain on" >&2; exit 1; }
    grep -c 'name-overlaps' run.log | grep -qv '^0$' || { echo "check-charts: no name check ran" >&2; exit 1; }
)

# The money-flow chart (US7, #821): drawn from made-up places, and a band
# found under a point of the picture.
(
    cd "$work"
    mkdir -p flow/out
    cp FlowChart.qml ChartPalette.qml DataTable.qml AmountLabel.qml FlowSmoke.qml qmldir flow/
    cd flow
    LIBGL_ALWAYS_SOFTWARE=1 QSG_RHI_BACKEND=opengl QT_QPA_PLATFORM=xcb \
        timeout 120 xvfb-run -a -s "-screen 0 1280x1024x24" qml6 FlowSmoke.qml 2>&1 | tee run.log
    if grep -E 'recursive rearrange|Binding loop' run.log; then
        echo "check-charts: the money-flow chart printed a layout or binding loop (above)" >&2
        exit 1
    fi
    grep -q 'flow-columns 3$' run.log || { echo "check-charts: the money-flow chart has the wrong columns" >&2; exit 1; }
    grep -q 'flow-band-at 0$' run.log || { echo "check-charts: no band under the point of the salary band" >&2; exit 1; }
    # A click opens the transactions of the outer category of what was hit;
    # the middle and the balance node open everything (D-773).
    grep -q 'flow-behind b,a,c,,$' run.log || { echo "check-charts: a click on the money flow opens the wrong category" >&2; exit 1; }
    grep -q 'flow-scope \[\["b"\],true,\["x"\],true,true\]$' run.log || { echo "check-charts: the scope behind a click is not narrowed to its category" >&2; exit 1; }
    grep -q 'flow-scope-all true$' run.log || { echo "check-charts: a click on no category narrows the scope" >&2; exit 1; }
    grep -q 'flow-text Salary to Your money: ' run.log || { echo "check-charts: a band does not say where it goes" >&2; exit 1; }
    cp out/*.png "$work/out/"
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
green, pink = (0x00, 0x9E, 0x73), (0xCC, 0x79, 0xA7)
# Thin marks (a line, a level, a few dots) have fewer pixels than bars.
thin = {"0-line", "1-area", "11-line", "16-stacked-area"}
custom_thin = {
    ("15-bar", orange): 300,
    ("15-bar", green): 100,
    ("17-bar", blue): 300,
    ("17-bar", pink): 300,
    # A series that is zero everywhere has no bar, only its legend mark
    # (US4 P3 R3b).
    ("19-bar", blue): 20,
    # Candles (US4.10): the rising ones are hollow, only their frame and
    # line are blue.
    ("20-candlestick", blue): 300,
    ("20-candlestick", orange): 300,
}
# A lone bar is never wider than three grid units (M6-B229): far fewer
# pixels than half the picture.
widest = {("19-bar", orange): 40000}
expected = {
    "0-line": [blue, orange],
    "1-area": [blue, orange],
    "2-bar": [blue, orange],
    "3-stacked-bar": [blue, orange],
    "4-bar": [blue],
    "5-pie": [blue, orange],
    "6-donut": [blue, orange],
    "11-line": [blue],
    "12-bar": [blue, orange],
    # Custom charts (M6e.2): bars with a level, dots and texts over them;
    # stacked areas; overlays alone on an empty chart.
    "15-bar": [blue, orange, green],
    "16-stacked-area": [blue, orange],
    "17-bar": [blue, green, pink],
    # Spending compared with an average (M6-B229): rows, and one lone bar.
    "18-bar": [blue, orange],
    "19-bar": [blue, orange],
    # The balance as candles: up hollow in blue, down filled in orange.
    "20-candlestick": [blue, orange],
}
# The money flow (US7): the salary node and the rent node are blue, food
# orange.
flow_expected = {"flow-chart": [blue, orange]}
failed = False
for name, colors in flow_expected.items():
    picture = pixels(os.path.join(out, name + ".png"))
    for color in colors:
        found = count(picture, color)
        print(f"{name}: {found} pixels of #{color[0]:02X}{color[1]:02X}{color[2]:02X}")
        if found < 300:
            print("  too few: the chart was not drawn")
            failed = True
    if not os.path.exists(os.path.join(out, name.replace("chart", "table") + ".png")):
        print("missing picture:", name, "as a table")
        failed = True
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
        least = custom_thin.get((name, color), 300 if name in thin else 3000)
        if found < least:
            print("  too few: the chart was not drawn")
            failed = True
        if found > widest.get((name, color), found):
            print("  too many: the bar is too wide")
            failed = True
sys.exit(1 if failed else 0)
PY
