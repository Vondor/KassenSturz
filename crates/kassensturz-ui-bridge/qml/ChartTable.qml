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
    // Which changes of a comparison stay uncoloured (`props.neutral`,
    // US6, #789, D-760): `off` colours every change, a number those smaller
    // than that many percent. The words are `NEUTRAL_WORDS` in order.
    property string neutral: "off"

    // How each row of a comparison changed, worked out in Rust from the
    // exact amounts (D-065): direction, tone and how many steps it reaches.
    readonly property var compared: chart.compared ?? ({ marks: [], total: [], totalMark: ({}) })
    readonly property int neutralSteps: Math.max(0, ["off", "2", "5", "10", "20"].indexOf(neutral))
    readonly property int percentColumn: chart.table.columns.indexOf("change_percent")

    // Less spent is green, more red; a change smaller than `neutral` and no
    // change stay uncoloured.
    function toneOf(mark): string {
        if (mark?.tone === undefined || mark.tone === "same" || mark.reach < neutralSteps) {
            return "";
        }
        return mark.tone;
    }

    // The arrow goes before the percent whatever the colour, so colour is
    // never the only sign (D-283), as on the key figure cards.
    function withArrow(row, mark): var {
        const arrow = mark?.direction === "up" ? "↑" : mark?.direction === "down" ? "↓" : "";
        if (arrow === "" || percentColumn < 0) {
            return row;
        }
        const cells = Array.from(row);
        cells[percentColumn] = cells[percentColumn] === "" ? arrow : arrow + " " + cells[percentColumn];
        return cells;
    }

    // The direction in words, for screen readers.
    function noteOf(mark): string {
        switch (mark?.direction) {
        case "up":
            return i18nContext.i18nc("@info a row of a table that compares spending", "More spent than in the comparison");
        case "down":
            return i18nContext.i18nc("@info a row of a table that compares spending", "Less spent than in the comparison");
        case "same":
            return i18nContext.i18nc("@info a row of a table that compares spending", "As much as in the comparison");
        }
        return "";
    }

    function markOf(index: int): var {
        return compared.marks.length > index ? compared.marks[index] : undefined;
    }

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
        // The table of a custom chart has a row for each value, not for each
        // place: money with no category yet has an empty first cell.
        if (chart.table.columns.length === 3 && chart.table.columns[1] === "series") {
            return rows.map(row => row[0] === "" && chart.unsortedIndex >= 0
                ? [i18nContext.i18nc("@label money with no category yet", "Not sorted yet"), ...row.slice(1)]
                : row);
        }
        return rows.map((row, index) => (chart.unsortedIndex === index)
            ? [i18nContext.i18nc("@label money with no category yet", "Not sorted yet"), ...row.slice(1)]
            // A pie and one bar split into the categories (D-666) both have
            // "Other" and refunds that are not drawn.
            : (chart.slices.other === index)
                ? [i18nContext.i18nc("@label the smaller slices of a pie put together", "Other"), ...row.slice(1)]
                : (chart.slices.undrawn[index] === true)
                    ? [i18nContext.i18nc("@label %1 is a category, whose total is money that came back", "%1 (money that came back, not drawn)", row[0]), ...row.slice(1)]
                    : row);
    }

    columns: chart.table.columns
    rows: shownRows().map((row, index) => withArrow(row, markOf(index)))
    tones: shownRows().map((row, index) => toneOf(markOf(index)))
    tonedColumns: compared.marks.length > 0
        ? ["change", "change_percent"].map(name => chart.table.columns.indexOf(name)).filter(column => column >= 0)
        : []
    notes: shownRows().map((row, index) => noteOf(markOf(index)))
    // The total row of a comparison; a list of the biggest few has none.
    totalRow: compared.total.length > 0 && limit === 0
        ? withArrow([i18nContext.i18nc("@label the last row of a table: all rows together", "Total"), ...Array.from(compared.total).slice(1)], compared.totalMark)
        : []
    totalTone: toneOf(compared.totalMark)
    totalNote: noteOf(compared.totalMark)
    title: limit > 0
        ? i18nContext.i18nc("@title", "Your biggest spending")
        : i18nContext.i18nc("@label", "Values of the chart as a table")
    columnNames: ({
        "category": i18nContext.i18nc("@title:column a table of a chart", "Category"),
        "total": i18nContext.i18nc("@title:column a table of a chart", "Total"),
        "per_month": i18nContext.i18nc("@title:column a table of a chart: the average spending per month", "Per month"),
        "per_quarter": i18nContext.i18nc("@title:column a table of a chart: the average spending per quarter", "Per quarter"),
        "per_year": i18nContext.i18nc("@title:column a table of a chart: the average spending per year", "Per year"),
        "month": i18nContext.i18nc("@title:column a table of a chart", "Month"),
        "income": i18nContext.i18nc("@title:column a table of a chart", "Money in"),
        "expenses": i18nContext.i18nc("@title:column a table of a chart", "Money out"),
        "net": i18nContext.i18nc("@title:column a table of a chart", "Left over"),
        // A spending table with a comparison (US3.1a).
        "previous_period": i18nContext.i18nc("@title:column a table of a chart: spending in the period before", "Period before"),
        "same_last_year": i18nContext.i18nc("@title:column a table of a chart: spending in the same days a year earlier", "Same period last year"),
        "average_3": i18nContext.i18nc("@title:column a table of a chart: the average spending of earlier periods", "Average of the 3 periods before"),
        "average_6": i18nContext.i18nc("@title:column a table of a chart: the average spending of earlier periods", "Average of the 6 periods before"),
        "average_12": i18nContext.i18nc("@title:column a table of a chart: the average spending of earlier periods", "Average of the 12 periods before"),
        "change": i18nContext.i18nc("@title:column a table of a chart: how much more or less was spent than in the comparison", "Change"),
        "change_percent": i18nContext.i18nc("@title:column a table of a chart: the change in percent of the comparison", "Change in %"),
        "series": i18nContext.i18nc("@title:column the table of a custom chart: which line, bars or dots the value is of", "Series"),
        "value": i18nContext.i18nc("@title:column the table of a custom chart", "Value"),
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
        "end": i18nContext.i18nc("@title:column a table of a chart: what the accounts held at the end of the month", "At the end"),
        // The candles of a balance (US4.10): each row is a day, week or
        // month, with its start and end in the columns above.
        "period": i18nContext.i18nc("@title:column a table of a candle chart: the day, week or month of a row", "Period"),
        "highest": i18nContext.i18nc("@title:column a table of a candle chart: the highest balance at the end of a day", "Highest"),
        "lowest": i18nContext.i18nc("@title:column a table of a candle chart: the lowest balance at the end of a day", "Lowest")
    })
}
