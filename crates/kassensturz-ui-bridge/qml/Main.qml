// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The walking skeleton's window (D-068): one account's transactions from the
// sample book. Amounts and dates arrive as finished text (D-065), so nothing
// here calculates with them.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: root

    // Translations come from the "kassensturz" catalog (D-034), set up from
    // QML alone, so no KDE Frameworks library is linked into the Rust program.
    // KI18n then warns once at startup that it "cannot filter events for
    // objects in a different thread": Qt loads its QML plugin on a loader
    // thread. Translating works; only switching languages while the app runs
    // wouldn't.
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The app's name; it isn't translated.
    title: "KassenSturz"
    width: Kirigami.Units.gridUnit * 32
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 20
    minimumHeight: Kirigami.Units.gridUnit * 20

    TransactionListModel {
        id: transactions
    }

    pageStack.initialPage: Kirigami.ScrollablePage {
        title: transactions.accountName

        header: Kirigami.InlineMessage {
            position: Kirigami.InlineMessage.Position.Header
            type: Kirigami.MessageType.Information
            visible: true
            text: root.i18nContext.i18nc("@info", "This is sample data. Nothing you see here is saved.")
        }

        footer: Controls.ToolBar {
            contentItem: Controls.Label {
                text: root.i18nContext.i18nc("@label %1 is an amount of money", "Balance: %1", transactions.balance)
                font.bold: true
                horizontalAlignment: Text.AlignRight
            }
        }

        ListView {
            model: transactions
            reuseItems: true

            delegate: Controls.ItemDelegate {
                id: delegate

                required property string date
                required property string otherParty
                required property string amount
                required property bool incoming

                // Money that came in shows who it came from, money paid shows
                // who it went to (D-034).
                readonly property string otherPartyText: otherParty === ""
                    ? ""
                    : incoming
                        ? root.i18nContext.i18nc("@label %1 is who the money came from", "From: %1", otherParty)
                        : root.i18nContext.i18nc("@label %1 is who the money went to", "To: %1", otherParty)

                width: ListView.view.width
                Accessible.name: [otherPartyText, date, amount].filter(text => text !== "").join(", ")

                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        Controls.Label {
                            Layout.fillWidth: true
                            text: delegate.otherPartyText
                            elide: Text.ElideRight
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            text: delegate.date
                            font: Kirigami.Theme.smallFont
                        }
                    }

                    Controls.Label {
                        text: delegate.amount
                        color: delegate.incoming ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor
                    }
                }
            }
        }
    }
}
