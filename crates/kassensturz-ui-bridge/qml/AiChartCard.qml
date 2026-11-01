// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A chart the AI prepared, shown in the chat as a preview (M6c.9, D-355,
// D-402): the real chart, drawn by KassenSturz from the book's own figures,
// the AI-made label and the title as plain text, and three ways on. *Keep* is
// the confirm: the chart is saved in the Statistic Builder and Undo takes it back.
// *Change…* hands the chart to the Statistic Builder; what the user saves
// there is the user's own chart (D-297), so this one is thrown away. *Leave
// out* throws it away. Nothing is saved before *Keep* (D-200). Every text a
// model wrote is plain text (T-34); a number in the title that no function
// produced is marked like one in an answer (D-188).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.AbstractCard {
    id: card

    required property AiSwitch ai
    // The `AiChat`.
    required property var chat
    // The ID of the set that holds the chart; empty for a message that is
    // not a chart, which then shows nothing.
    property string setId: ""
    // The numbers in the title that no function produced.
    property var unverified: []
    // The name of the dashboard the Overview shows, which *Add to* puts the
    // chart on (M6d.1, D-438); empty says the Overview.
    property string dashboardName: ""

    // The user wants to make the chart in the Statistic Builder, starting
    // from this one.
    signal changeClicked(string title, string component, string period)
    // The user wants the kept chart on the Overview (D-428): the component
    // as JSON text and its period, as D-326 adds a saved view.
    signal addToOverviewClicked(string component, string period)
    // What became of the chart: `kept`, `added`, `builder` or `left`.
    signal handled(string outcome)

    // What the user did: empty while the chart waits, then `kept`, `added`,
    // `builder`, `left` or `outOfDate`. A card made again for a chart that
    // was dealt with starts with it and loads nothing.
    property string outcome: ""
    // The refusal of the book when keeping failed, as `FormError` shows it.
    property var refusal: ({})
    // The chart as `AiChat.changes` gave it: the title, the period and the
    // component as JSON text.
    property var chart: null

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
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

    function keep() {
        const result = chat.confirm(setId);
        if (result.code !== undefined) {
            refusal = result;
            return;
        }
        refusal = ({});
        outcome = "kept";
        handled("kept");
    }

    // After *Keep*: the chart, already saved, is also put on the Overview as
    // a copy, the way a saved view is (D-326, D-428). Only this button does
    // it; keeping alone never touches the Overview.
    function addToOverview() {
        if (chart === null || outcome !== "kept") {
            return;
        }
        outcome = "added";
        handled("added");
        addToOverviewClicked(chart.component, chart.period);
    }

    function change() {
        if (chart === null) {
            return;
        }
        // The chart is the user's to work on now: this one is not kept.
        chat.discard(setId);
        outcome = "builder";
        handled("builder");
        changeClicked(chart.title, chart.component, chart.period);
    }

    function leaveOut() {
        chat.discard(setId);
        outcome = "left";
        handled("left");
    }

    // The chart is read by the same code as one made by hand, so the preview
    // shows exactly what would be saved.
    ComponentProperties {
        id: chartProperties
    }

    Component.onCompleted: {
        if (setId === "" || outcome !== "") {
            return;
        }
        const rows = chat.changes(setId);
        if (rows.code !== undefined || rows.length === 0 || rows[0].kind !== "newView") {
            outcome = "outOfDate";
            return;
        }
        chart = rows[0];
        chartProperties.load(chart.component);
    }

    visible: ai.enabled

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            font.bold: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@label a chart that an AI prepared, not KassenSturz", "Chart prepared by the AI")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.outcome === "outOfDate"
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@info", "This chart is no longer available. Ask for it again if you still want it.")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.chart !== null
            font.bold: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.chart !== null ? card.chart.title : ""
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            visible: card.chart !== null && card.unverified.length > 0
            text: visible ? Plain.plain(card.i18nContext.i18nc("@info %1 is a list of numbers", "Don't rely on the numbers in this title: they don't come from your book, so the AI may have made them up: %1", card.unverified.join(", "))) : ""
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.chart !== null
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: card.chart !== null ? card.periodName(card.chart.period) : ""
        }

        ComponentPreview {
            Layout.fillWidth: true
            visible: card.chart !== null
            subject: chartProperties.view
            period: card.chart !== null ? card.chart.period : "last-12-months"
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.chart !== null && card.outcome === ""
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@info", "The AI chose what this chart shows. The figures are worked out by KassenSturz from your book, not by the AI. It is saved in the Statistic Builder only when you keep it, and Undo removes it again.")
        }

        FormError {
            Layout.fillWidth: true
            result: card.refusal
        }

        RowLayout {
            Layout.fillWidth: true
            visible: card.chart !== null && card.outcome === ""

            Controls.Button {
                icon.name: "dialog-ok-apply"
                text: card.i18nContext.i18nc("@action:button save the chart the AI prepared", "Keep")
                onClicked: {
                    ActionLog.taken("AiChartCard_keep")
                    card.keep()
                }
            }

            Controls.Button {
                icon.name: "document-edit"
                text: card.i18nContext.i18nc("@action:button open the chart the AI prepared in the Statistic Builder to change it", "Change…")
                onClicked: {
                    ActionLog.taken("AiChartCard_change")
                    card.change()
                }
            }

            Controls.Button {
                icon.name: "list-remove"
                text: card.i18nContext.i18nc("@action:button throw the chart the AI prepared away", "Leave out")
                onClicked: {
                    ActionLog.taken("AiChartCard_leaveOut")
                    card.leaveOut()
                }
            }
        }

        Controls.Button {
            visible: card.chart !== null && card.outcome === "kept"
            icon.name: "view-grid"
            text: card.dashboardName === ""
                ? card.i18nContext.i18nc("@action:button put the chart the AI prepared on the Overview page", "Add to Overview")
                : card.i18nContext.i18nc("@action:button put the chart the AI prepared on a dashboard, %1 is its name", "Add to %1", card.dashboardName)
            onClicked: {
                ActionLog.taken("AiChartCard_addToOverview")
                card.addToOverview()
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.outcome === "kept" || card.outcome === "added" || card.outcome === "builder" || card.outcome === "left"
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
            textFormat: Text.PlainText
            text: card.outcome === "kept"
                ? card.i18nContext.i18nc("@info", "Kept. The chart is saved in the Statistic Builder. You can take it back with Undo.")
                : card.outcome === "added"
                    ? card.i18nContext.i18nc("@info", "Kept and added to the Overview. You can take it back with Undo.")
                : card.outcome === "builder"
                    ? card.i18nContext.i18nc("@info", "Opened in the Statistic Builder. Save it there if you want to keep it: it is then your own chart.")
                    : card.i18nContext.i18nc("@info", "Left out. Nothing was changed.")
        }
    }
}
