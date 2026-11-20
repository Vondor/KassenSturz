// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton

import QtQuick

// What a theme plugin changes besides the colours (D-925): the corner
// radius of KassenSturz's own boxes. Main.qml binds `themeRadius` to the
// `cornerRadius` of the `ThemeSettings` object; -1, the default and the
// value wherever a screen runs without the app, keeps each box's own.
QtObject {
    // The theme's corner radius in logical pixels, or -1 for none.
    property int themeRadius: -1

    // The radius of a box whose own radius is `standard`.
    function radius(standard: real): real {
        return themeRadius >= 0 ? themeRadius : standard;
    }
}
