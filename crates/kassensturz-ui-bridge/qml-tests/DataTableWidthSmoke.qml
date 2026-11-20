// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: comparison table stays inside a narrow tile
// check-panel-desktop: comparison table stays inside a narrow tile with the desktop style

// Shows the table of a spending comparison, with the long German column
// names, in a tile as wide as the Overview's at 125% text (B-427, #1299): no
// column may reach past the table's right edge and no header or figure may
// be cut. The first column and the headers wrap; the figures stay whole.
// `tools/check-panel.sh` runs it and fails on the first FAIL line.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 700
    height: 500
    visible: true
    property bool failed: false

    // A page makes its one child as wide as itself, so the tile is a layout
    // of the width to test inside it, as in the Overview.
    pageStack.initialPage: Kirigami.Page {
        ColumnLayout {
            id: tile

            width: 520

        DataTable {
            id: table

            Layout.fillWidth: true
            columns: ["category", "total", "average_3", "change", "change_percent"]
            columnNames: ({
                "category": "Kategorie",
                "total": "Gesamt",
                "average_3": "Durchschnitt der 3 Zeiträume davor",
                "change": "Veränderung",
                "change_percent": "Veränderung in %"
            })
            rows: [
                ["Lebensmittel und Haushaltswaren", "1.234,50 €", "1.100,25 €", "+134,25 €", "↑ 12,2 %"],
                ["Wohnen", "950,00 €", "950,00 €", "0,00 €", "0,0 %"]
            ]
            totalRow: ["Gesamt", "2.184,50 €", "2.050,25 €", "+134,25 €", "↑ 6,5 %"]
        }
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    // Every item under `item` that shows text.
    function texts(item, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child instanceof Controls.Label && child.text !== "") {
                out.push(child);
            }
            texts(child, out);
        }
        return out;
    }

    Timer {
        interval: 600
        running: true
        onTriggered: {
            const labels = window.texts(table, []);
            window.check(labels.length >= 20, "the headers, rows and total are found, got " + labels.length);
            for (const label of labels) {
                const right = label.mapToItem(table, label.width, 0).x;
                window.check(right <= table.width + 0.5, "'" + label.text + "' reaches " + right + " past the table's width " + table.width);
                window.check(!label.truncated, "'" + label.text + "' is cut");
            }
            // The total's figures and the last column are really there.
            const last = labels.find(label => label.text === "↑ 12,2 %");
            window.check(last !== undefined && last.width > 0, "the last column of a row is visible");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
