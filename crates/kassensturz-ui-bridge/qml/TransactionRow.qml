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
    // What the user noted about it; empty when nothing was noted.
    property string note: ""
    // Wide pages show a row as the lines of a book, one column each: date,
    // from or to, category, payment, deposit and balance. `header` makes the
    // row the book's column titles instead, so that both line up.
    property bool ledger: false
    property bool header: false
    // Which columns a register shows, as the keys of `Column::key`
    // separated by commas (US1.19); the default ones elsewhere.
    property string columns: "date,other-party,category,amount,balance"
    // What a bank or file delivered, for the columns that show it. Plain
    // text from outside.
    property string otherPartyIban: ""
    property string purpose: ""
    property string valueDate: ""
    // Whether the balance column is there: not in any other order than by
    // date (US1.18).
    property bool balanceShown: true
    // In the column titles, what the list is sorted by and which way, and a
    // request to sort by a column (US1.18).
    property string sortKey: ""
    property bool descending: false
    signal sortRequested(string key)

    readonly property real dateWidth: Kirigami.Units.gridUnit * 7
    readonly property real amountWidth: Kirigami.Units.gridUnit * 8
    readonly property real nameWidth: Kirigami.Units.gridUnit * 10

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
    // A note from a bank statement can run over several lines; Text only
    // elides a single line, so the rest would spill into the next column.
    readonly property string oneLineNote: note.replace(/\s*\n\s*/g, " ")
    readonly property string balanceText: balance === "" || !balanceShown ? "" : balanceLabel(balance)
    readonly property string exactBalanceText: balance === "" || !balanceShown ? "" : balanceLabel(exactBalance)

    // Whether the register shows the column with this key.
    function showsColumn(key: string): bool {
        return columns.split(",").indexOf(key) >= 0;
    }

    // What a screen reader says after a column's title: how the list is
    // sorted by it, or that a click sorts by it.
    function sortHint(key: string): string {
        if (sortKey !== key) {
            return i18nContext.i18nc("@info a column title that sorts the list when it is clicked", "Sort by this column");
        }
        return descending
            ? i18nContext.i18nc("@info the list is sorted by this column, largest or latest first", "Sorted descending")
            : i18nContext.i18nc("@info the list is sorted by this column, smallest or earliest first", "Sorted ascending");
    }

    function balanceLabel(amount: string): string {
        return owes
            ? i18nContext.i18nc("@label %1 is an amount of money the user owed after this entry", "Then you owed: %1", amount)
            : i18nContext.i18nc("@label %1 is an amount of money the account held after this entry", "Then: %1", amount);
    }

    spacing: Kirigami.Units.largeSpacing
    Accessible.role: Accessible.ListItem
    Accessible.name: header
        ? ""
        : [title, details, note, exactAmount, exactBalanceText].filter(text => text !== "").join(", ")

    // The book's columns. In the column titles, the ones that can be sorted
    // by are buttons (US1.18).
    ColumnTitle {
        Layout.preferredWidth: row.dateWidth
        visible: row.ledger && row.header
        text: row.i18nContext.i18nc("@title:column", "Date")
        sortKey: "date"
        activeKey: row.sortKey
        descending: row.descending
        hint: row.sortHint("date")
        onPicked: key => row.sortRequested(key)
    }

    Controls.Label {
        textFormat: Text.PlainText
        Layout.preferredWidth: row.dateWidth
        visible: row.ledger && !row.header && row.showsColumn("date")
        text: row.date
        elide: Text.ElideRight
    }

    // The day the money counts from, if the book's column was chosen.
    Controls.Label {
        textFormat: Text.PlainText
        Layout.preferredWidth: row.dateWidth
        visible: row.ledger && row.showsColumn("value-date")
        text: row.header ? row.i18nContext.i18nc("@title:column the day the money counts from or until (German: Wertstellung)", "Value date") : row.valueDate
        font.bold: row.header
        elide: Text.ElideRight
    }

    // The note sits under the name, so a line stays one row where nothing
    // was noted.
    ColumnLayout {
        Layout.fillWidth: true
        Layout.preferredWidth: row.nameWidth
        Layout.minimumWidth: 0
        visible: row.ledger
        clip: true
        spacing: 0

        ColumnTitle {
            Layout.fillWidth: true
            visible: row.header
            text: row.i18nContext.i18nc("@title:column whom the money came from or went to", "From or to")
            sortKey: "other-party"
            activeKey: row.sortKey
            descending: row.descending
            hint: row.sortHint("other-party")
            onPicked: key => row.sortRequested(key)
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: !row.header
            text: row.kind === "starting-balance" ? row.title : row.otherParty
            elide: Text.ElideRight
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: !row.header && row.note !== ""
            text: row.oneLineNote
            elide: Text.ElideRight
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.disabledTextColor
        }
    }

    // What the bank delivered, if the book's columns were chosen. It is
    // text from outside, shown as plain text (AGENTS.md §8).
    Controls.Label {
        textFormat: Text.PlainText
        Layout.preferredWidth: row.nameWidth
        visible: row.ledger && row.showsColumn("other-party-iban")
        text: row.header ? row.i18nContext.i18nc("@title:column the IBAN of whom the money came from or went to", "Their IBAN") : row.otherPartyIban
        font.bold: row.header
        elide: Text.ElideRight
    }

    Controls.Label {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        Layout.preferredWidth: row.nameWidth
        Layout.minimumWidth: 0
        visible: row.ledger && row.showsColumn("purpose")
        text: row.header ? row.i18nContext.i18nc("@title:column what the payment came with (German: Verwendungszweck)", "Purpose") : row.purpose
        font.bold: row.header
        elide: Text.ElideRight
    }

    ColumnTitle {
        Layout.fillWidth: true
        Layout.preferredWidth: row.nameWidth
        Layout.minimumWidth: 0
        visible: row.ledger && row.header && row.showsColumn("category")
        text: row.i18nContext.i18nc("@title:column", "Category")
        sortKey: "category"
        activeKey: row.sortKey
        descending: row.descending
        hint: row.sortHint("category")
        onPicked: key => row.sortRequested(key)
    }

    Controls.Label {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        Layout.preferredWidth: row.nameWidth
        visible: row.ledger && !row.header && row.showsColumn("category")
        text: row.whatFor
        elide: Text.ElideRight
    }

    // A payment and a deposit each have their own column, so what went out
    // and what came in can be told apart at a glance. Both are sorted by
    // the amount.
    ColumnTitle {
        Layout.preferredWidth: row.amountWidth
        visible: row.ledger && row.header
        alignRight: true
        text: row.i18nContext.i18nc("@title:column money that went out", "Payment")
        sortKey: "amount"
        activeKey: row.sortKey
        descending: row.descending
        hint: row.sortHint("amount")
        onPicked: key => row.sortRequested(key)
    }

    AmountLabel {
        textFormat: Text.PlainText
        Layout.preferredWidth: row.amountWidth
        visible: row.ledger && !row.header
        horizontalAlignment: Text.AlignRight
        text: row.incoming ? "" : row.amount
        exactText: row.incoming ? "" : row.exactAmount
    }

    ColumnTitle {
        Layout.preferredWidth: row.amountWidth
        visible: row.ledger && row.header
        alignRight: true
        text: row.i18nContext.i18nc("@title:column money that came in", "Deposit")
        sortKey: "amount"
        activeKey: row.sortKey
        descending: row.descending
        hint: row.sortHint("amount")
        onPicked: key => row.sortRequested(key)
    }

    AmountLabel {
        textFormat: Text.PlainText
        Layout.preferredWidth: row.amountWidth
        visible: row.ledger && !row.header
        horizontalAlignment: Text.AlignRight
        text: row.incoming ? row.amount : ""
        exactText: row.incoming ? row.exactAmount : ""
        color: Kirigami.Theme.positiveTextColor
    }

    // What the account held after each entry has no title to sort by: it
    // is only true in date order, and goes away in any other (US1.18).
    AmountLabel {
        textFormat: Text.PlainText
        Layout.preferredWidth: row.amountWidth
        visible: row.ledger && row.balanceShown
        horizontalAlignment: Text.AlignRight
        text: row.header
            ? (row.owes
                ? row.i18nContext.i18nc("@title:column what you owed after the transaction", "You owed after")
                : row.i18nContext.i18nc("@title:column what the account held after the transaction", "Balance after"))
            : row.balance
        exactText: row.header ? text : row.exactBalance
        font.bold: row.header
    }

    // The same, as two lines, where there is not room for columns.
    ColumnLayout {
        Layout.fillWidth: true
        visible: !row.ledger
        spacing: 0

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            text: row.title
            elide: Text.ElideRight
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            text: row.details
            elide: Text.ElideRight
            font: Kirigami.Theme.smallFont
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: row.note !== ""
            text: row.oneLineNote
            elide: Text.ElideRight
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.disabledTextColor
        }
    }

    ColumnLayout {
        visible: !row.ledger
        spacing: 0

        AmountLabel {
            textFormat: Text.PlainText
            Layout.alignment: Qt.AlignRight
            text: row.amount
            exactText: row.exactAmount
            color: row.incoming ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor
        }

        AmountLabel {
            textFormat: Text.PlainText
            Layout.alignment: Qt.AlignRight
            visible: row.balance !== "" && row.balanceShown
            text: row.balanceText
            exactText: row.exactBalanceText
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.disabledTextColor
        }
    }

    // A column title in a register: a button that sorts the list by the
    // column, with an arrow while the list is sorted by it (US1.18).
    component ColumnTitle: Controls.AbstractButton {
        id: title

        // The key the list is sorted by when the title is clicked.
        property string sortKey: ""
        // The key it is sorted by now, and which way.
        property string activeKey: ""
        property bool descending: false
        property bool alignRight: false
        // Said after the title by a screen reader.
        property string hint: ""
        readonly property bool sorted: activeKey === sortKey

        signal picked(string key)

        hoverEnabled: true
        padding: 0
        Accessible.role: Accessible.Button
        Accessible.name: text
        Accessible.description: hint
        onClicked: {
            ActionLog.taken("TransactionRow_sortByColumn")
            title.picked(title.sortKey)
        }

        contentItem: RowLayout {
            spacing: Kirigami.Units.smallSpacing

            Item {
                Layout.fillWidth: true
                visible: title.alignRight
            }

            Controls.Label {
                textFormat: Text.PlainText
                Layout.fillWidth: !title.alignRight
                text: title.text
                font.bold: true
                elide: Text.ElideRight
            }

            Kirigami.Icon {
                visible: title.sorted
                source: title.descending ? "arrow-down" : "arrow-up"
                implicitWidth: Kirigami.Units.iconSizes.small
                implicitHeight: Kirigami.Units.iconSizes.small
            }
        }

        background: Rectangle {
            color: Kirigami.Theme.highlightColor
            opacity: title.hovered || title.visualFocus ? 0.3 : 0
        }
    }
}
