// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton

import QtQuick

// The colors of the charts (M6a.4, D-283): the Okabe-Ito set, which stays
// apart for the common kinds of color blindness, with grey in place of black
// so that it also reads on a dark theme. The first four series also have a
// second mark (`markerName`), so no meaning sits in the color alone. The
// number of colors is `paletteSize` in the chart (Rust: PALETTE_SIZE).
//
// The user can change every color in Settings (M7a.10, D-589). The colors in
// use are `colors`: Main.qml binds them to the `colors` of the
// `ChartPaletteSettings` object, which holds `#RRGGBB` text that Rust has
// checked (D-065). Until then, and wherever a screen runs without the app
// (the smoke tests), they are the standard ones below.
QtObject {
    id: palette

    // Must match STANDARD_COLORS in kassensturz-charts (a test of the
    // bridge reads the Rust ones back).
    readonly property var standardColors: [
        "#0072B2", "#D55E00", "#009E73", "#CC79A7",
        "#E69F00", "#56B4E9", "#F0E442", "#999999"
    ]

    // The colors in use, as `#RRGGBB` text.
    property var colors: standardColors

    // A screen asks for the place where the colors are changed (a link such
    // as "Edit palette…"); Main.qml opens Settings.
    signal editRequested()

    // The color of series or slice `index`. An index from the palette's size
    // on is a free color of a custom chart, `extra[index - size]`, which a
    // description named as `#rrggbb` (Q-410); with no such color it wraps
    // round the palette.
    function color(index: int, extra: var): color {
        const size = colors.length;
        if (extra !== undefined && extra !== null && index >= size) {
            const free = extra[index - size];
            if (free !== undefined)
                return free;
        }
        return colors[((index % size) + size) % size];
    }

    // A second mark besides the color, by the `marker` of a series: a
    // square, a circle, a triangle or a diamond. -1 has none.
    function markerName(marker: int): string {
        return ["square", "circle", "triangle", "diamond"][marker] ?? "";
    }
}
