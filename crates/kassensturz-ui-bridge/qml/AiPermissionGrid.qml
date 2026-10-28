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
            cells: [
                { key: "AllowReadTransactions", read: true },
                { key: "", absent: "later" },
                { key: "AllowSortTransactions", needsRead: true },
                { key: "", absent: "later" }
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

    // Whether the cell is on, as the settings say it.
    // `again` is the revision the caller read, so that its binding is
    // evaluated again after each choice.
    function isOn(key: string, again: int): bool {
        return settings.allows(key);
    }

    // Whether Read of this row is on because something in the row needs it.
    function readForced(row: var, again: int): bool {
        return row.cells.some(cell => cell.needsRead && settings.allows(cell.key));
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
                implicitHeight: Math.max(nameLabel.implicitHeight, toggle.implicitHeight)

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
                        grid.choose(cell.modelData.key, checked);
                        checked = Qt.binding(() => forced || grid.isOn(cell.modelData.key ?? "", grid.revision));
                    }
                }

                Controls.Label {
                    anchors.centerIn: parent
                    visible: !cell.isName && cell.modelData.key === ""
                    enabled: false
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    textFormat: Text.PlainText
                    text: cell.modelData.absent === "never"
                        ? grid.i18nContext.i18nc("@info the AI is never offered this", "not offered")
                        : grid.i18nContext.i18nc("@info the AI can't do this yet", "not yet")
                }
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: grid.i18nContext.i18nc("@info", "Changing or deleting also lets the AI read that row, because it can't change what it can't see. The AI can only delete a category that nothing uses. For an account, Change means renaming, closing and opening it again; deleting an account is never offered, you can do that yourself.")
    }

    // Locked fields, for the transactions the AI may change (D-252). Shown
    // only while that is on.
    Kirigami.AbstractCard {
        Layout.fillWidth: true
        visible: grid.isOn("AllowSortTransactions", grid.revision)

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Heading {
                level: 3
                text: grid.i18nContext.i18nc("@title:group", "Fields of a transaction the AI may not change")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: grid.i18nContext.i18nc("@info", "A locked field is never changed by the AI on a transaction you already have. It is still shown to the AI when reading is on.")
            }

            Repeater {
                model: grid.fields

                delegate: RowLayout {
                    id: fieldRow

                    required property var modelData

                    Layout.fillWidth: true

                    Controls.Switch {
                        checked: grid.revision >= 0 && grid.settings.locked(fieldRow.modelData.key)
                        Accessible.name: grid.i18nContext.i18nc("@label:switch a field of a transaction the AI may not change", "%1 is locked against the AI", fieldRow.modelData.label)
                        onToggled: {
                            grid.lock(fieldRow.modelData.key, checked);
                            checked = Qt.binding(() => grid.revision >= 0 && grid.settings.locked(fieldRow.modelData.key));
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
