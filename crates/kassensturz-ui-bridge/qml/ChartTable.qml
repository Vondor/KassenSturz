// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The same values as a chart, as a table of text (M6a.4, D-283): the way a
// screen reader reads a chart, and for anyone who prefers numbers. Every cell
// is text that Rust wrote (D-065). It is a `DataTable` (M6a.5) with the
// chart's column names put into words and the money with no category named.

import QtQuick
import org.kde.ki18n

DataTable {
    id: table

    // The chart object from `BookCharts.view.charts`.
    required property var chart
    // List only the first rows (0 for all of them), e.g. the largest
    // categories of a spending chart, which the dataset orders largest first.
    property int limit: 0

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The first column of a spending table names the category; money without
    // one has an empty name.
    function shownRows(): var {
        const rows = limit > 0 ? chart.table.rows.slice(0, limit) : chart.table.rows;
        return rows.map((row, index) => (chart.unsortedIndex === index)
            ? [i18nContext.i18nc("@label money with no category yet", "Not sorted yet"), ...row.slice(1)]
            : (chart.round && chart.slices.other === index)
                ? [i18nContext.i18nc("@label the smaller slices of a pie put together", "Other"), ...row.slice(1)]
                : (chart.round && chart.slices.undrawn[index] === true)
                    ? [i18nContext.i18nc("@label %1 is a category, whose total is money that came back", "%1 (money that came back, not drawn)", row[0]), ...row.slice(1)]
                    : row);
    }

    columns: chart.table.columns
    rows: shownRows()
    title: limit > 0
        ? i18nContext.i18nc("@title", "Your biggest spending")
        : i18nContext.i18nc("@label", "Values of the chart as a table")
    columnNames: ({
        "category": i18nContext.i18nc("@title:column a table of a chart", "Category"),
        "total": i18nContext.i18nc("@title:column a table of a chart", "Total"),
        "month": i18nContext.i18nc("@title:column a table of a chart", "Month"),
        "income": i18nContext.i18nc("@title:column a table of a chart", "Money in"),
        "expenses": i18nContext.i18nc("@title:column a table of a chart", "Money out"),
        "net": i18nContext.i18nc("@title:column a table of a chart", "Left over")
    })
}
