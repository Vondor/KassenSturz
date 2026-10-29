// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The month shown on the overview, built from the report components
// (M6a.8): the key figure cards of the month against the month before, where
// the month's money went as a chart with its table and its five largest
// categories, and the last 12 months' money in and out as a line (Q-215). The month comes
// from the overview's own view (`monthKey`), so going to an earlier or later
// month moves the cards and the chart too. The figures are the ones of the
// reporting layer (D-277, D-285), as text (D-065).
//
// This is the fixed default layout; the user's own dashboards come with the
// grid editor (M6b).

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: month

    // The overview's view (`BookOverview.view`): `monthKey` and `empty`.
    required property var view
    // Objects with a `view` and a `show` like `BookCards` and `BookCharts`;
    // the page gives it the real ones.
    required property var cards
    required property var charts
    // A `BookCharts` of its own for the line of the last 12 months.
    required property var trend

    // How many categories the table of the largest ones lists.
    readonly property int largest: 5

    readonly property var sets: cards.view?.sets ?? []
    readonly property var drawn: charts.view?.charts ?? []
    readonly property var trends: trend.view?.charts ?? []

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Asks for this month's figures again. The overview's view changes when
    // the month, the book or the number format does, and this follows it.
    function reload() {
        // Read from the view itself: when this runs, `monthKey` may still be
        // the old month.
        const key = view.monthKey ?? "";
        if (key === "") {
            return;
        }
        cards.show(key, "previous_period");
        charts.show("spending_by_category", "donut", key, "top-level");
        trend.show("income_expense_by_month", "line", "last-12-months", "each");
    }

    onViewChanged: reload()

    spacing: Kirigami.Units.largeSpacing

    Repeater {
        model: month.sets

        delegate: ColumnLayout {
            id: currencySet

            required property var modelData

            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            // Only a book with several currencies needs to say which.
            Kirigami.Heading {
                visible: month.sets.length > 1
                text: currencySet.modelData.currency
                level: 3
                textFormat: Text.PlainText
            }

            KpiGrid {
                cards: currencySet.modelData.cards
            }
        }
    }

    Repeater {
        model: month.drawn

        delegate: ColumnLayout {
            id: spent

            required property var modelData

            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.largeSpacing

            ChartView {
                Layout.fillWidth: true
                chart: spent.modelData
                title: month.drawn.length > 1
                    ? month.i18nContext.i18nc("@title %1 is a currency code, e.g. EUR", "Where your money went in %1", spent.modelData.currency)
                    : month.i18nContext.i18nc("@title", "Where your money went")
            }

            // The categories the most went to, from the same rows as the chart.
            Kirigami.Heading {
                Layout.fillWidth: true
                level: 3
                textFormat: Text.PlainText
                text: month.drawn.length > 1
                    ? month.i18nContext.i18nc("@title %1 is a currency code, e.g. EUR", "Your biggest spending in %1", spent.modelData.currency)
                    : month.i18nContext.i18nc("@title", "Your biggest spending")
            }

            ChartTable {
                Layout.fillWidth: true
                chart: spent.modelData
                limit: month.largest
            }
        }
    }

    // The last 12 months, whatever month the page shows.
    Repeater {
        model: month.trends

        delegate: ChartView {
            required property var modelData

            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.largeSpacing
            chart: modelData
            title: month.trends.length > 1
                ? month.i18nContext.i18nc("@title %1 is a currency code, e.g. EUR", "Money in and out, last 12 months, in %1", modelData.currency)
                : month.i18nContext.i18nc("@title", "Money in and out, last 12 months")
        }
    }
}
