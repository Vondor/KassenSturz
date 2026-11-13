// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Rows of text under column names (M6a.5). It shows a `table` object of a
// chart as it is (columns, rows: all text), so it is also the screen-reader
// path for a chart (D-283). Nothing here adds up or sorts figures (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: table

    // The `columns` (fixed names) and `rows` (a list of lists of text) of a
    // chart's table.
    property var columns: []
    property var rows: []
    // The words for the column names: `{ key: "words" }`. A name that isn't
    // in it is shown as it is. The names are fixed names, not text (D-034),
    // so the screen that knows them translates them.
    property var columnNames: ({})
    // What the table is about, for screen readers.
    property string title
    // A comparison (US6, #789, D-760): for each row `good`, `bad` or "" (no
    // colour), the columns that take it, and words for screen readers, so
    // colour is never the only sign (D-283). Empty for other tables.
    property var tones: []
    property var tonedColumns: []
    property var notes: []
    // A total row under the others, already in words; empty for none.
    property var totalRow: []
    property string totalTone
    property string totalNote

    function toneColor(tone: string): color {
        return tone === "good" ? Kirigami.Theme.positiveTextColor
            : tone === "bad" ? Kirigami.Theme.negativeTextColor
            : Kirigami.Theme.textColor;
    }

    readonly property int columnCount: columns.length

    spacing: 0
    Accessible.role: Accessible.Table
    Accessible.name: title

    // Measures text in the table's font, so a figure column is never narrower
    // than its longest figure (M6-B26), in the font the labels use,
    // Kirigami's, not the application's, which can be smaller.
    FontMetrics {
        id: metrics
        font: Kirigami.Theme.defaultFont
    }

    // The headers are bold, which is wider than a margin on the normal font
    // allowed for: "Pro Monat" was cut to "Pro Mon…" (#590, US5 P3).
    FontMetrics {
        id: headerMetrics
        font.family: Kirigami.Theme.defaultFont.family
        font.pointSize: Kirigami.Theme.defaultFont.pointSize
        font.bold: true
    }

    // The width the figures of column `index` need, without cutting any
    // (principle 1: amounts are never elided), nor its header. The first
    // column holds names, which wrap, so it needs none. Headers are bold, so
    // they are measured in bold.
    function figureWidth(index: int): real {
        if (index === 0) {
            return 0;
        }
        let widest = 0;
        for (const row of rows) {
            if (row[index] !== undefined) {
                widest = Math.max(widest, metrics.advanceWidth(row[index]));
            }
        }
        // The total row is bold too (D-760).
        if (totalRow[index] !== undefined) {
            widest = Math.max(widest, headerMetrics.advanceWidth(totalRow[index]) * 1.05);
        }
        const title = columnTitle(columns[index]);
        // A little room for hinting, so the bold header is never cut.
        return Math.ceil(Math.max(widest, headerMetrics.advanceWidth(title) * 1.05)) + 2;
    }

    // The names column takes what the figure columns leave (a layout shares
    // the space in proportion to the preferred widths, so a figure column with
    // a larger preferred width than the names took it all, M6-B32). It never
    // gets narrower than a few letters, so a name wraps by words, not by
    // letter.
    function minimumColumnWidth(index: int): real {
        return index === 0 ? Kirigami.Units.gridUnit * 6 : figureWidth(index);
    }

    function preferredColumnWidth(index: int): real {
        return index === 0 ? Kirigami.Units.gridUnit * 6 : figureWidth(index);
    }

    function columnTitle(key): string {
        return columnNames[key] !== undefined ? columnNames[key] : key;
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        Repeater {
            model: table.columns

            Controls.Label {
                required property int index
                required property var modelData

                Layout.fillWidth: index === 0
                Layout.preferredWidth: table.preferredColumnWidth(index)
                Layout.minimumWidth: table.minimumColumnWidth(index)
                text: table.columnTitle(modelData)
                font.bold: true
                elide: Text.ElideRight
                Accessible.role: Accessible.ColumnHeader
                // The first column is names, the others are figures.
                horizontalAlignment: index === 0 ? Text.AlignLeft : Text.AlignRight
                textFormat: Text.PlainText
            }
        }
    }

    Kirigami.Separator {
        Layout.fillWidth: true
    }

    Repeater {
        model: table.rows

        RowLayout {
            id: row

            required property int index
            required property var modelData

            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: row.modelData

                AmountLabel {
                    required property int index
                    required property var modelData

                    Layout.fillWidth: index === 0
                    Layout.preferredWidth: table.preferredColumnWidth(index)
                    Layout.minimumWidth: table.minimumColumnWidth(index)
                    text: modelData
                    // A name wraps; it is never cut short (M6-B3).
                    wrapMode: index === 0 ? Text.Wrap : Text.NoWrap
                    elide: index === 0 ? Text.ElideNone : Text.ElideRight
                    Accessible.role: Accessible.Cell
                    horizontalAlignment: index === 0 ? Text.AlignLeft : Text.AlignRight
                    readonly property bool toned: table.tonedColumns.indexOf(index) >= 0
                    color: toned ? table.toneColor(table.tones[row.index] ?? "") : Kirigami.Theme.textColor
                    Accessible.name: toned && (table.notes[row.index] ?? "") !== ""
                        ? modelData + ", " + table.notes[row.index]
                        : modelData
                }
            }
        }
    }

    Kirigami.Separator {
        Layout.fillWidth: true
        visible: table.totalRow.length > 0
    }

    RowLayout {
        Layout.fillWidth: true
        visible: table.totalRow.length > 0
        spacing: Kirigami.Units.largeSpacing

        Repeater {
            model: table.totalRow

            AmountLabel {
                required property int index
                required property var modelData

                Layout.fillWidth: index === 0
                Layout.preferredWidth: table.preferredColumnWidth(index)
                Layout.minimumWidth: table.minimumColumnWidth(index)
                text: modelData
                font.bold: true
                wrapMode: index === 0 ? Text.Wrap : Text.NoWrap
                elide: index === 0 ? Text.ElideNone : Text.ElideRight
                Accessible.role: Accessible.Cell
                horizontalAlignment: index === 0 ? Text.AlignLeft : Text.AlignRight
                readonly property bool toned: table.tonedColumns.indexOf(index) >= 0
                color: toned ? table.toneColor(table.totalTone) : Kirigami.Theme.textColor
                Accessible.name: toned && table.totalNote !== "" ? modelData + ", " + table.totalNote : modelData
            }
        }
    }
}
