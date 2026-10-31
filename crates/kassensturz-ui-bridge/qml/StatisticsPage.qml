// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Statistics (M6a.7, D-278): look at the book's figures as a chart, a table,
// a key figure or a progress bar, change how it is shown, and keep a view
// you like. Saving, renaming and deleting a view go into the change history,
// so the menu's Undo takes them back (D-279). Every figure is text from Rust
// (D-065); the choices come from the property schema, not from this page.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property SavedViews saved
    // A `CategoryGroups`, for picking My groups and categories in the panel.
    property var groupsSource: null
    readonly property var views: saved.view?.views ?? []
    // A chart to start from, as the AI prepared it in the chat (M6c.9,
    // D-355): its title, the component as JSON text and its period. What the
    // user saves is the user's own chart (D-297). Empty for the usual start.
    property string initialName: ""
    property string initialComponent: ""
    property string initialPeriod: ""
    // Else the dataset the AI understood and gave up on, empty for none.
    property string initialDataset: ""
    // The saved view being changed, by ID; empty for one not saved yet.
    property string openId: ""
    property string openName: ""
    property string period: "last-12-months"
    readonly property var periods: ["this-month", "last-month", "last-12-months", "this-year", "last-year"]
    readonly property var types: ["chart.bar", "chart.pie", "chart.donut", "chart.line", "chart.area", "chart.stacked_bar", "kpi.card", "progress.bar", "table.rows"]
    property var renamed: ({})
    // A new view is made in the wizard (D-335); "Show all settings at once"
    // and opening a saved view give the page with every setting.
    property bool wizard: true
    readonly property alias workbench: workbench

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function typeName(id: string): string {
        switch (id) {
        case "chart.bar":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Bars");
        case "chart.pie":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Pie");
        case "chart.donut":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Ring");
        case "chart.line":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Line");
        case "chart.area":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Filled line");
        case "chart.stacked_bar":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Stacked bars");
        case "kpi.card":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "One figure");
        case "progress.bar":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Progress bar");
        case "table.rows":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Table");
        }
        return "";
    }

    function periodName(id: string): string {
        switch (id) {
        case "this-month":
            return i18nContext.i18nc("@item:inlistbox a period", "This month");
        case "last-month":
            return i18nContext.i18nc("@item:inlistbox a period", "Last month");
        case "last-12-months":
            return i18nContext.i18nc("@item:inlistbox a period", "The last 12 months");
        case "this-year":
            return i18nContext.i18nc("@item:inlistbox a period", "This year");
        case "last-year":
            return i18nContext.i18nc("@item:inlistbox a period", "Last year");
        }
        return "";
    }

    // Opens a saved view on the workbench.
    function openView(id: string) {
        const opened = saved.open(id);
        if (opened.code !== undefined) {
            error.result = opened;
            return;
        }
        error.result = ({});
        workbench.load(opened.component);
        page.period = opened.period !== "" ? opened.period : page.period;
        page.openId = opened.id;
        page.openName = opened.name;
        page.wizard = false;
    }

    function saveNew() {
        saveAs(nameField.text);
    }

    function saveAs(name: string) {
        const result = saved.save(name, page.period, workbench.view.json);
        error.result = result;
        if (result.code === undefined) {
            page.openId = result.id;
            page.openName = name.trim();
            nameField.text = "";
            page.wizard = false;
        }
    }

    function saveChanges() {
        error.result = saved.update(page.openId, page.period, workbench.view.json);
        return error.result.code === undefined;
    }

    // Finish in the wizard: keeps a new view under its name, or the changes
    // of the open one.
    function finishWizard(name: string) {
        if (page.openId === "") {
            page.saveAs(name);
        } else if (page.saveChanges()) {
            page.wizard = false;
        }
    }

    // Cancel in the wizard: the open view is read again as it was saved; a
    // new one starts over from a clean page.
    function cancelWizard() {
        if (page.openId !== "") {
            page.openView(page.openId);
        } else {
            error.result = ({});
            workbench.open("chart.bar", "spending_by_category");
            page.wizard = false;
        }
    }

    // Opens the wizard on the view that is on the page, at a step.
    function changeStep(index: int) {
        page.wizard = true;
        wizardView.openAt(index);
    }

    function startNew() {
        page.openId = "";
        page.openName = "";
        error.result = ({});
        workbench.open("chart.bar", "spending_by_category");
        page.wizard = true;
        wizardView.restart();
    }

    title: i18nContext.i18nc("@title", "Statistic Builder")
    header: FormError {
        id: error
    }

    ComponentProperties {
        id: workbench
    }

    // Every change of the book, also Undo and Redo, reads the saved views
    // again: the figures are read again, and a view that is gone is no longer
    // the one being changed.
    Connections {
        target: page.saved

        function onViewChanged() {
            preview.refresh();
            wizardView.refresh();
            if (page.openId !== "" && !page.views.some(view => view.id === page.openId)) {
                page.openId = "";
                page.openName = "";
            }
        }
    }

    // Starts on the chart the AI prepared, or on the dataset it understood,
    // in the page with every setting so the name and Save are in view. A
    // component the schema refuses leaves the usual start.
    function startFrom() {
        if (initialComponent !== "") {
            workbench.load(initialComponent);
            if (workbench.view.error === undefined) {
                nameField.text = initialName;
                page.period = initialPeriod !== "" ? initialPeriod : page.period;
                page.wizard = false;
                return;
            }
        } else if (initialDataset !== "") {
            for (const kind of ["chart.bar", "chart.line", "kpi.card", "table.rows"]) {
                workbench.open(kind, initialDataset);
                if (workbench.view.error === undefined) {
                    return;
                }
            }
        }
        workbench.open("chart.bar", "spending_by_category");
    }

    Component.onCompleted: {
        saved.refresh();
        startFrom();
    }

    Kirigami.PromptDialog {
        id: renameDialog

        title: page.i18nContext.i18nc("@title:window", "Rename this view")
        standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
        onAccepted: {
            error.result = page.saved.rename(page.renamed.id, renameField.text);
            if (error.result.code === undefined && page.openId === page.renamed.id) {
                page.openName = renameField.text.trim();
            }
        }

        Controls.TextField {
            id: renameField

            text: page.renamed.name ?? ""
            maximumLength: 200
            Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name of the view")
            onAccepted: renameDialog.accept()
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        StatisticWizard {
            id: wizardView

            Layout.fillWidth: true
            visible: page.wizard
            builder: page
            onShowAll: page.wizard = false
            onCancelled: page.cancelWizard()
            onFinishRequested: name => page.finishWizard(name)
        }

        // The wizard is all there is while it is open, as a dialog is.
        RowLayout {
            Layout.fillWidth: true
            visible: !page.wizard && page.openId !== ""
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                text: page.i18nContext.i18nc("@label change parts of the open view in the wizard", "Change in the wizard:")
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button opens the wizard at the step for the period", "Period…")
                onClicked: page.changeStep(0)
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button opens the wizard at the step for the categories", "Categories…")
                onClicked: page.changeStep(1)
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button opens the wizard at the step for the look and the title", "Look and title…")
                onClicked: page.changeStep(2)
            }
        }

        Controls.Button {
            visible: !page.wizard && page.openId === ""
            text: page.i18nContext.i18nc("@action:button make a new view step by step", "Guide me step by step")
            icon.name: "tools-wizard"
            onClicked: {
                page.wizard = true;
                wizardView.restart();
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true
            visible: !page.wizard

            Controls.ComboBox {
                id: typeBox

                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Show as")
                Accessible.name: Kirigami.FormData.label
                model: page.types.map(id => ({id: id, name: page.typeName(id)}))
                textRole: "name"
                valueRole: "id"
                currentIndex: page.types.indexOf(workbench.view?.type)
                onActivated: index => workbench.setType(page.types[index])
            }

            PeriodChoice {
                id: periodBox

                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Period")
                options: page.periods.map(id => ({word: id, name: page.periodName(id)}))
                word: page.period
                onChosen: word => page.period = word
            }
        }

        PropertiesPanel {
            Layout.fillWidth: true
            visible: !page.wizard
            model: workbench
            period: page.period
            choices: page.groupsSource
        }

        ComponentPreview {
            id: preview

            Layout.fillWidth: true
            visible: !page.wizard
            subject: workbench.view
            period: page.period
        }

        ColumnLayout {
            Layout.fillWidth: true
            visible: !page.wizard
            spacing: Kirigami.Units.largeSpacing

            Kirigami.Separator {
                Layout.fillWidth: true
            }

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 2
                text: page.i18nContext.i18nc("@title:group", "Saved views")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: page.openId !== ""
                    ? page.i18nContext.i18nc("@info %1 is the name of a saved view", "You are looking at the saved view “%1”.", page.openName)
                    : page.i18nContext.i18nc("@info", "Give what you see a name to keep it. Saving, renaming and deleting a view can be undone from the menu.")
                textFormat: Text.PlainText
            }

            RowLayout {
                Layout.fillWidth: true
                visible: page.openId !== ""

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Save changes to this view")
                    icon.name: "document-save"
                    onClicked: page.saveChanges()
                }

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button start from a clean page", "Start a new one")
                    icon.name: "document-new"
                    onClicked: page.startNew()
                }
            }

            RowLayout {
                Layout.fillWidth: true

                Controls.TextField {
                    id: nameField

                    Layout.fillWidth: true
                    maximumLength: 200
                    placeholderText: page.i18nContext.i18nc("@info:placeholder", "A name for this view")
                    Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name of the view")
                    onAccepted: page.saveNew()
                }

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Save as a new view")
                    icon.name: "document-save-as"
                    enabled: nameField.text.trim() !== ""
                    onClicked: page.saveNew()
                }
            }

            Repeater {
                model: page.views

                delegate: RowLayout {
                    id: row

                    required property var modelData

                    Layout.fillWidth: true

                    Controls.Button {
                        Layout.fillWidth: true
                        flat: true
                        enabled: row.modelData.usable
                        text: row.modelData.usable
                            ? row.modelData.name
                            : page.i18nContext.i18nc("@action:button %1 is the name of a saved view", "%1 (can't be opened by this version)", row.modelData.name)
                        onClicked: page.openView(row.modelData.id)
                    }

                    Controls.ToolButton {
                        icon.name: "edit-rename"
                        text: page.i18nContext.i18nc("@action:button", "Rename")
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        onClicked: {
                            page.renamed = row.modelData;
                            renameField.text = row.modelData.name;
                            renameDialog.open();
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "edit-delete"
                        text: page.i18nContext.i18nc("@action:button", "Delete")
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        onClicked: {
                            error.result = page.saved.remove(row.modelData.id);
                            if (page.openId === row.modelData.id) {
                                page.openId = "";
                                page.openName = "";
                            }
                        }
                    }
                }
            }
        }
    }
}
