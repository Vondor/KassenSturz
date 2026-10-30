// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Statistic Builder's wizard for a new view (M6b.R7, D-335), in the way
// of a chart wizard: pick how it looks, what it shows, the details, and a
// name. It decides nothing itself: every choice goes to the same workbench
// the settings screen uses, and the preview under it shows each change at
// once. "Show all settings at once" gives that screen back.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: wizard

    // The Statistic Builder page, for the workbench, the period and the
    // names of types and periods.
    required property var builder

    property int step: 0
    readonly property int steps: 4
    readonly property var workbench: builder.workbench
    // The properties that belong to each step; the others are not offered
    // there.
    readonly property var whatKeys: ["grouping", "categories", "groups", "series", "measure", "compare", "formula", "target"]
    readonly property var detailKeys: ["show_hidden", "legend"]
    readonly property bool hasDetails: (workbench.view.fields ?? []).some(field => detailKeys.includes(field.key))

    signal showAll()
    signal saveRequested(string name)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function iconOf(id: string): string {
        switch (id) {
        case "chart.bar":
            return "office-chart-bar";
        case "chart.pie":
            return "office-chart-pie";
        case "chart.donut":
            return "office-chart-ring";
        case "chart.line":
            return "office-chart-line";
        case "chart.area":
            return "office-chart-area";
        case "chart.stacked_bar":
            return "office-chart-bar-stacked";
        case "kpi.card":
            return "view-statistics";
        case "progress.bar":
            return "office-chart-bar";
        case "table.rows":
            return "view-list-details";
        }
        return "office-chart-bar";
    }

    function stepName(index: int): string {
        switch (index) {
        case 0:
            return i18nContext.i18nc("@title:group a step of the wizard", "How should it look?");
        case 1:
            return i18nContext.i18nc("@title:group a step of the wizard", "What should it show?");
        case 2:
            return i18nContext.i18nc("@title:group a step of the wizard", "Details");
        }
        return i18nContext.i18nc("@title:group a step of the wizard", "Title and name");
    }

    function restart() {
        step = 0;
        wizardName.text = "";
    }

    spacing: Kirigami.Units.largeSpacing

    RowLayout {
        Layout.fillWidth: true

        Kirigami.Heading {
            Layout.fillWidth: true
            level: 2
            text: wizard.i18nContext.i18nc("@title:group %1 is the number of the step, %2 how many there are, %3 its name", "Step %1 of %2: %3", wizard.step + 1, wizard.steps, wizard.stepName(wizard.step))
            textFormat: Text.PlainText
        }

        Controls.Button {
            flat: true
            text: wizard.i18nContext.i18nc("@action:button leave the wizard and see every setting on one page", "Show all settings at once")
            onClicked: wizard.showAll()
        }
    }

    // Step 1: how it looks, as pictures.
    GridLayout {
        Layout.fillWidth: true
        visible: wizard.step === 0
        columns: Math.max(2, Math.floor(width / (Kirigami.Units.gridUnit * 7)))
        columnSpacing: Kirigami.Units.largeSpacing
        rowSpacing: Kirigami.Units.largeSpacing

        Repeater {
            model: wizard.builder.types

            delegate: Controls.AbstractButton {
                id: choice

                required property string modelData

                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 6
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

    // Step 2: what it shows, and for which period.
    ColumnLayout {
        Layout.fillWidth: true
        visible: wizard.step === 1

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
            only: wizard.whatKeys
        }
    }

    // Step 3: the details that are left.
    ColumnLayout {
        Layout.fillWidth: true
        visible: wizard.step === 2

        Controls.Label {
            Layout.fillWidth: true
            visible: !wizard.hasDetails
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: wizard.i18nContext.i18nc("@info", "There is nothing more to set for this one. Go on to the title and the name.")
        }

        PropertiesPanel {
            Layout.fillWidth: true
            model: wizard.workbench
            period: wizard.builder.period
            choices: wizard.builder.groupsSource
            only: wizard.detailKeys
            showDataset: false
        }
    }

    // Step 4: the title in the view, and the name it is kept under.
    ColumnLayout {
        Layout.fillWidth: true
        visible: wizard.step === 3

        PropertiesPanel {
            Layout.fillWidth: true
            model: wizard.workbench
            period: wizard.builder.period
            choices: wizard.builder.groupsSource
            only: ["title"]
            showDataset: false
        }

        RowLayout {
            Layout.fillWidth: true

            Controls.TextField {
                id: wizardName

                Layout.fillWidth: true
                maximumLength: 200
                placeholderText: wizard.i18nContext.i18nc("@info:placeholder", "A name for this view")
                Accessible.name: wizard.i18nContext.i18nc("@label:textbox", "Name of the view")
                onAccepted: wizard.saveRequested(text)
            }

            Controls.Button {
                text: wizard.i18nContext.i18nc("@action:button keep the view the wizard made", "Save the view")
                icon.name: "document-save"
                enabled: wizardName.text.trim() !== ""
                onClicked: wizard.saveRequested(wizardName.text)
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: wizard.i18nContext.i18nc("@info", "To put it on the Overview, choose Edit layout there and add it with Add a saved view.")
        }
    }

    RowLayout {
        Layout.fillWidth: true

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button go to the step before", "Back")
            icon.name: "go-previous"
            enabled: wizard.step > 0
            onClicked: --wizard.step
        }

        Item {
            Layout.fillWidth: true
        }

        Controls.Button {
            text: wizard.i18nContext.i18nc("@action:button go to the next step", "Next")
            icon.name: "go-next"
            visible: wizard.step < wizard.steps - 1
            onClicked: ++wizard.step
        }
    }
}
