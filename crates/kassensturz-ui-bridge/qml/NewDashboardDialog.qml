// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// *New dashboard…* on the Overview (M6d.1, D-436): the templates to start
// from (M6d.2, D-440), each a `TemplatePicture` with its name and one line
// of what it shows, and a name; makes the dashboard at the end of the list.
// An empty one then opens being arranged, one from a template as made. The
// name follows the template picked until the user types one. *Your
// accounts* is offered only while bank connections can be used (D-054).
// The same dialog renames the dashboard shown (`renaming`), without the
// templates. A name another dashboard has is refused by Rust and said here,
// and the dialog stays open.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    // A `DashboardLayout`.
    required property var layout
    // Whether the dialog renames the dashboard shown instead.
    property bool renaming: false
    // Whether bank connections can be used (D-054).
    property bool bankReady: false
    // How many budgets the book has: the *Budgets* template says that its
    // bars wait for one while there is none.
    property int budgetCount: 0
    // The word of the template picked.
    property string word: "empty"
    // Whether the user typed a name of their own.
    property bool nameTyped: false

    // A new dashboard was made and is shown; `empty` when it has no widgets
    // and is being arranged.
    signal made(bool empty)

    // The templates offered, from Rust, in their order (Empty first,
    // Overview second).
    readonly property var templates: (layout.templates() ?? []).filter(template => template.word !== "accounts" || bankReady)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The name a template is shown with, which a dashboard made from it
    // starts with.
    function templateName(word: string): string {
        switch (word) {
        case "overview":
            return i18nContext.i18nc("@item a template for a new dashboard", "Overview");
        case "spending":
            return i18nContext.i18nc("@item a template for a new dashboard", "Where the money goes");
        case "money-in-out":
            return i18nContext.i18nc("@item a template for a new dashboard", "Money in and out");
        case "worth":
            return i18nContext.i18nc("@item a template for a new dashboard", "What you own");
        case "budgets":
            return i18nContext.i18nc("@item a template for a new dashboard", "Budgets");
        case "accounts":
            return i18nContext.i18nc("@item a template for a new dashboard", "Your accounts");
        }
        return i18nContext.i18nc("@item a template for a new dashboard: one with nothing on it", "Empty");
    }

    // One line of what a template shows.
    function templateLine(word: string): string {
        switch (word) {
        case "overview":
            return i18nContext.i18nc("@info what a template shows", "Everything at a glance, like the standard Overview.");
        case "spending":
            return i18nContext.i18nc("@info what a template shows", "What you spent, by category, this month and over the year.");
        case "money-in-out":
            return i18nContext.i18nc("@info what a template shows", "What came in and went out, month by month.");
        case "worth":
            return i18nContext.i18nc("@info what a template shows", "What you have, what you owe and how it changed.");
        case "budgets":
            return budgetCount > 0
                ? i18nContext.i18nc("@info what a template shows", "How much of each budget is spent and left.")
                : i18nContext.i18nc("@info what a template shows, while the book has no budget", "How much of each budget is spent and left. The bars appear once you make a budget.");
        case "accounts":
            return i18nContext.i18nc("@info what a template shows", "What each account holds, and one button to update all of them from the bank.");
        }
        return i18nContext.i18nc("@info what a template shows", "Nothing yet: you add the widgets you want.");
    }

    // `name`, or with a number if another dashboard has it.
    function freeName(word: string): string {
        const taken = (layout.view.dashboards ?? []).map(dashboard => dashboard.name.toLowerCase());
        if (word === "empty") {
            let name = i18nContext.i18nc("@item the name a new dashboard starts with", "New dashboard");
            for (let number = 2; taken.includes(name.toLowerCase()); number++) {
                name = i18nContext.i18nc("@item the name a new dashboard starts with, %1 a number", "New dashboard %1", number);
            }
            return name;
        }
        const base = templateName(word);
        let name = base;
        for (let number = 2; taken.includes(name.toLowerCase()); number++) {
            name = i18nContext.i18nc("@item the name of a dashboard made from a template, %1 the template's name, %2 a number", "%1 %2", base, number);
        }
        return name;
    }

    // Picks a template; the name follows unless the user typed one.
    function pick(chosen: string) {
        word = chosen;
        if (!nameTyped) {
            nameField.text = freeName(chosen);
        }
    }

    // Opens the dialog to make a new dashboard, with a name no dashboard
    // has yet.
    function openNew() {
        renaming = false;
        word = "empty";
        start(freeName(word));
    }

    // Opens the dialog to rename the dashboard shown.
    function openRename() {
        renaming = true;
        start(layout.view.name ?? "");
    }

    function start(name: string) {
        nameField.text = name;
        nameTyped = false;
        error.result = ({});
        open();
        nameField.selectAll();
        nameField.forceActiveFocus();
    }

    function finish() {
        // A template's heading is its name in the user's language (D-440).
        const result = renaming
            ? layout.renameDashboard(nameField.text)
            : layout.newDashboard(word, nameField.text, templateName(word));
        if (result.code !== undefined) {
            error.result = result;
            return;
        }
        close();
        if (!renaming) {
            made(word === "empty");
        }
    }

    title: renaming
        ? i18nContext.i18nc("@title:window", "Rename this dashboard")
        : i18nContext.i18nc("@title:window", "New dashboard")
    standardButtons: Kirigami.Dialog.NoButton
    preferredWidth: Kirigami.Units.gridUnit * (renaming ? 22 : 34)
    padding: Kirigami.Units.largeSpacing

    customFooterActions: [
        Kirigami.Action {
            icon.name: dialog.renaming ? "edit-rename" : "list-add"
            enabled: nameField.text.trim() !== ""
            text: dialog.renaming
                ? dialog.i18nContext.i18nc("@action:button", "Rename")
                : dialog.i18nContext.i18nc("@action:button make the new dashboard", "Make it")
            onTriggered: {
                ActionLog.taken("NewDashboardDialog_keep")
                dialog.finish()
            }
        },
        Kirigami.Action {
            icon.name: "dialog-cancel"
            text: dialog.i18nContext.i18nc("@action:button", "Cancel")
            onTriggered: {
                ActionLog.taken("NewDashboardDialog_cancel")
                dialog.close()
            }
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.fillWidth: true
            visible: !dialog.renaming
            wrapMode: Text.Wrap
            text: dialog.i18nContext.i18nc("@info", "Pick what the new dashboard starts with. You can change everything on it later, and switch between your dashboards with the tabs at the top of the Overview.")
        }

        GridLayout {
            id: choices

            Layout.fillWidth: true
            visible: !dialog.renaming
            columns: 3
            columnSpacing: Kirigami.Units.smallSpacing
            rowSpacing: Kirigami.Units.smallSpacing

            Repeater {
                model: dialog.renaming ? [] : dialog.templates

                Controls.ItemDelegate {
                    id: choice

                    required property var modelData

                    Layout.fillWidth: true
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                    Layout.alignment: Qt.AlignTop
                    checkable: true
                    checked: dialog.word === choice.modelData.word
                    highlighted: checked
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: dialog.templateName(choice.modelData.word)
                    Accessible.description: dialog.templateLine(choice.modelData.word)
                    onClicked: {
                        ActionLog.taken("NewDashboardDialog_template")
                        dialog.pick(choice.modelData.word)
                    }

                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing

                        TemplatePicture {
                            Layout.fillWidth: true
                            frames: choice.modelData.frames
                            rows: choice.modelData.rows
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            text: dialog.templateName(choice.modelData.word)
                            textFormat: Text.PlainText
                            font.bold: true
                            elide: Text.ElideRight
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            text: dialog.templateLine(choice.modelData.word)
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            font: Kirigami.Theme.smallFont
                            opacity: 0.8
                        }
                    }
                }
            }
        }

        Controls.TextField {
            id: nameField

            Layout.fillWidth: true
            maximumLength: 200
            Accessible.name: dialog.i18nContext.i18nc("@label:textbox", "Name of the dashboard")
            onTextEdited: dialog.nameTyped = true
            onAccepted: {
                ActionLog.taken("NewDashboardDialog_nameAccepted")
                dialog.finish()
            }
        }

        FormError {
            id: error

            Layout.fillWidth: true
        }
    }
}
