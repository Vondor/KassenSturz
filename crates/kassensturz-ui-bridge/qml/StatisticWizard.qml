// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Statistic Builder's wizard (M6b.R7, D-335, D-338), built like the chart
// wizard of an office suite: the steps in a list on the left (chart type,
// data range, data series, chart elements), the live preview always above the
// options of the step, and Back, Next, Finish and Cancel at the bottom. Any
// step can be clicked at any time; a step with nothing to set for the chosen
// type is greyed out and skipped. It decides nothing itself: every choice
// goes to the same workbench the settings screen uses. It serves a new view
// and, opened at a step, a saved one. "Show all settings at once" gives the
// page with every setting back.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: wizard

    // The Statistic Builder page, for the workbench, the period, the names of
    // types and periods and the view being changed.
    required property var builder

    property int step: 0
    readonly property int steps: 3
    readonly property var workbench: builder.workbench
    // A chart from the Overview (M6-B164) is changed, not saved as a view.
    readonly property bool onOverview: builder.overviewComponentId !== ""
    readonly property bool isNew: builder.openId === "" && !onOverview
    // The properties that belong to each step; the others are not offered
    // there.
    readonly property var rangeKeys: ["grouping", "measure", "candle", "rows"]
    readonly property var advancedKeys: ["compare", "formula", "target"]
    // Candles show the balance of accounts, so their second step chooses
    // accounts; budget bars choose budgets; a custom chart is described in
    // its own text; the transactions list chooses accounts and categories
    // (US5.5, #753).
    readonly property var seriesKeys: {
        switch (workbench.view.type) {
        case "chart.candlestick":
            return ["accounts"];
        case "chart.flow":
            return ["accounts", "categories"];
        case "chart.custom":
            return ["spec", "series", "categories"];
        case "table.transactions":
            return ["accounts", "categories"];
        }
        return ["budgets", "categories", "groups", "series", "show_hidden"];
    }
    readonly property var elementKeys: ["title", "legend", "sub_categories", "payees", "currency"]
    // Formulas and fine tuning are one deliberate step away (Q-260).
    property bool advanced: false
    // Charts and tables are made of columns on a board (D-340).
    readonly property bool hasBoard: workbench.view.columns !== undefined
    // The kinds of the first step, each with the ways to show it.
    readonly property var kinds: [
        {id: "bars", types: ["chart.bar", "chart.stacked_bar"]},
        {id: "pie", types: ["chart.pie", "chart.donut"]},
        {id: "lines", types: ["chart.line", "chart.area"]},
        {id: "figure", types: ["kpi.card", "progress.bar"]},
        {id: "table", types: ["table.rows"]},
        {id: "candles", types: ["chart.candlestick"]},
        {id: "flow", types: ["chart.flow"]},
        {id: "budgets", types: ["budget.progress"]},
        {id: "custom", types: ["chart.custom"]},
        {id: "list", types: ["table.transactions"]}
    ]
    readonly property var currentKind: kinds.find(kind => kind.types.includes(workbench.view.type)) ?? kinds[0]
    readonly property alias nameText: wizardName.text

    // The columns the user made, kept while a kind without columns (one
    // figure) is looked at, so that going back to a chart or table does not
    // lose them.
    property string keptColumns: "[]"
    // Whether the view before the last change had columns.
    property bool hadColumns: false

    signal showAll()
    signal cancelled()
    signal finishRequested(string name)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function hasFields(keys): bool {
        return (workbench.view.fields ?? []).some(field => keys.includes(field.key));
    }

    // Whether a step has anything to set for the chosen type.
    function usable(index: int): bool {
        return index !== 1 || hasBoard || hasFields(seriesKeys);
    }

    function iconOf(id: string): string {
        switch (id) {
        case "chart.bar":
        case "bars":
            return "office-chart-bar";
        case "chart.pie":
        case "pie":
            return "office-chart-pie";
        case "chart.donut":
            return "office-chart-ring";
        case "chart.line":
        case "lines":
            return "office-chart-line";
        case "chart.area":
            return "office-chart-area";
        case "chart.stacked_bar":
            return "office-chart-bar-stacked";
        case "kpi.card":
        case "figure":
            return "view-statistics";
        case "progress.bar":
            return "office-chart-bar";
        case "table.rows":
        case "table":
            return "view-list-details";
        case "chart.candlestick":
        case "candles":
            return "office-chart-scatter";
        case "chart.flow":
        case "flow":
            return "office-chart-area-stacked";
        case "budget.progress":
        case "budgets":
            return "office-chart-bar-percentage";
        case "chart.custom":
        case "custom":
            return "document-edit";
        case "table.transactions":
        case "list":
            return "view-list-text";
        }
        return "office-chart-bar";
    }

    function kindName(id: string): string {
        switch (id) {
        case "bars":
            return i18nContext.i18nc("@item:inlistbox a kind of chart", "Bars");
        case "pie":
            return i18nContext.i18nc("@item:inlistbox a kind of chart", "Pie");
        case "lines":
            return i18nContext.i18nc("@item:inlistbox a kind of chart", "Lines");
        case "figure":
            return i18nContext.i18nc("@item:inlistbox a kind of display", "One figure");
        case "candles":
            return i18nContext.i18nc("@item:inlistbox a kind of chart: the balance of accounts per day, week or month", "Balance candles");
        case "flow":
            return i18nContext.i18nc("@item:inlistbox a kind of chart: where money came from and where it went, as bands", "Money flow");
        case "budgets":
            return i18nContext.i18nc("@item:inlistbox a kind of display: how much of each budget is spent and left", "Budgets");
        case "custom":
            return i18nContext.i18nc("@item:inlistbox a kind of chart: one the user describes in a short text", "Your own description");
        case "list":
            return i18nContext.i18nc("@item:inlistbox a kind of display: single transactions, newest first", "Transactions");
        }
        return i18nContext.i18nc("@item:inlistbox a kind of display", "Table");
    }

    function stepName(index: int): string {
        switch (index) {
        case 0:
            return i18nContext.i18nc("@title:group a step of the wizard", "Period");
        case 1:
            switch (workbench.view.type) {
            case "chart.candlestick":
                return i18nContext.i18nc("@title:group a step of the wizard", "Choose accounts");
            case "chart.flow":
                return i18nContext.i18nc("@title:group a step of the wizard", "Choose accounts and categories");
            case "budget.progress":
                return i18nContext.i18nc("@title:group a step of the wizard", "Choose budgets");
            case "chart.custom":
                return i18nContext.i18nc("@title:group a step of the wizard", "Describe the chart");
            case "table.transactions":
                return i18nContext.i18nc("@title:group a step of the wizard", "Choose accounts and categories");
            }
            return i18nContext.i18nc("@title:group a step of the wizard", "Choose categories");
        }
        return i18nContext.i18nc("@title:group a step of the wizard", "Look and preview");
    }

    function stepHelp(index: int): string {
        switch (index) {
        case 0:
            return i18nContext.i18nc("@info", "Choose the period the figures are taken from. For example: this month, or from 01.01.2026 to 31.03.2026.");
        case 1:
            switch (workbench.view.type) {
            case "chart.flow":
                return i18nContext.i18nc("@info", "Choose whose money the chart follows. With none ticked, every account and every category counts. For example: only your checking account.");
            case "chart.candlestick":
                return i18nContext.i18nc("@info", "Choose the accounts whose balance the candles show. With none ticked, your checking, savings and cash accounts count together. For example: only your checking account.");
            case "budget.progress":
                return i18nContext.i18nc("@info", "Choose which budgets get a bar. With none ticked, every budget gets one. For example: Groceries and Leisure.");
            case "chart.custom":
                return i18nContext.i18nc("@info", "Change the text that describes the chart, or start from a ready one with “Insert an example”. The preview below shows the result.");
            case "table.transactions":
                return i18nContext.i18nc("@info", "Choose whose transactions are listed. With none ticked, every open account and every category counts. For example: only your checking account, only Groceries.");
            }
            return i18nContext.i18nc("@info", "Choose which categories are counted. Every category that is not ticked is left out. For example: Groceries and Restaurants, but not Insurance.");
        }
        return i18nContext.i18nc("@info", "Choose how it looks and give it a title and a name. For example: Household costs.");
    }

    function goTo(index: int) {
        if (index >= 0 && index < steps && usable(index)) {
            step = index;
        }
    }

    function next() {
        goTo(usable(step + 1) ? step + 1 : step + 2);
    }

    function back() {
        goTo(usable(step - 1) ? step - 1 : step - 2);
    }

    // Finish keeps the view. A new view without a name gets a default one
    // from the page (M6-B164).
    function finish() {
        finishRequested(wizardName.text);
    }

    // The name of an unfinished statistic that comes back (M6-B164).
    function restoreName(name: string) {
        wizardName.text = name;
    }

    function openAt(index: int) {
        step = usable(index) ? index : 0;
    }

    // Remembers the columns, and puts them back when the user returns from a
    // kind that has none to one that has (and the new one starts empty).
    function keepColumns() {
        const columns = workbench.view.columns;
        const has = columns !== undefined;
        if (has && !hadColumns && columns === "[]" && keptColumns !== "[]" && step === 2) {
            hadColumns = true;
            workbench.setColumns(keptColumns);
            return;
        }
        if (has) {
            keptColumns = columns;
        }
        hadColumns = has;
    }

    Connections {
        target: wizard.workbench

        function onViewChanged() {
            wizard.keepColumns();
        }
    }

    function restart() {
        keptColumns = "[]";
        step = 0;
        wizardName.text = "";
    }

    function refresh() {
        preview.refresh();
    }

    spacing: Kirigami.Units.largeSpacing

    RowLayout {
        Layout.fillWidth: true

        Kirigami.Heading {
            Layout.fillWidth: true
            level: 2
            text: wizard.onOverview
                ? wizard.i18nContext.i18nc("@title:group", "Change the chart on the Overview")
                : wizard.isNew
                ? wizard.i18nContext.i18nc("@title:group", "New statistic")
                : wizard.i18nContext.i18nc("@title:group %1 is the name of a saved view", "Change “%1”", wizard.builder.openName)
            textFormat: Text.PlainText
        }

        Controls.Button {
            flat: true
            visible: !wizard.onOverview
            text: wizard.i18nContext.i18nc("@action:button leave the wizard and see every setting on one page", "Show all settings at once")
            onClicked: {
                ActionLog.taken("StatisticWizard_showAllSettings")
                wizard.showAll()
            }
        }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: wizard.width > Kirigami.Units.gridUnit * 36 ? 2 : 1
        columnSpacing: Kirigami.Units.largeSpacing
        rowSpacing: Kirigami.Units.largeSpacing

        // The steps.
        ColumnLayout {
            Layout.alignment: Qt.AlignTop
            Layout.preferredWidth: Kirigami.Units.gridUnit * 12
            spacing: Kirigami.Units.smallSpacing

            Repeater {
                model: wizard.steps

                delegate: Controls.ItemDelegate {
                    id: entry

                    required property int index

                    Layout.fillWidth: true
                    highlighted: wizard.step === index
                    enabled: wizard.usable(index)
                    text: Plain.plain(wizard.i18nContext.i18nc("@item:inlistbox %1 is the number of a step, %2 its name", "%1. %2", index + 1, wizard.stepName(index)))
                    Accessible.name: wizard.stepName(index)
                    onClicked: {
                        ActionLog.taken("StatisticWizard_goTo")
                        wizard.goTo(index)
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            spacing: Kirigami.Units.largeSpacing

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 3
                text: wizard.stepName(wizard.step)
                textFormat: Text.PlainText
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: wizard.stepHelp(wizard.step)
            }

            // Step 1: the period and the figures it works on.
            ColumnLayout {
                Layout.fillWidth: true
                visible: wizard.step === 0

                Kirigami.FormLayout {
                    Layout.fillWidth: true

                    PeriodChoice {
                        Kirigami.FormData.label: wizard.i18nContext.i18nc("@label:listbox", "Period")
                        options: wizard.builder.periods.map(id => ({word: id, name: wizard.builder.periodName(id)}))
                        word: wizard.builder.period
                        onChosen: word => {
                            ActionLog.taken("StatisticWizard_period")
                            wizard.builder.period = word
                        }
                    }
                }

                PropertiesPanel {
                    Layout.fillWidth: true
                    model: wizard.workbench
                    period: wizard.builder.period
                    choices: wizard.builder.groupsSource
                    only: wizard.rangeKeys
                }
            

                // Only offered when the chosen type has something to put
                // there; an open row with nothing under it did nothing (M6-B14).
                Controls.Button {
                    flat: true
                    visible: wizard.hasFields(wizard.advancedKeys)
                    icon.name: wizard.advanced ? "arrow-down" : "arrow-right"
                    text: wizard.i18nContext.i18nc("@action:button show or hide formulas and fine tuning", "Advanced: formulas and fine tuning")
                    onClicked: {
                        ActionLog.taken("StatisticWizard_advancedFormulasFine")
                        wizard.advanced = !wizard.advanced
                    }
                }

                PropertiesPanel {
                    Layout.fillWidth: true
                    visible: wizard.advanced && wizard.hasFields(wizard.advancedKeys)
                    model: wizard.workbench
                    period: wizard.builder.period
                    choices: wizard.builder.groupsSource
                    only: wizard.advancedKeys
                    showDataset: false
                }
            }

            // Step 2: the board of columns, and the older lists behind
            // Advanced.
            ColumnLayout {
                Layout.fillWidth: true
                visible: wizard.step === 1

                ColumnBoard {
                    Layout.fillWidth: true
                    visible: wizard.hasBoard
                    workbench: wizard.workbench
                    choices: wizard.builder.groupsSource
                    totals: preview.totals
                }

                Controls.Button {
                    flat: true
                    visible: wizard.hasBoard && wizard.hasFields(wizard.seriesKeys)
                    icon.name: wizard.advanced ? "arrow-down" : "arrow-right"
                    text: wizard.i18nContext.i18nc("@action:button show or hide the lists of categories, groups and lines", "Advanced: pick from lists")
                    onClicked: {
                        ActionLog.taken("StatisticWizard_advancedPickLists")
                        wizard.advanced = !wizard.advanced
                    }
                }

                PropertiesPanel {
                    Layout.fillWidth: true
                    visible: !wizard.hasBoard || wizard.advanced
                    model: wizard.workbench
                    period: wizard.builder.period
                    choices: wizard.builder.groupsSource
                    only: wizard.seriesKeys
                    showDataset: false
                }
            }

            // Step 3: how it looks, its title, and the name it is kept under.
            ColumnLayout {
                Layout.fillWidth: true
                visible: wizard.step === 2

                // The kind on the left, its variants as pictures.
                RowLayout {
                    Layout.fillWidth: true
                    visible: true
                    spacing: Kirigami.Units.largeSpacing

                    ColumnLayout {
                        Layout.alignment: Qt.AlignTop
                        spacing: Kirigami.Units.smallSpacing

                        Repeater {
                            model: wizard.kinds

                            delegate: Controls.ItemDelegate {
                                id: kindEntry

                                required property var modelData

                                Layout.fillWidth: true
                                icon.name: wizard.iconOf(modelData.id)
                                text: Plain.plain(wizard.kindName(modelData.id))
                                highlighted: wizard.currentKind.id === modelData.id
                                onClicked: {
                                    ActionLog.taken("StatisticWizard_clicked")
                                    if (!modelData.types.includes(wizard.workbench.view.type)) {
                                        wizard.workbench.setType(modelData.types[0]);
                                    }
                                }
                            }
                        }
                    }

                    Flow {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignTop
                        spacing: Kirigami.Units.largeSpacing

                        Repeater {
                            model: wizard.currentKind.types

                            delegate: Controls.AbstractButton {
                                id: choice

                                required property string modelData

                                width: Kirigami.Units.gridUnit * 7
                                height: Kirigami.Units.gridUnit * 6
                                checkable: true
                                checked: wizard.workbench.view.type === modelData
                                Accessible.name: wizard.builder.typeName(modelData)
                                onClicked: {
                                    ActionLog.taken("StatisticWizard_setType")
                                    wizard.workbench.setType(modelData)
                                }

                                background: Rectangle {
                                    radius: Kirigami.Units.cornerRadius
                                    color: choice.checked ? Kirigami.Theme.highlightColor : Kirigami.Theme.backgroundColor
                                    opacity: choice.checked ? 0.25 : 1
                                    border.width: 1
                                    border.color: choice.checked || choice.visualFocus ? Kirigami.Theme.highlightColor : Kirigami.Theme.disabledTextColor
                                }

                                contentItem: ColumnLayout {
                                    spacing: Kirigami.Units.smallSpacing

                                    Kirigami.Icon {
                                        Layout.alignment: Qt.AlignHCenter
                                        Layout.preferredWidth: Kirigami.Units.iconSizes.large
                                        Layout.preferredHeight: Kirigami.Units.iconSizes.large
                                        source: wizard.iconOf(choice.modelData)
                                    }

                                    Controls.Label {
                                        Layout.fillWidth: true
                                        horizontalAlignment: Text.AlignHCenter
                                        wrapMode: Text.Wrap
                                        textFormat: Text.PlainText
                                        text: wizard.builder.typeName(choice.modelData)
                                    }
                                }
                            }
                        }
                    }
                }

                PropertiesPanel {
                    Layout.fillWidth: true
                    model: wizard.workbench
                    period: wizard.builder.period
                    choices: wizard.builder.groupsSource
                    only: wizard.elementKeys
                    showDataset: false
                }

                Controls.TextField {
                    id: wizardName

                    Layout.fillWidth: true
                    visible: wizard.isNew
                    maximumLength: 200
                    placeholderText: wizard.i18nContext.i18nc("@info:placeholder", "A name for this view")
                    Accessible.name: wizard.i18nContext.i18nc("@label:textbox", "Name of the view")
                    onAccepted: {
                        ActionLog.taken("StatisticWizard_nameView")
                        wizard.finish()
                    }
                }

                Controls.Label {
                    Layout.fillWidth: true
                    visible: wizard.isNew
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: wizard.i18nContext.i18nc("@info", "Finish keeps it under this name, or as “Statistic 1” and so on if you leave the name empty. It then stays in the list of saved views below, and “Put it on the Overview” shows it there.")
                }

                // The colors of the charts are the palette in Settings, so
                // every chart matches (M7a.10, D-589).
                // Not for a chart of the Overview: Settings would leave its
                // change unfinished.
                Controls.Button {
                    visible: !wizard.onOverview
                    text: wizard.i18nContext.i18nc("@action:button opens Settings, where the colors of the charts are changed", "Edit palette…")
                    icon.name: "color-management"
                    onClicked: {
                        ActionLog.taken("StatisticWizard_editPalette")
                        ChartPalette.editRequested()
                    }
                }
            }

            // The live preview: below the options of every step (Q-254).
            Kirigami.AbstractCard {
                Layout.fillWidth: true

                contentItem: ComponentPreview {
                    id: preview

                    subject: wizard.workbench.view
                    period: wizard.builder.period
                    explainChart: true
                    // A double click on the title goes to the step with it.
                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        onDoubleTapped: wizard.goTo(2)
                    }
                }
            }
        }
    }

    Kirigami.Separator {
        Layout.fillWidth: true
    }

    RowLayout {
        Layout.fillWidth: true

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button leave the wizard without a change of the saved view", "Cancel")
            icon.name: "dialog-cancel"
            onClicked: {
                ActionLog.taken("StatisticWizard_cancel")
                wizard.cancelled()
            }
        }

        Item {
            Layout.fillWidth: true
        }

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button go to the step before", "Back")
            icon.name: "go-previous"
            enabled: wizard.step > 0
            onClicked: {
                ActionLog.taken("StatisticWizard_back")
                wizard.back()
            }
        }

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button go to the next step", "Next")
            icon.name: "go-next"
            enabled: wizard.step < wizard.steps - 1
            onClicked: {
                ActionLog.taken("StatisticWizard_next")
                wizard.next()
            }
        }

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button keep the view the wizard made", "Finish")
            icon.name: "dialog-ok-apply"
            highlighted: true
            onClicked: {
                ActionLog.taken("StatisticWizard_finish")
                wizard.finish()
            }
        }
    }
}
