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
    // The saved view being changed, by ID; empty for one not saved yet.
    property string openId: ""
    property string openName: ""
    property string period: "last-12-months"
    readonly property var periods: ["this-month", "last-month", "last-12-months", "this-year", "last-year"]
    readonly property var types: ["chart.bar", "chart.pie", "chart.donut", "chart.line", "chart.area", "chart.stacked_bar", "kpi.card", "progress.bar", "table.rows"]
    property var renamed: ({})

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
    }

    function saveNew() {
        const result = saved.save(nameField.text, page.period, workbench.view.json);
        error.result = result;
        if (result.code === undefined) {
            page.openId = result.id;
            page.openName = nameField.text.trim();
            nameField.text = "";
        }
    }

    function saveChanges() {
        error.result = saved.update(page.openId, page.period, workbench.view.json);
    }

    function startNew() {
        page.openId = "";
        page.openName = "";
        error.result = ({});
        workbench.open("chart.bar", "spending_by_category");
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
            if (page.openId !== "" && !page.views.some(view => view.id === page.openId)) {
                page.openId = "";
                page.openName = "";
            }
        }
    }

    Component.onCompleted: {
        saved.refresh();
        workbench.open("chart.bar", "spending_by_category");
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

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.ComboBox {
                id: typeBox

                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Show as")
                Accessible.name: Kirigami.FormData.label
                model: page.types.map(id => ({id: id, name: page.typeName(id)}))
                textRole: "name"
                valueRole: "id"
                currentIndex: page.types.indexOf(workbench.view.type)
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
            model: workbench
            period: page.period
            choices: page.groupsSource
        }

        ComponentPreview {
            id: preview

            Layout.fillWidth: true
            subject: workbench.view
            period: page.period
        }

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
