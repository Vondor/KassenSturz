// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton

import QtQuick

// The colors of the charts (M6a.4, D-283): the Okabe-Ito set, which stays
// apart for the common kinds of color blindness, with grey in place of black
// so that it also reads on a dark theme. The first four series also have a
// second mark (`markerName`), so no meaning sits in the color alone. The
// number of colors is `paletteSize` in the chart (Rust: PALETTE_SIZE).
QtObject {
    readonly property var colors: [
        "#0072B2", "#D55E00", "#009E73", "#CC79A7",
        "#E69F00", "#56B4E9", "#F0E442", "#999999"
    ]

    function color(index: int): color {
        return colors[((index % colors.length) + colors.length) % colors.length];
    }

    // A second mark besides the color, by the `marker` of a series: a
    // square, a circle, a triangle or a diamond. -1 has none.
    function markerName(marker: int): string {
        return ["square", "circle", "triangle", "diamond"][marker] ?? "";
    }
}
