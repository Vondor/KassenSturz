// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Statistics (M6a.7, D-278): look at the book's figures as a chart, a table,
// a key figure or a progress bar, change how it is shown, and keep a view
// you like. Saving, renaming and deleting a view go into the change history,
// so the menu's Undo takes them back (D-279). Every figure is text from Rust
// (D-065); the choices come from the property schema, not from this page.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
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
    // A chart on the Overview being changed step by step (M6-B164), by the
    // ID of its component there; empty when the builder makes or changes a
    // saved view. Finish gives the chart back to the Overview, which keeps
    // it as one change that Undo takes back.
    property string overviewComponentId: ""
    // The period the Overview's chart had, as the builder showed it, so that
    // Finish writes the period only when the user changed it.
    property string startPeriod: ""
    // A statistic the user built and left before Finish (M6-B164): the
    // component as JSON text, its period, its name and the wizard's step.
    // Main keeps it while the app runs, so it is back when the builder is
    // opened again; empty for none.
    property var draft: ({})
    property bool shownDraft: false
    // The fresh chart a new statistic starts from, to tell whether the user
    // has made anything yet.
    property string startJson: ""
    property bool ready: false
    property string openName: ""
    property string period: "last-12-months"
    readonly property var periods: ["this-month", "last-month", "last-12-months", "this-year", "last-year"]
    readonly property var types: ["chart.bar", "chart.pie", "chart.donut", "chart.line", "chart.area", "chart.stacked_bar", "kpi.card", "progress.bar", "table.rows", "chart.candlestick", "budget.progress"]
    property var renamed: ({})
    // A new view is made in the wizard (D-335); "Show all settings at once"
    // and opening a saved view give the page with every setting.
    property bool wizard: true
    readonly property alias workbench: workbench

    // The user chose a KMyMoney file to add its reports from (M6d.6b).
    signal kmyReportsFileChosen(url file)
    // Finish or Cancel of a chart from the Overview (M6-B164).
    signal overviewChangeFinished(string id, string json)
    signal overviewChangeCancelled()
    // "Put it on the Overview" after a statistic is kept (M6-B164).
    signal addToOverviewRequested(string component, string period)
    // The unfinished statistic changed; `draft` is empty when there is none.
    signal draftLeft(var draft)

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
        case "chart.candlestick":
            return i18nContext.i18nc("@item:inlistbox a way to show figures: the balance per day, week or month with its start, highest, lowest and end", "Balance candles");
        case "budget.progress":
            return i18nContext.i18nc("@item:inlistbox a way to show figures: one bar per budget, how much is spent and left", "Budget bars");
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

    // Whether another saved view has this name; names are compared as the
    // book does, without regard to case.
    function nameTaken(name: string): bool {
        const wanted = name.trim().toLowerCase();
        return page.views.some(view => view.name.trim().toLowerCase() === wanted);
    }

    // The name a statistic gets when the user gave none (M6-B164): its title
    // if it has one no other view has, else "Statistic 1", "Statistic 2"…
    function defaultName(): string {
        const title = ((workbench.view?.fields ?? []).find(field => field.key === "title")?.text ?? "").trim();
        if (title !== "" && !nameTaken(title)) {
            return title;
        }
        for (let number = 1; ; ++number) {
            const name = i18nContext.i18nc("@item the name a new saved statistic gets when the user gave none, %1 is a number", "Statistic %1", number);
            if (!nameTaken(name)) {
                return name;
            }
        }
    }

    // Tells Main what is unfinished now (M6-B164): a statistic that is not
    // saved and differs from the fresh start. A saved view open on the page
    // leaves the draft as it was.
    function noteDraft() {
        if (!page.ready || page.overviewComponentId !== "" || page.openId !== "") {
            return;
        }
        const json = workbench.view?.json ?? "";
        const unfinished = json !== "" && json !== page.startJson;
        page.draftLeft(unfinished ? {
            component: json,
            period: page.period,
            name: wizardView.nameText,
            step: wizardView.step
        } : ({}));
    }

    function saveAs(name: string) {
        const result = saved.save(name, page.period, workbench.view.json);
        error.result = result;
        if (result.code === undefined) {
            page.openId = result.id;
            page.openName = name.trim();
            nameField.text = "";
            page.wizard = false;
            page.shownDraft = false;
            page.draftLeft({});
        }
    }

    function saveChanges() {
        error.result = saved.update(page.openId, page.period, workbench.view.json);
        return error.result.code === undefined;
    }

    // Finish in the wizard: keeps a new view under its name, or the changes
    // of the open one.
    // A chart from the Overview goes back there; a new view without a name
    // gets a default one (M6-B164).
    function finishWizard(name: string) {
        if (page.overviewComponentId !== "") {
            if (page.period !== page.startPeriod) {
                workbench.setWord("period", page.period);
            }
            page.overviewChangeFinished(page.overviewComponentId, workbench.view.json);
        } else if (page.openId === "") {
            page.saveAs(name.trim() !== "" ? name : page.defaultName());
        } else if (page.saveChanges()) {
            page.wizard = false;
        }
    }

    // Cancel in the wizard: the open view is read again as it was saved; a
    // new one starts over from a clean page.
    function cancelWizard() {
        if (page.overviewComponentId !== "") {
            page.overviewChangeCancelled();
        } else if (page.openId !== "") {
            page.openView(page.openId);
        } else {
            error.result = ({});
            workbench.open("chart.bar", "spending_by_category");
            page.wizard = false;
            page.shownDraft = false;
        }
    }

    // Opens a saved view in the wizard at its first step (M6-B164).
    function changeView(id: string) {
        page.openView(id);
        if (page.openId === id) {
            page.changeStep(0);
        }
    }

    // Deletes a saved view. Removing it rebuilds the list, which destroys the
    // row whose button called this, so the work after `remove()` runs here
    // on the page and not in the row (M6-B199).
    function removeView(id: string) {
        error.result = page.saved.remove(id);
        if (page.openId === id) {
            page.openId = "";
            page.openName = "";
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

    // Every change of the statistic, its period or its name keeps the draft
    // up to date, so leaving the page any way loses nothing (M6-B164).
    Connections {
        target: workbench

        function onViewChanged() {
            page.noteDraft();
        }
    }

    onPeriodChanged: noteDraft()

    // Starts on the chart the AI prepared, or on the dataset it understood,
    // in the page with every setting so the name and Save are in view. A
    // component the schema refuses leaves the usual start.
    function startFrom() {
        // A chart from the Overview opens in the wizard at its first step.
        if (overviewComponentId !== "") {
            workbench.load(initialComponent);
            if (initialPeriod !== "" && initialPeriod !== "page") {
                page.period = initialPeriod;
            }
            page.startPeriod = page.period;
            page.wizard = true;
            wizardView.openAt(0);
            return;
        }
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
        // The statistic left unfinished last time comes back (M6-B164).
        if (initialComponent === "" && initialDataset === "" && draft.component !== undefined) {
            page.startJson = workbench.view?.json ?? "";
            workbench.load(draft.component);
            if (workbench.view.error === undefined) {
                page.period = draft.period ? draft.period : page.period;
                wizardView.restoreName(draft.name ?? "");
                wizardView.openAt(draft.step ?? 0);
                page.shownDraft = true;
                return;
            }
            workbench.open("chart.bar", "spending_by_category");
        }
    }

    Component.onCompleted: {
        saved.refresh();
        startFrom();
        if (page.startJson === "") {
            page.startJson = workbench.view?.json ?? "";
        }
        page.ready = initialComponent === "" && initialDataset === "";
    }

    Kirigami.PromptDialog {
        id: renameDialog

        title: page.i18nContext.i18nc("@title:window", "Rename this view")
        standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
        onAccepted: {
            ActionLog.taken("StatisticsPage_accepted")
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
            onAccepted: {
                ActionLog.taken("StatisticsPage_nameView")
                renameDialog.accept()
            }
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.shownDraft && page.openId === ""
            type: Kirigami.MessageType.Information
            text: page.i18nContext.i18nc("@info", "This is the statistic you left before Finish. Finish keeps it; Cancel starts a new one.")
        }

        StatisticWizard {
            id: wizardView

            Layout.fillWidth: true
            visible: page.wizard
            builder: page
            onShowAll: page.wizard = false
            onCancelled: page.cancelWizard()
            onFinishRequested: name => page.finishWizard(name)
            onNameTextChanged: page.noteDraft()
            onStepChanged: page.noteDraft()
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
                onClicked: {
                    ActionLog.taken("StatisticsPage_period")
                    page.changeStep(0)
                }
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button opens the wizard at the step for the categories", "Categories…")
                onClicked: {
                    ActionLog.taken("StatisticsPage_categories")
                    page.changeStep(1)
                }
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button opens the wizard at the step for the look and the title", "Look and title…")
                onClicked: {
                    ActionLog.taken("StatisticsPage_lookTitle")
                    page.changeStep(2)
                }
            }
        }

        Controls.Button {
            visible: !page.wizard && page.openId === ""
            text: page.i18nContext.i18nc("@action:button make a new view step by step", "Guide me step by step")
            icon.name: "tools-wizard"
            onClicked: {
                ActionLog.taken("StatisticsPage_guideMeStep")
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
                onActivated: index => {
                    ActionLog.taken("StatisticsPage_setType")
                    workbench.setType(page.types[index])
                }
            }

            PeriodChoice {
                id: periodBox

                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Period")
                options: page.periods.map(id => ({word: id, name: page.periodName(id)}))
                word: page.period
                onChosen: word => {
                    ActionLog.taken("StatisticsPage_period_2")
                    page.period = word
                }
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

        // The saved views stay in view in the wizard too, so a saved statistic
        // is always easy to find and to change (M6-B164). Not while a chart of
        // the Overview is being changed: that one is not a saved view.
        ColumnLayout {
            Layout.fillWidth: true
            visible: page.overviewComponentId === ""
            spacing: Kirigami.Units.largeSpacing

            Kirigami.Separator {
                Layout.fillWidth: true
            }

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 2
                text: page.i18nContext.i18nc("@title:group", "Saved views")
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Add reports from a KMyMoney file…")
                icon.name: "document-import"
                onClicked: {
                    ActionLog.taken("StatisticsPage_kmyReports")
                    kmyReportsDialog.open()
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: !page.wizard
                wrapMode: Text.Wrap
                text: page.openId !== ""
                    ? page.i18nContext.i18nc("@info %1 is the name of a saved view", "You are looking at the saved view “%1”.", page.openName)
                    : page.i18nContext.i18nc("@info", "Give what you see a name to keep it. Saving, renaming and deleting a view can be undone from the menu.")
                textFormat: Text.PlainText
            }

            RowLayout {
                Layout.fillWidth: true
                visible: !page.wizard && page.openId !== ""

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Save changes to this view")
                    icon.name: "document-save"
                    onClicked: {
                        ActionLog.taken("StatisticsPage_saveChangesView")
                        page.saveChanges()
                    }
                }

                // The statistic as it is shown now goes onto the Overview as a
                // chart of its own (M6-B164).
                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Put it on the Overview")
                    icon.name: "go-home"
                    onClicked: {
                        ActionLog.taken("StatisticsPage_addToOverview")
                        page.addToOverviewRequested(workbench.view.json, page.period)
                    }
                }

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button start from a clean page", "Start a new one")
                    icon.name: "document-new"
                    onClicked: {
                        ActionLog.taken("StatisticsPage_startNewOne")
                        page.startNew()
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                visible: !page.wizard

                Controls.TextField {
                    id: nameField

                    Layout.fillWidth: true
                    maximumLength: 200
                    placeholderText: page.i18nContext.i18nc("@info:placeholder", "A name for this view")
                    Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name of the view")
                    onAccepted: {
                        ActionLog.taken("StatisticsPage_nameView_2")
                        page.saveNew()
                    }
                }

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Save as a new view")
                    icon.name: "document-save-as"
                    enabled: nameField.text.trim() !== ""
                    onClicked: {
                        ActionLog.taken("StatisticsPage_saveNewView")
                        page.saveNew()
                    }
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
                        text: Plain.plain(row.modelData.usable
                            ? row.modelData.name
                            : page.i18nContext.i18nc("@action:button %1 is the name of a saved view", "%1 (can't be opened by this version)", row.modelData.name))
                        onClicked: {
                            ActionLog.taken("StatisticsPage_openView")
                            page.openView(row.modelData.id)
                        }
                    }

                    // Opens the view in the wizard to change it (M6-B164).
                    Controls.ToolButton {
                        enabled: row.modelData.usable
                        icon.name: "document-edit"
                        text: page.i18nContext.i18nc("@action:button change a saved view in the wizard", "Change step by step")
                        display: Controls.AbstractButton.TextBesideIcon
                        onClicked: {
                            ActionLog.taken("StatisticsPage_changeView")
                            page.changeView(row.modelData.id)
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "edit-rename"
                        text: page.i18nContext.i18nc("@action:button", "Rename")
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        onClicked: {
                            ActionLog.taken("StatisticsPage_rename")
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
                            ActionLog.taken("StatisticsPage_delete")
                            page.removeView(row.modelData.id)
                        }
                    }
                }
            }
        }
    }

    Dialogs.FileDialog {
        id: kmyReportsDialog

        title: page.i18nContext.i18nc("@title:window", "Add reports from a KMyMoney file")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [
            // First, because a KMyMoney database may have no ending at all.
            page.i18nContext.i18nc("@item:inlistbox a file type", "All files (*)"),
            page.i18nContext.i18nc("@item:inlistbox a file type", "KMyMoney files (*.kmy *.sqlite *.db)")
        ]
        onAccepted: {
            ActionLog.taken("StatisticsPage_kmyReportsFileChosen")
            page.kmyReportsFileChosen(selectedFile)
        }
    }
}
