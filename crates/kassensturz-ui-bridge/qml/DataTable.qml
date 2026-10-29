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

    readonly property int columnCount: columns.length

    spacing: 0
    Accessible.role: Accessible.Table
    Accessible.name: title

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

                Layout.fillWidth: true
                Layout.preferredWidth: 1
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

            required property var modelData

            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: row.modelData

                AmountLabel {
                    required property int index
                    required property var modelData

                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    text: modelData
                    // A name wraps; it is never cut short (M6-B3).
                    wrapMode: index === 0 ? Text.Wrap : Text.NoWrap
                    elide: index === 0 ? Text.ElideNone : Text.ElideRight
                    Accessible.role: Accessible.Cell
                    horizontalAlignment: index === 0 ? Text.AlignLeft : Text.AlignRight
                }
            }
        }
    }
}
