// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton

import QtQuick

// What a theme plugin changes besides the colours (D-925), and the switches
// of a UI density plugin (D-927). Main.qml binds `themeRadius` to the
// `cornerRadius` of the `ThemeSettings` object and the switches to the
// `DensitySettings` object; the defaults, which also hold wherever a screen
// runs without the app, keep KassenSturz's own look.
QtObject {
    // The theme's corner radius in logical pixels, or -1 for none.
    property int themeRadius: -1

    // Tighter rows in lists and registers.
    property bool compactLists: false
    // Hides the grey explanations of `HintText`, and nothing else: warnings,
    // errors and questions before a change never use it.
    property bool hideHints: false
    // Smaller gaps between the start screen's tiles.
    property bool compactTiles: false

    // The padding above and below a list's row while `compactLists` is on.
    readonly property real compactRowPadding: 1

    // The radius of a box whose own radius is `standard`.
    function radius(standard: real): real {
        return themeRadius >= 0 ? themeRadius : standard;
    }

    // The gap between tiles whose own gap is `standard`.
    function tileGap(standard: real): real {
        return compactTiles ? Math.round(standard / 3) : standard;
    }
}
