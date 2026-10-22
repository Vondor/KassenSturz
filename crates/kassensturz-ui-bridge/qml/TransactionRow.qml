// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One transaction in a list: who, what for, when, and how much. Money that
// came in shows who it came from, money paid shows who it went to (D-034).
// In a register the row also says what the account held after it (D-118).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

RowLayout {
    id: row

    // What the Rust side wrote for the row, all as finished text (D-065).
    property string date: ""
    property string otherParty: ""
    property string category: ""
    property string amount: ""
    property string exactAmount: amount
    property bool incoming: false
    // What kind of entry it is: simple, starting-balance, transfer or parts.
    property string kind: "simple"
    // On the overview, which account it belongs to.
    property string account: ""
    // In a register, what the account held after this entry; empty on the
    // overview.
    property string balance: ""
    property string exactBalance: balance
    property bool owes: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property string title: kind === "starting-balance"
        ? i18nContext.i18nc("@label", "Starting balance")
        : otherParty === ""
            ? whatFor
            : incoming
                ? i18nContext.i18nc("@label %1 is who the money came from", "From: %1", otherParty)
                : i18nContext.i18nc("@label %1 is who the money went to", "To: %1", otherParty)
    // The category, or for a Transfer the other account, or that the entry
    // has several Parts or no category yet.
    readonly property string whatFor: kind === "transfer"
        ? (incoming
            ? i18nContext.i18nc("@label %1 is the name of an account", "Moved from %1", category)
            : i18nContext.i18nc("@label %1 is the name of an account", "Moved to %1", category))
        : kind === "parts"
            ? i18nContext.i18nc("@label", "Several parts")
            : kind === "starting-balance"
                ? ""
                : category === ""
                    ? i18nContext.i18nc("@label the transaction has no category yet", "Not sorted yet")
                    : category
    // When, what for, and on the overview which account it belongs to.
    readonly property string details: [date, otherParty === "" ? "" : whatFor, account]
        .filter(text => text !== "").join(" · ")
    readonly property string balanceText: balance === "" ? "" : balanceLabel(balance)
    readonly property string exactBalanceText: balance === "" ? "" : balanceLabel(exactBalance)

    function balanceLabel(amount: string): string {
        return owes
            ? i18nContext.i18nc("@label %1 is an amount of money the user owed after this entry", "Then you owed: %1", amount)
            : i18nContext.i18nc("@label %1 is an amount of money the account held after this entry", "Then: %1", amount);
    }

    spacing: Kirigami.Units.largeSpacing
    Accessible.role: Accessible.ListItem
    Accessible.name: [title, details, exactAmount, exactBalanceText].filter(text => text !== "").join(", ")

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 0

        Controls.Label {
            Layout.fillWidth: true
            text: row.title
            elide: Text.ElideRight
        }

        Controls.Label {
            Layout.fillWidth: true
            text: row.details
            elide: Text.ElideRight
            font: Kirigami.Theme.smallFont
        }
    }

    ColumnLayout {
        spacing: 0

        AmountLabel {
            Layout.alignment: Qt.AlignRight
            text: row.amount
            exactText: row.exactAmount
            color: row.incoming ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor
        }

        AmountLabel {
            Layout.alignment: Qt.AlignRight
            visible: row.balance !== ""
            text: row.balanceText
            exactText: row.exactBalanceText
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.disabledTextColor
        }
    }
}
