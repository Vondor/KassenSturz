// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Shows one component of the properties panel with the real figures of the
// open book (M6a.7): a chart, a table, a key figure card or a progress bar.
// What to ask `BookCharts` and `BookCards` for comes from the component's
// `preview` object (properties.rs), so nothing is mapped here. Every figure
// is text from Rust (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: preview

    // `ComponentProperties.view`.
    required property var subject
    // One of the fixed period names (`this-month`, `last-month`,
    // `last-12-months`, `this-year`, `last-year`).
    property string period: "last-12-months"

    readonly property var asked: subject.preview ?? ({})
    readonly property var charts: chartData.view?.charts ?? []
    readonly property var cardSets: cardData.view?.sets ?? []
    readonly property bool failed: asked.shows === "card" || asked.shows === "progress"
        ? cardData.view?.error !== undefined : chartData.view?.error !== undefined
    readonly property bool nothing: !failed && (asked.shows === "card" || asked.shows === "progress"
        ? cardSets.length === 0 : charts.length === 0)
    readonly property string title: {
        const field = (subject.fields ?? []).find(f => f.key === "title");
        return field ? field.text : "";
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    KpiNames {
        id: names
    }

    BookCharts {
        id: chartData
    }

    BookCards {
        id: cardData
    }

    // Asks again, e.g. after the book changed.
    function refresh() {
        show();
    }

    function show() {
        // Read from `subject` itself: this runs from its change signal, before
        // the bindings that depend on it (`asked`) have been worked out again.
        const want = subject.preview;
        if (want === undefined) {
            return;
        }
        if (want.shows === "card" || want.shows === "progress") {
            cardData.show(period, want.compare);
        } else {
            chartData.showWith(subject.dataset, want.chart, period, want.grouping, want.chosen, want.showHidden);
        }
    }

    // The card of the measure in each currency's set.
    readonly property var shownCards: cardSets
        .map(set => set.cards.find(card => card.measure === asked.measure))
        .filter(card => card !== undefined)

    onSubjectChanged: show()
    onPeriodChanged: show()
    Component.onCompleted: show()

    spacing: Kirigami.Units.largeSpacing

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: preview.failed
        type: Kirigami.MessageType.Warning
        text: preview.i18nContext.i18nc("@info", "This couldn't be shown right now. Try another period, or open the page again.")
    }

    Kirigami.PlaceholderMessage {
        Layout.fillWidth: true
        visible: preview.nothing
        text: preview.i18nContext.i18nc("@info", "There is nothing to show for this period yet. Add transactions, or choose another period.")
    }

    Repeater {
        model: preview.asked.shows === "chart" ? preview.charts : []

        ChartView {
            required property var modelData

            Layout.fillWidth: true
            chart: modelData
            title: preview.title
        }
    }

    Repeater {
        model: preview.asked.shows === "table" ? preview.charts : []

        ColumnLayout {
            id: tableBox

            required property var modelData

            Layout.fillWidth: true

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 3
                text: preview.title
                visible: text.length > 0
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
            }

            ChartTable {
                Layout.fillWidth: true
                chart: tableBox.modelData
            }
        }
    }

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 3
        text: preview.title
        visible: text.length > 0 && (preview.asked.shows === "card" || preview.asked.shows === "progress")
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    Repeater {
        model: preview.asked.shows === "card" ? preview.shownCards : []

        KpiCard {
            Layout.fillWidth: true
        }
    }

    Repeater {
        model: preview.asked.shows === "progress" ? preview.shownCards : []

        ProgressMeter {
            required property var modelData

            Layout.fillWidth: true
            title: names.measure("savings-rate")
            hasValue: modelData.value !== ""
            valueText: modelData.value !== "" ? modelData.value : names.noValue()
            permille: modelData.permille ?? 0
        }
    }
}
