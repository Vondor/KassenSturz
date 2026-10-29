// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The same values as a chart, as a table of text (M6a.4, D-283): the way a
// screen reader reads a chart, and for anyone who prefers numbers. Every cell
// is text that Rust wrote (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

GridLayout {
    id: table

    // The chart object from `BookCharts.view.charts`.
    required property var chart
    readonly property var names: chart.table.columns
    readonly property var rows: chart.table.rows

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function columnName(key: string): string {
        switch (key) {
        case "category":
            return i18nContext.i18nc("@title:column a table of a chart", "Category");
        case "total":
            return i18nContext.i18nc("@title:column a table of a chart", "Total");
        case "month":
            return i18nContext.i18nc("@title:column a table of a chart", "Month");
        case "income":
            return i18nContext.i18nc("@title:column a table of a chart", "Money in");
        case "expenses":
            return i18nContext.i18nc("@title:column a table of a chart", "Money out");
        case "net":
            return i18nContext.i18nc("@title:column a table of a chart", "Left over");
        }
        return "";
    }

    // The first column of a spending table names the category; money without
    // one has an empty name.
    function cellText(row: int, column: int): string {
        const text = rows[row][column];
        return (chart.unsortedIndex === row && column === 0)
            ? i18nContext.i18nc("@label money with no category yet", "Not sorted yet")
            : text;
    }

    columns: names.length
    columnSpacing: Kirigami.Units.largeSpacing
    rowSpacing: Kirigami.Units.smallSpacing

    Accessible.role: Accessible.Table
    Accessible.name: i18nContext.i18nc("@label", "Values of the chart as a table")

    Repeater {
        model: table.names

        delegate: Controls.Label {
            required property string modelData
            required property int index

            Layout.fillWidth: index === 0
            Layout.alignment: index === 0 ? Qt.AlignLeft : Qt.AlignRight
            font.bold: true
            textFormat: Text.PlainText
            text: table.columnName(modelData)
            Accessible.role: Accessible.ColumnHeader
        }
    }

    Repeater {
        model: table.rows.length * table.names.length

        delegate: Controls.Label {
            required property int index
            readonly property int row: Math.floor(index / table.names.length)
            readonly property int column: index % table.names.length

            Layout.fillWidth: column === 0
            Layout.alignment: column === 0 ? Qt.AlignLeft : Qt.AlignRight
            elide: Text.ElideRight
            // Names and amounts come from the book: never markup (D-143).
            textFormat: Text.PlainText
            text: table.cellText(row, column)
            Accessible.role: Accessible.Cell
        }
    }
}
