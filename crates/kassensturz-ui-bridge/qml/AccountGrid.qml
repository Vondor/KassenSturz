// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The account cards of one currency, or of the closed accounts. Every group
// on the Accounts page uses this one grid, so that the groups line up the
// same way: cards start at the left edge, and a card is as wide as a cell of
// the grid however many cards the group has (M5-F3). Kirigami's
// `CardsLayout` centred a group with many cards and put a lone card at the
// left with another width.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Item {
    id: grid

    // The accounts of the group, as the Rust side gives them.
    property var accounts: []

    // Emitted with the ID of the account the user clicked.
    signal accountClicked(var accountId)

    readonly property int cardMinWidth: Kirigami.Units.gridUnit * 18
    readonly property int cardMaxColumns: 4
    readonly property real spacing: Kirigami.Units.largeSpacing
    // The same for every group, whatever its number of cards.
    readonly property int columns: Math.max(1, Math.min(cardMaxColumns, Math.floor((width + spacing) / (cardMinWidth + spacing))))
    readonly property real cellWidth: (width - (columns - 1) * spacing) / columns

    Layout.fillWidth: true
    implicitHeight: cells.implicitHeight

    GridLayout {
        id: cells

        // Sized from the group's width, not from the cards in it, so that the
        // cards cannot change their own cell width.
        width: grid.width
        columns: grid.columns
        columnSpacing: grid.spacing
        rowSpacing: grid.spacing

        Repeater {
            model: grid.accounts

            delegate: AccountCard {
                Layout.preferredWidth: grid.cellWidth
                Layout.maximumWidth: grid.cellWidth
                Layout.fillHeight: true
                Layout.alignment: Qt.AlignTop
                onClicked: {
                    ActionLog.taken("AccountGrid_accountClicked")
                    grid.accountClicked(modelData.id)
                }
            }
        }
    }
}
