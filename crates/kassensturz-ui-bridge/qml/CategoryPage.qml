// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Adds a category, or renames and moves one (D-082). A category goes inside
// another one of its kind, or stays at the top level; moving it takes
// everything inside it and all its transactions along.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property CategoryTree categories
    // The category to change, by ID; empty to add a new one.
    property string categoryId: ""
    // The category's name, kind, and the one it sits in, if it is changed.
    property string categoryName: ""
    property bool income: false
    property string parentId: ""

    readonly property bool adding: categoryId === ""
    // The places the category can go: the top level, then every category of
    // its kind it may sit inside.
    readonly property var choices: {
        const found = categories.parentChoices(income, categoryId);
        return found.code === undefined ? found : [];
    }

    // The page is done: a category was added, or the changes were saved.
    signal done()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: adding
        ? i18nContext.i18nc("@title", "Add a category")
        : i18nContext.i18nc("@title %1 is the name of a category", "Change %1", categoryName)
    header: FormError {
        id: error
        forCategory: true
    }

    // The ID of the category chosen in the list, empty for the top level.
    function chosenParent(): string {
        return parentBox.currentIndex <= 0 ? "" : choices[parentBox.currentIndex - 1].id;
    }

    function save() {
        let result = ({});
        if (adding) {
            result = categories.addCategory(nameField.text, income, chosenParent(), "");
        } else {
            if (nameField.text.trim() !== categoryName) {
                result = categories.renameCategory(categoryId, nameField.text);
            }
            if (result.code === undefined && chosenParent() !== parentId) {
                result = categories.moveCategory(categoryId, chosenParent());
            }
        }
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    Kirigami.FormLayout {
        Controls.ButtonGroup {
            id: kindGroup
        }

        Controls.RadioButton {
            visible: page.adding
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "What kind of category is it?")
            text: page.i18nContext.i18nc("@option:radio", "What money is spent on")
            checked: !page.income
            Controls.ButtonGroup.group: kindGroup
            onToggled: page.income = false
        }

        Controls.RadioButton {
            visible: page.adding
            text: page.i18nContext.i18nc("@option:radio", "Where money comes from")
            checked: page.income
            Controls.ButtonGroup.group: kindGroup
            onToggled: page.income = true
        }

        Controls.TextField {
            id: nameField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Name:")
            text: page.categoryName
            placeholderText: page.income
                ? page.i18nContext.i18nc("@info:placeholder", "For example Salary")
                : page.i18nContext.i18nc("@info:placeholder", "For example Groceries")
            onAccepted: page.save()
        }

        Controls.ComboBox {
            id: parentBox
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Inside:")
            // "Nothing" is the top level, as the first choice.
            model: [page.i18nContext.i18nc("@item:inlistbox put the category at the top level", "Nothing, on its own")]
                .concat(page.choices.map(choice => choice.path))
            currentIndex: {
                const at = page.choices.findIndex(choice => choice.id === page.parentId);
                return at + 1;
            }
            Accessible.name: page.i18nContext.i18nc("@label:listbox", "The category it goes inside")
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            text: page.adding
                ? page.i18nContext.i18nc("@info", "A category can go inside another one, for example Groceries inside Food.")
                : page.i18nContext.i18nc("@info", "Moving a category takes everything inside it, and all its transactions, along.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Controls.Button {
            text: page.adding
                ? page.i18nContext.i18nc("@action:button", "Add category")
                : page.i18nContext.i18nc("@action:button", "Save")
            icon.name: page.adding ? "list-add" : "document-save"
            onClicked: page.save()
        }
    }
}
