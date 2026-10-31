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

    // The fixed words of the balances of the accounts (M6c.2): whether an
    // account is one you have money in or one you owe on, and the rows of the
    // table of what you have, owe and are worth.
    function haveOrOwe(word: string): string {
        switch (word) {
        case "have":
            return i18nContext.i18nc("@label a row of a table: money the user has", "What you have");
        case "owe":
            return i18nContext.i18nc("@label a row of a table: money the user owes", "What you owe");
        case "worth":
            return i18nContext.i18nc("@label a row of a table: what the user has less what they owe", "What you are worth");
        }
        return word;
    }

    // The first column of a spending table names the category; money without
    // one has an empty name. The balances of the accounts have words in the
    // column `kind` or `figure`, which are put into plain words.
    function shownRows(): var {
        const rows = limit > 0 ? chart.table.rows.slice(0, limit) : chart.table.rows;
        const words = chart.table.columns.map(name => name === "kind" || name === "figure");
        if (chart.dataset === "account_balances") {
            return rows.map(row => row.map((cell, column) => words[column] ? haveOrOwe(cell) : cell));
        }
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
        "net": i18nContext.i18nc("@title:column a table of a chart", "Left over"),
        "account": i18nContext.i18nc("@title:column a table of a chart", "Account"),
        "kind": i18nContext.i18nc("@title:column a table of a chart: whether the user has the money or owes it", "Have or owe"),
        "figure": i18nContext.i18nc("@title:column a table of a chart: what has been added up", "Figure"),
        "balance": i18nContext.i18nc("@title:column a table of a chart", "Balance"),
        "assets": i18nContext.i18nc("@title:column a table of a chart: what the user has", "You have"),
        "liabilities": i18nContext.i18nc("@title:column a table of a chart: what the user owes", "You owe"),
        "net_worth": i18nContext.i18nc("@title:column a table of a chart: what the user has less what they owe", "You are worth"),
        "start": i18nContext.i18nc("@title:column a table of a chart: what the accounts held at the start of the month", "At the start"),
        "money_in": i18nContext.i18nc("@title:column a table of a chart", "Money in"),
        "money_out": i18nContext.i18nc("@title:column a table of a chart", "Money out"),
        "starting": i18nContext.i18nc("@title:column a table of a chart: starting balances set in the month", "Starting balances"),
        "end": i18nContext.i18nc("@title:column a table of a chart: what the accounts held at the end of the month", "At the end")
    })
}
