// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// "New category …" at the end of a category picker (D-910, #1244): a small
// name field, so a person who needs a category that doesn't exist yet makes
// it where they are choosing one, without leaving the form. The category
// goes at the top level (it can be moved on the Categories page); its name
// is checked like any other category name, and adding it is one change the
// person can take back with Undo. The picker that opened this selects the
// new category when `created` says so.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    // The kind the picker needs: money coming in (true) or money spent
    // (false).
    property bool income: false
    // The picker offers one kind only, so the dialog doesn't ask for it.
    property bool kindFixed: true

    // A category was added: its ID, for the picker to select.
    signal created(string categoryId)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Opens the dialog with an empty name.
    function start() {
        nameField.text = "";
        error.result = ({});
        open();
        nameField.forceActiveFocus();
    }

    // Adds the category and tells the picker; shows why if it isn't
    // accepted.
    function save() {
        const name = nameField.text.trim();
        const result = tree.addCategory(name, dialog.income, "", "");
        error.result = result;
        if (result.code !== undefined) {
            return;
        }
        // The new category is the top-level one of this name and kind.
        const rows = Array.from(tree.view.rows ?? []);
        const found = rows.find(row => row.parent === "" && row.name === name && row.income === dialog.income);
        close();
        if (found !== undefined) {
            dialog.created(found.id);
        }
    }

    // Its own handle on the book's categories: the change is announced to
    // the window, so every screen reads the book again and offers Undo.
    CategoryTree {
        id: tree

        onChanged: applicationWindow().screenChangedBook()
    }

    title: Plain.plain(!kindFixed
        ? i18nContext.i18nc("@title:window", "New category")
        : income
        ? i18nContext.i18nc("@title:window", "New category for money coming in")
        : i18nContext.i18nc("@title:window", "New category for money spent"))
    padding: Kirigami.Units.largeSpacing
    preferredWidth: Kirigami.Units.gridUnit * 24
    standardButtons: Kirigami.Dialog.NoButton
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Add category")
            icon.name: "list-add"
            enabled: nameField.text.trim() !== ""
            onTriggered: {
                ActionLog.taken("NewCategoryDialog_add")
                dialog.save()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Cancel")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("NewCategoryDialog_cancel")
                dialog.close()
            }
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        FormError {
            id: error

            Layout.fillWidth: true
            forCategory: true
        }

        // Only where the picker offers both kinds.
        Controls.ButtonGroup {
            id: kindGroup
        }

        Controls.RadioButton {
            visible: !dialog.kindFixed
            text: dialog.i18nContext.i18nc("@option:radio", "What money is spent on")
            checked: !dialog.income
            Controls.ButtonGroup.group: kindGroup
            onToggled: {
                ActionLog.taken("NewCategoryDialog_whatMoneySpent")
                dialog.income = false
            }
        }

        Controls.RadioButton {
            visible: !dialog.kindFixed
            text: dialog.i18nContext.i18nc("@option:radio", "Where money comes from")
            checked: dialog.income
            Controls.ButtonGroup.group: kindGroup
            onToggled: {
                ActionLog.taken("NewCategoryDialog_whereMoneyComes")
                dialog.income = true
            }
        }

        Controls.TextField {
            id: nameField

            Layout.fillWidth: true
            placeholderText: dialog.income
                ? dialog.i18nContext.i18nc("@info:placeholder", "For example Salary")
                : dialog.i18nContext.i18nc("@info:placeholder", "For example Groceries")
            Accessible.name: dialog.i18nContext.i18nc("@label:textbox", "Name of the new category")
            onAccepted: {
                ActionLog.taken("NewCategoryDialog_accepted")
                if (text.trim() !== "") {
                    dialog.save()
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
            text: dialog.i18nContext.i18nc("@info", "You can move it inside another category later, on the Categories page.")
        }
    }
}
