// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Rows of text under column names (M6a.5). It shows a `table` object of a
// chart as it is (columns, rows: all text), so it is also the screen-reader
// path for a chart (D-283). Nothing here adds up or sorts figures (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A table never makes its tile wider than the tile is: when its columns need
// more room than the tile has (a long German header, a larger text size), it
// scrolls sideways inside the tile (B-427, #1299), so a note or a chart next
// to it keeps the tile's width instead of the table's.
Flickable {
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

    // The table takes the width of its tile, and no more room than that
    // from it, however wide its columns would like to be.
    implicitWidth: 0
    implicitHeight: content.implicitHeight + (contentWidth > width ? horizontalBar.height : 0)
    contentWidth: Math.max(width, minimumRowWidth())
    contentHeight: content.implicitHeight
    flickableDirection: Flickable.HorizontalFlick
    boundsBehavior: Flickable.StopAtBounds
    clip: true

    Controls.ScrollBar.horizontal: Controls.ScrollBar {
        id: horizontalBar
        policy: table.contentWidth > table.width ? Controls.ScrollBar.AlwaysOn : Controls.ScrollBar.AlwaysOff
    }

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
    // (principle 1: amounts are never elided). The first column holds names,
    // which wrap, so it needs none.
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
        return Math.ceil(widest) + 2;
    }

    // The headers are bold, and a long one ("Durchschnitt der 3 Zeiträume
    // davor") wraps by words when the table is narrow, so a column is never
    // narrower than the longest word of its header; the room for the whole
    // header is only what it likes to have (B-427, #1299). A little extra
    // room for hinting, so the bold header is never cut.
    function headerWidth(index: int, whole: bool): real {
        const title = columnTitle(columns[index]);
        const parts = whole ? [title] : title.split(/\s+/);
        let widest = 0;
        for (const part of parts) {
            widest = Math.max(widest, headerMetrics.advanceWidth(part));
        }
        return Math.ceil(widest * 1.05) + 2;
    }

    // The names column takes what the figure columns leave (a layout shares
    // the space in proportion to the preferred widths, so a figure column with
    // a larger preferred width than the names took it all, M6-B32). It never
    // gets narrower than a few letters, so a name wraps by words, not by
    // letter. A figure column is at least as wide as its figures and the
    // longest word of its header, and prefers the whole header on one line.
    function minimumColumnWidth(index: int): real {
        return index === 0 ? Kirigami.Units.gridUnit * 6 : Math.max(figureWidth(index), headerWidth(index, false));
    }

    function preferredColumnWidth(index: int): real {
        return index === 0 ? Kirigami.Units.gridUnit * 6 : Math.max(figureWidth(index), headerWidth(index, true));
    }

    // The least width of a row: all its columns at their least, and the
    // gaps. A row is a layout in this layout, and a layout in a layout stays
    // as wide as its columns like to be, however narrow the table is, unless
    // it is told how narrow it may get: this lets the columns shrink to their
    // least and the table stay inside its tile (B-427).
    function minimumRowWidth(): real {
        let width = 0;
        for (let index = 0; index < columnCount; ++index) {
            width += minimumColumnWidth(index);
        }
        return width + Math.max(0, columnCount - 1) * Kirigami.Units.largeSpacing;
    }

    function columnTitle(key): string {
        return columnNames[key] !== undefined ? columnNames[key] : key;
    }

    ColumnLayout {
        id: content

        width: table.contentWidth
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.minimumWidth: table.minimumRowWidth()
            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: table.columns

                Controls.Label {
                    required property int index
                    required property var modelData

                    Layout.fillWidth: true
                    Layout.preferredWidth: table.preferredColumnWidth(index)
                    Layout.maximumWidth: index === 0 ? Number.POSITIVE_INFINITY : table.preferredColumnWidth(index)
                    Layout.minimumWidth: table.minimumColumnWidth(index)
                    text: table.columnTitle(modelData)
                    font.bold: true
                    // A header wraps by words, so a long one is never cut and
                    // the columns after it stay inside the table (B-427).
                    wrapMode: Text.Wrap
                    elide: Text.ElideNone
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
                Layout.minimumWidth: table.minimumRowWidth()
                spacing: Kirigami.Units.largeSpacing

                Repeater {
                    model: row.modelData

                    AmountLabel {
                        required property int index
                        required property var modelData

                        Layout.fillWidth: true
                        Layout.preferredWidth: table.preferredColumnWidth(index)
                        Layout.maximumWidth: index === 0 ? Number.POSITIVE_INFINITY : table.preferredColumnWidth(index)
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
            Layout.minimumWidth: table.minimumRowWidth()
            visible: table.totalRow.length > 0
            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: table.totalRow

                AmountLabel {
                    required property int index
                    required property var modelData

                    Layout.fillWidth: true
                    Layout.preferredWidth: table.preferredColumnWidth(index)
                    Layout.maximumWidth: index === 0 ? Number.POSITIVE_INFINITY : table.preferredColumnWidth(index)
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
}
