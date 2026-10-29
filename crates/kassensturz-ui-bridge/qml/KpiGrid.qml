// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Key figure cards side by side (M6-B2): as many to a row as the width
// allows, at most four, so that four cards sit in one row or in a 2 x 2 block
// and never in one narrow column of a wide window. Like `AccountGrid`, it is
// sized from its own width, not from the cards in it (Kirigami's
// `CardsLayout` stacked them in one column in a wide window).

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Item {
    id: grid

    // Objects like the cards of a `BookCards.view` set, each for a `KpiCard`.
    property var cards: []

    readonly property int cardMinWidth: Kirigami.Units.gridUnit * 11
    readonly property int cardMaxColumns: 4
    readonly property real spacing: Kirigami.Units.largeSpacing
    readonly property int columns: Math.max(1, Math.min(cardMaxColumns, Math.floor((width + spacing) / (cardMinWidth + spacing))))
    readonly property real cellWidth: (width - (columns - 1) * spacing) / columns

    Layout.fillWidth: true
    implicitHeight: cells.implicitHeight

    GridLayout {
        id: cells

        width: grid.width
        columns: grid.columns
        columnSpacing: grid.spacing
        rowSpacing: grid.spacing

        Repeater {
            model: grid.cards

            delegate: KpiCard {
                Layout.preferredWidth: grid.cellWidth
                Layout.maximumWidth: grid.cellWidth
                Layout.fillHeight: true
                Layout.alignment: Qt.AlignTop
            }
        }
    }
}
