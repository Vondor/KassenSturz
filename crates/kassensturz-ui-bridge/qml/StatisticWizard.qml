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
    readonly property bool isNew: builder.openId === ""
    // The properties that belong to each step; the others are not offered
    // there.
    readonly property var rangeKeys: ["grouping", "measure"]
    readonly property var advancedKeys: ["compare", "formula", "target"]
    readonly property var seriesKeys: ["categories", "groups", "series", "show_hidden"]
    readonly property var elementKeys: ["title", "legend"]
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
        {id: "table", types: ["table.rows"]}
    ]
    readonly property var currentKind: kinds.find(kind => kind.types.includes(workbench.view.type)) ?? kinds[0]
    readonly property alias nameText: wizardName.text

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
        }
        return i18nContext.i18nc("@item:inlistbox a kind of display", "Table");
    }

    function stepName(index: int): string {
        switch (index) {
        case 0:
            return i18nContext.i18nc("@title:group a step of the wizard", "Period");
        case 1:
            return i18nContext.i18nc("@title:group a step of the wizard", "Choose categories");
        }
        return i18nContext.i18nc("@title:group a step of the wizard", "Look and preview");
    }

    function stepHelp(index: int): string {
        switch (index) {
        case 0:
            return i18nContext.i18nc("@info", "Choose the period the figures are taken from. For example: this month, or from 01.01.2026 to 31.03.2026.");
        case 1:
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

    // Finish keeps the view. A new view needs a name first, so the last step
    // is shown when it has none.
    function finish() {
        if (isNew && wizardName.text.trim() === "") {
            step = 2;
            wizardName.forceActiveFocus();
            return;
        }
        finishRequested(wizardName.text);
    }

    function openAt(index: int) {
        step = usable(index) ? index : 0;
    }

    function restart() {
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
            text: wizard.isNew
                ? wizard.i18nContext.i18nc("@title:group", "New statistic")
                : wizard.i18nContext.i18nc("@title:group %1 is the name of a saved view", "Change “%1”", wizard.builder.openName)
            textFormat: Text.PlainText
        }

        Controls.Button {
            flat: true
            text: wizard.i18nContext.i18nc("@action:button leave the wizard and see every setting on one page", "Show all settings at once")
            onClicked: wizard.showAll()
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
                    text: wizard.i18nContext.i18nc("@item:inlistbox %1 is the number of a step, %2 its name", "%1. %2", index + 1, wizard.stepName(index))
                    Accessible.name: wizard.stepName(index)
                    onClicked: wizard.goTo(index)
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
                        onChosen: word => wizard.builder.period = word
                    }
                }

                PropertiesPanel {
                    Layout.fillWidth: true
                    model: wizard.workbench
                    period: wizard.builder.period
                    choices: wizard.builder.groupsSource
                    only: wizard.rangeKeys
                }
            

                Controls.Button {
                    flat: true
                    icon.name: wizard.advanced ? "arrow-down" : "arrow-right"
                    text: wizard.i18nContext.i18nc("@action:button show or hide formulas and fine tuning", "Advanced: formulas and fine tuning")
                    onClicked: wizard.advanced = !wizard.advanced
                }

                PropertiesPanel {
                    Layout.fillWidth: true
                    visible: wizard.advanced
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
                    onClicked: wizard.advanced = !wizard.advanced
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
                                text: wizard.kindName(modelData.id)
                                highlighted: wizard.currentKind.id === modelData.id
                                onClicked: {
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
                                onClicked: wizard.workbench.setType(modelData)

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
                    onAccepted: wizard.finish()
                }

                Controls.Label {
                    Layout.fillWidth: true
                    visible: wizard.isNew
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: wizard.i18nContext.i18nc("@info", "To put it on the Overview, choose Edit layout there, then Add to the page and Saved views.")
                }
            }

            // The live preview: below the options of every step (Q-254).
            Kirigami.AbstractCard {
                Layout.fillWidth: true

                contentItem: ComponentPreview {
                    id: preview

                    subject: wizard.workbench.view
                    period: wizard.builder.period
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
            onClicked: wizard.cancelled()
        }

        Item {
            Layout.fillWidth: true
        }

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button go to the step before", "Back")
            icon.name: "go-previous"
            enabled: wizard.step > 0
            onClicked: wizard.back()
        }

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button go to the next step", "Next")
            icon.name: "go-next"
            enabled: wizard.step < wizard.steps - 1
            onClicked: wizard.next()
        }

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button keep the view the wizard made", "Finish")
            icon.name: "dialog-ok-apply"
            highlighted: true
            onClicked: wizard.finish()
        }
    }
}
