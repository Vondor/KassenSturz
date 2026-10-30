// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One entry of the menu that adds a component to the layout (M6b.3): its
// name is the plain word for the kind (`GridNames`), and choosing it says
// which kind.

import org.kde.kirigami as Kirigami

Kirigami.Action {
    // `chart.line`, `kpi.card`, `text.heading`, ...
    required property string kind

    signal chosen(string kind)

    text: names.typeName(kind)
    onTriggered: chosen(kind)

    readonly property GridNames names: GridNames {}
}
