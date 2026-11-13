// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The transactions behind a click on the money-flow chart (US7, #821, D-773),
// listed as the Transactions tile lists them: the rows, their order and every
// amount come from `BookTransactions`; nothing opens from here. Made by
// `FlowChart.qml` when it is first needed.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: behind

    readonly property KI18nContext i18nContext: KI18nContext {}

    // What `FlowChart.qml` sets before it opens: the period, the scope text
    // narrowed to what was clicked, and the words above the list.
    property string period: ""
    property string scopeText: ""
    property string heading: ""

    title: Plain.plain(heading)
    padding: Kirigami.Units.largeSpacing
    preferredWidth: Kirigami.Units.gridUnit * 34
    preferredHeight: Kirigami.Units.gridUnit * 24
    standardButtons: Kirigami.Dialog.Close
    onOpened: list.ask()

    ColumnLayout {
        id: list

        readonly property var shown: feed.view?.rows ?? []
        readonly property int total: feed.view?.total ?? 0
        readonly property bool failed: feed.view?.error !== undefined

        function ask() {
            feed.setScope(behind.scopeText);
            feed.show(behind.period, "");
        }

        spacing: 0

        BookTransactions {
            id: feed
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !list.failed && list.total === 0 && feed.view !== undefined
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Kirigami.Theme.disabledTextColor
            text: behind.i18nContext.i18nc("@info", "No transactions behind this in this period.")
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: list.failed
            type: Kirigami.MessageType.Warning
            text: behind.i18nContext.i18nc("@info", "This couldn't be shown right now. Try another period, or open the page again.")
        }

        Repeater {
            model: list.shown

            delegate: TransactionRow {
                required property var modelData

                Layout.fillWidth: true
                date: modelData.date
                otherParty: modelData.otherParty
                category: modelData.other
                amount: modelData.amount
                exactAmount: modelData.exact
                incoming: modelData.incoming
                kind: modelData.kind
                account: feed.view?.severalAccounts ? modelData.account : ""
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            visible: list.total > list.shown.length
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.disabledTextColor
            text: behind.i18nContext.i18nc("@info %1 is how many are listed, %2 how many there are", "Showing the newest %1 of %2.", list.shown.length, list.total)
        }
    }
}
