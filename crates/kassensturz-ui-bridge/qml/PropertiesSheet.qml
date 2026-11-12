// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Properties dialog of a component on the Overview (the panel of M6a.6),
// with the figures it shows now. A change goes through `model`, and the page
// keeps it as part of the session's undo.
//
// A Kirigami.Dialog, not an OverlaySheet: the sheet of KF 6.x prints
// "binding loop" for its implicitHeight on every opening, whatever it holds
// (M6-B10).
//
// The page makes this dialog when it opens and it destroys itself when it
// closes (M6-B197): made together with the Overview page, Kirigami's dialog
// (seen with KF 6.29) kept the height of its content at -1 and opened with
// only its title and Close; made when it opens, it measures what it holds.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Kirigami.Dialog {
    id: sheet

    // A `ComponentProperties` holding the component.
    required property var model
    // The period the component shows, as `PropertiesPanel.period` takes it.
    property string period: "last-12-months"
    // A `CategoryGroups`, for the lists the panel offers.
    property var choices: null
    // The kinds the Statistic Builder can change step by step (M6-B164).
    property var builderTypes: []

    // The user asked for the Statistic Builder on this component, with the
    // component's JSON and the word of its period.
    signal builderRequested(string component, string period)

    readonly property var view: model.view
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title:window", "Properties")
    padding: Kirigami.Units.largeSpacing
    preferredWidth: Kirigami.Units.gridUnit * 32
    // Room for the dialog's opening movement, which starts two grid units
    // low: a dialog as tall as the window was pushed back up while it moved,
    // and Qt printed a binding loop for its `y`.
    maximumHeight: parent ? parent.height - Kirigami.Units.gridUnit * 6 : absoluteMaximumHeight
    standardButtons: Kirigami.Dialog.Close
    onClosed: destroy()

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        // The same chart in the Statistic Builder's steps (M6-B164); Finish
        // there brings the change back to the page.
        Controls.Button {
            visible: sheet.builderTypes.includes(sheet.view?.type ?? "")
            text: sheet.i18nContext.i18nc("@action:button opens the Statistic Builder on this chart", "Change step by step in the Statistic Builder")
            icon.name: "tools-wizard"
            onClicked: {
                ActionLog.taken("PropertiesSheet_changeInBuilder")
                const component = sheet.view.json;
                const period = sheet.view.period?.word ?? "";
                // Before closing: the page forgets which component it was
                // once the dialog is closed.
                sheet.builderRequested(component, period);
                sheet.close();
            }
        }

        // A chart's colors are the palette in Settings, shared by every chart
        // (M7a.10, D-589); this leads there (M6-B164).
        Controls.Button {
            visible: (sheet.view?.type ?? "").startsWith("chart.")
            text: sheet.i18nContext.i18nc("@action:button opens Settings, where the colors of the charts are changed", "Edit palette…")
            icon.name: "color-management"
            onClicked: {
                ActionLog.taken("PropertiesSheet_editPalette")
                sheet.close();
                ChartPalette.editRequested();
            }
        }

        Loader {
            Layout.fillWidth: true
            active: sheet.view?.type !== undefined
            sourceComponent: PropertiesPanel {
                model: sheet.model
                withPeriod: true
                period: sheet.period
                choices: sheet.choices
            }
        }

        // The preview area has a fixed height, so the dialog does not shrink
        // when a choice leaves it with only a short message (M6-B12); what
        // does not fit scrolls.
        Controls.ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 22
            // Headings, notices and boxes have no figures to show.
            visible: ["chart", "table", "card", "progress", "budget-table"].includes(sheet.view?.preview?.shows)
            contentWidth: availableWidth
            clip: true

            Loader {
                width: parent.width
                active: sheet.view?.preview !== undefined
                sourceComponent: ComponentPreview {
                    subject: sheet.view
                    period: sheet.period
                    explainChart: true
                }
            }
        }
    }
}
