// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What the AI may do (D-252): a grid of one row per part of the book
// (categories, Rules, accounts, transactions) and one column per kind of
// action (read, add, change, delete). Everything is off until the user
// switches it on. Changing or deleting needs the reading of its row, so
// switching one on shows Read on and keeps it there. Under the transactions
// a list of fields can be locked against the AI. The chat and the user's own
// AI tools read the same settings. Whatever the AI prepares is shown to the
// user and applied only after the user confirms it. Hidden with the rest of
// AI while the switch is off (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: grid

    required property AiSwitch ai
    required property AiSettings settings

    // Told after each choice: whether the settings file could be written.
    signal saved(bool ok)

    visible: ai.enabled

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Raised after each choice so the switches read their state again.
    property int revision: 0

    // The cells with something behind them. A cell with no key is one the AI
    // can't do (yet): "later" is shown greyed, "never" says it is not offered.
    readonly property var rows: [
        {
            name: i18nContext.i18nc("@label a part of the book", "Categories"),
            cells: [
                { key: "AllowReadCategories", read: true },
                { key: "AllowNewCategories" },
                { key: "AllowEditCategories", needsRead: true },
                { key: "AllowDeleteCategories", needsRead: true }
            ]
        },
        {
            name: i18nContext.i18nc("@label a part of the book", "Rules"),
            cells: [
                { key: "AllowReadRules", read: true },
                { key: "AllowNewRules" },
                { key: "AllowEditRules", needsRead: true },
                { key: "AllowDeleteRules", needsRead: true }
            ]
        },
        {
            name: i18nContext.i18nc("@label a part of the book", "Accounts"),
            cells: [
                { key: "AllowReadAccounts", read: true },
                { key: "AllowNewAccounts" },
                { key: "AllowEditAccounts", needsRead: true },
                { key: "", absent: "never" }
            ]
        },
        {
            name: i18nContext.i18nc("@label a part of the book", "Transactions"),
            key: "Transactions",
            cells: [
                { key: "AllowReadTransactions", read: true },
                { key: "AllowNewTransactions" },
                { key: "AllowSortTransactions", needsRead: true },
                { key: "AllowDeleteTransactions", needsRead: true }
            ]
        },
        {
            // Its own row, so allowing sorting never allowed merging two
            // transactions into one (D-693). It reads the transactions, so
            // it keeps Transactions › Read on.
            name: i18nContext.i18nc("@label a part of the book", "Moves between your accounts"),
            readRow: "Transactions",
            cells: [
                { key: "", absent: "viaTransactions" },
                { key: "", absent: "never" },
                { key: "AllowJoinTransfers", needsRead: true },
                { key: "", absent: "never" }
            ]
        },
        {
            name: i18nContext.i18nc("@label a part of the book", "Charts"),
            cells: [
                { key: "AllowReadViews", read: true },
                { key: "AllowNewViews" },
                { key: "AllowEditViews", needsRead: true },
                { key: "AllowDeleteViews", needsRead: true }
            ]
        },
        {
            // Its own row, so allowing charts never allowed the AI to write
            // a chart's description (D-729). It widens Charts › Add and
            // Charts › Change and does nothing without them.
            name: i18nContext.i18nc("@label a part of the book", "Chart descriptions"),
            cells: [
                { key: "", absent: "never" },
                { key: "AllowCustomCharts" },
                { key: "", absent: "never" },
                { key: "", absent: "never" }
            ]
        },
        {
            name: i18nContext.i18nc("@label a part of the book", "Budgets"),
            cells: [
                { key: "AllowReadBudgets", read: true },
                { key: "AllowNewBudgets", needsRead: true },
                { key: "AllowEditBudgets", needsRead: true },
                { key: "", absent: "never" }
            ]
        }
    ]

    readonly property var columns: [
        i18nContext.i18nc("@title:column what the AI may do", "Read"),
        i18nContext.i18nc("@title:column what the AI may do", "Add"),
        i18nContext.i18nc("@title:column what the AI may do", "Change"),
        i18nContext.i18nc("@title:column what the AI may do", "Delete")
    ]

    // The fields of a transaction that can be locked against the AI.
    readonly property var fields: [
        { key: "LockAmount", label: i18nContext.i18nc("@label a field of a transaction", "Amount") },
        { key: "LockDate", label: i18nContext.i18nc("@label a field of a transaction", "Date") },
        { key: "LockPayee", label: i18nContext.i18nc("@label a field of a transaction", "Who paid or was paid") },
        { key: "LockAccount", label: i18nContext.i18nc("@label a field of a transaction", "Account") },
        { key: "LockCategory", label: i18nContext.i18nc("@label a field of a transaction", "Category") },
        { key: "LockNote", label: i18nContext.i18nc("@label a field of a transaction", "Note") }
    ]

    // Whether the AI may write to the transaction history: add, change or
    // delete. That is the one part that can hurt for good, so it is marked
    // with a warning while any of it is on.
    function historyAtRisk(again: int): bool {
        return ["AllowNewTransactions", "AllowSortTransactions", "AllowDeleteTransactions", "AllowJoinTransfers"].some(key => settings.allows(key));
    }

    // Whether the cell is on, as the settings say it.
    // `again` is the revision the caller read, so that its binding is
    // evaluated again after each choice.
    function isOn(key: string, again: int): bool {
        return settings.allows(key);
    }

    // Whether Read of this row is on because something in the row needs it.
    // A row may also need the reading of another one (`readRow`). Only a row
    // that names one does: most rows have neither a `key` nor a `readRow`,
    // and two missing names are not a match (US4 P3, R5).
    function readForced(row: var, again: int): bool {
        return rows.some(other => (other === row || (row.key !== undefined && other.readRow === row.key)) && other.cells.some(cell => cell.needsRead && settings.allows(cell.key)));
    }

    function choose(key: string, value: bool) {
        const ok = settings.setAllows(key, value);
        revision++;
        saved(ok);
    }

    function lock(key: string, value: bool) {
        const ok = settings.setLocked(key, value);
        revision++;
        saved(ok);
    }

    spacing: Kirigami.Units.smallSpacing

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: grid.i18nContext.i18nc("@info", "Choose what the AI may do with your book. Everything is off until you switch it on. Whatever the AI prepares is shown to you first, and nothing changes until you have read it and confirmed it.")
    }

    // The grid: a row of column titles, then for each row its name and its
    // four cells, all in one layout so that the columns line up.
    readonly property var cells: {
        const flat = [];
        rows.forEach((row, rowIndex) => {
            flat.push({ name: row.name, rowIndex: rowIndex });
            row.cells.forEach((cell, column) => flat.push({ rowIndex: rowIndex, column: column, key: cell.key, read: cell.read === true, absent: cell.absent ?? "" }));
        });
        return flat;
    }

    // The widest single word of the texts in cells that aren't offered,
    // in their small font: an action column is never narrower, so these
    // words wrap between words and never inside one (#759).
    FontMetrics {
        id: smallMetrics

        font.pointSize: Kirigami.Theme.smallFont.pointSize
    }
    readonly property real widestWord: {
        const texts = [
            i18nContext.i18nc("@info the AI is never offered this", "not offered"),
            i18nContext.i18nc("@info reading this row is the reading of the transactions", "as Transactions"),
            i18nContext.i18nc("@info the AI can't do this yet", "not yet")
        ];
        let widest = 0;
        texts.forEach(text => text.split(/\s+/).forEach(word => widest = Math.max(widest, smallMetrics.advanceWidth(word))));
        return Math.ceil(widest) + 2;
    }

    GridLayout {
        Layout.fillWidth: true
        columns: 5
        columnSpacing: Kirigami.Units.largeSpacing
        rowSpacing: Kirigami.Units.smallSpacing

        Item {
            Layout.fillWidth: true
        }
        Repeater {
            model: grid.columns

            delegate: Controls.Label {
                required property string modelData

                Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                horizontalAlignment: Text.AlignHCenter
                font.bold: true
                textFormat: Text.PlainText
                text: modelData
            }
        }

        Repeater {
            model: grid.cells

            delegate: Item {
                id: cell

                required property var modelData

                readonly property bool isName: cell.modelData.name !== undefined
                readonly property var row: grid.rows[cell.modelData.rowIndex]

                Layout.fillWidth: cell.isName
                Layout.preferredWidth: cell.isName ? Kirigami.Units.gridUnit * 8 : Kirigami.Units.gridUnit * 5
                Layout.minimumWidth: cell.isName ? 0 : Math.max(toggle.implicitWidth, grid.widestWord)
                implicitHeight: Math.max(nameLabel.implicitHeight, toggle.implicitHeight, absentLabel.visible ? absentLabel.implicitHeight : 0)

                Controls.Label {
                    id: nameLabel

                    anchors.verticalCenter: parent.verticalCenter
                    visible: cell.isName
                    font.bold: true
                    textFormat: Text.PlainText
                    text: cell.isName ? cell.modelData.name : ""
                }

                Controls.Switch {
                    id: toggle

                    anchors.centerIn: parent
                    visible: !cell.isName && cell.modelData.key !== ""
                    // Read stays on while a change or a deletion of its row
                    // needs it.
                    readonly property bool forced: cell.modelData.read === true && grid.readForced(cell.row, grid.revision)
                    enabled: !forced
                    checked: forced || grid.isOn(cell.modelData.key ?? "", grid.revision)
                    Accessible.name: cell.isName ? "" : grid.i18nContext.i18nc("@label:switch row and action, e.g. Rules, Delete", "%1: %2", cell.row.name, grid.columns[cell.modelData.column])
                    onToggled: {
                        ActionLog.taken("AiPermissionGrid_toggled")
                        grid.choose(cell.modelData.key, checked);
                        checked = Qt.binding(() => forced || grid.isOn(cell.modelData.key ?? "", grid.revision));
                    }
                }

                // Wrapped within its own column, so a longer translation
                // never runs into the next cell (US4 P3, R8), and only
                // between words: the column is as wide as the widest word.
                Controls.Label {
                    id: absentLabel

                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !cell.isName && cell.modelData.key === ""
                    enabled: false
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    textFormat: Text.PlainText
                    text: cell.modelData.absent === "never"
                        ? grid.i18nContext.i18nc("@info the AI is never offered this", "not offered")
                        : cell.modelData.absent === "viaTransactions"
                        ? grid.i18nContext.i18nc("@info reading this row is the reading of the transactions", "as Transactions")
                        : grid.i18nContext.i18nc("@info the AI can't do this yet", "not yet")
                }
            }
        }
    }

    // The warning (D-259). It can't know whether the user has a backup: a
    // copy can be saved anywhere, so it is shown whenever the history can be
    // written to, and says how to make one.
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: grid.historyAtRisk(grid.revision)
        type: Kirigami.MessageType.Error
        text: grid.i18nContext.i18nc("@info", "Danger: the AI can now change your transaction history. A model can misunderstand or make things up, and a wrong entry that you confirm without noticing may stay unnoticed for a long time. You can undo your last steps, but undo does not replace a backup. Save a copy of your book first (Save a copy of my data → The whole book).")
    }

    // The plain warning of D-709: the AI writes what a custom chart draws.
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: grid.isOn("AllowCustomCharts", grid.revision)
        type: Kirigami.MessageType.Warning
        text: grid.isOn("AllowNewViews", grid.revision) || grid.isOn("AllowEditViews", grid.revision)
            ? grid.i18nContext.i18nc("@info", "The AI can now write chart descriptions itself, for new charts (Charts › Add) and for your own custom charts (Charts › Change). It never sees the figures: KassenSturz checks every description and draws the chart from your book, and you see each chart before it is saved.")
            : grid.i18nContext.i18nc("@info", "Chart descriptions do nothing on their own: switch on Charts › Add for new charts or Charts › Change for your own custom charts too.")
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: grid.i18nContext.i18nc("@info", "Changing or deleting also lets the AI read that row, because it can't change what it can't see. The AI can only delete a category that nothing uses. For an account, Change means renaming, closing and opening it again; deleting an account is never offered, you can do that yourself. For transactions, the AI can add and delete only payments in a cash account, and delete only what was entered by hand.")
    }

    // Locked fields, for the transactions the AI may change (D-252). Shown
    // only while that is on. A switch here means "the AI may write it", so
    // on is allowed as in the grid above (#631); the setting stored is still
    // the lock, the opposite of the switch.
    Kirigami.AbstractCard {
        Layout.fillWidth: true
        visible: grid.historyAtRisk(grid.revision)

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Heading {
                level: 3
                text: grid.i18nContext.i18nc("@title:group", "Fields of a transaction the AI may write")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: grid.i18nContext.i18nc("@info", "The AI writes only the fields switched on, as in the grid above. A field switched off is never written by the AI: not changed on a transaction you already have, and not filled in on one it adds. To add a payment, the account, day and amount must be switched on. A field switched off is still shown to the AI when reading is on.")
            }

            Repeater {
                model: grid.fields

                delegate: RowLayout {
                    id: fieldRow

                    required property var modelData

                    Layout.fillWidth: true

                    Controls.Switch {
                        checked: grid.revision >= 0 && !grid.settings.locked(fieldRow.modelData.key)
                        Accessible.name: grid.i18nContext.i18nc("@label:switch a field of a transaction", "The AI may write %1", fieldRow.modelData.label)
                        onToggled: {
                            ActionLog.taken("AiPermissionGrid_fieldWritable")
                            grid.lock(fieldRow.modelData.key, !checked);
                            checked = Qt.binding(() => grid.revision >= 0 && !grid.settings.locked(fieldRow.modelData.key));
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        text: fieldRow.modelData.label
                    }

                    Controls.Label {
                        enabled: false
                        textFormat: Text.PlainText
                        text: grid.revision >= 0 && grid.settings.locked(fieldRow.modelData.key)
                            ? grid.i18nContext.i18nc("@info a field is locked", "Locked")
                            : grid.i18nContext.i18nc("@info a field is free", "The AI may change it")
                    }
                }
            }
        }
    }
}
