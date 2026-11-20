// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: comparison table stays inside a narrow tile
// check-panel-desktop: comparison table stays inside a narrow tile with the desktop style

// Shows the table of a spending comparison, with the long German column
// names and a note over it, in tiles as wide as the Overview's at 125% text
// and far narrower (B-427, #1299). The table must never make its tile or the
// note over it wider than the tile: when its columns need more room than
// the tile has, it scrolls sideways inside the tile. No header or figure may
// be cut. `tools/check-panel.sh` runs it and fails on the first FAIL line.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 700
    height: 900
    visible: true
    property bool failed: false

    // A page makes its one child as wide as itself, so each tile sits in a
    // layout of the width to test, as in the Overview.
    pageStack.initialPage: Kirigami.Page {
        ColumnLayout {
            Repeater {
                model: [520, 300]

                ColumnLayout {
                    id: holder

                    required property int modelData
                    property alias note: note
                    property alias table: table

                    Layout.preferredWidth: modelData
                    Layout.maximumWidth: modelData

                    // The tile, which fills what the holder gives it.
                    ColumnLayout {
                        Layout.fillWidth: true

                        Controls.Label {
                            id: note

                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: "3 der 3 Zeiträume, deren Durchschnitt dieser Vergleich bildet, liegen vor der ersten Buchung und zählen als null."
                        }

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
            const holders = [];
            for (const item of window.pageStack.initialPage.contentItem.children[0].children) {
                if (item.table !== undefined) {
                    holders.push(item);
                }
            }
            window.check(holders.length === 2, "both tiles are found, got " + holders.length);
            for (const holder of holders) {
                const room = holder.modelData;
                const table = holder.table;
                const where = "in a tile of " + room + ": ";
                window.check(holder.width <= room + 0.5, where + "the tile is " + holder.width + " wide");
                window.check(table.width <= room + 0.5, where + "the table is " + table.width + " wide");
                window.check(holder.note.width <= room + 0.5, where + "the note over it is " + holder.note.width + " wide");
                const labels = window.texts(table, []);
                window.check(labels.length >= 20, where + "the headers, rows and total are found, got " + labels.length);
                for (const label of labels) {
                    window.check(!label.truncated, where + "'" + label.text + "' is cut");
                }
                // A table wider than its tile scrolls; one that fits doesn't.
                const scrolls = table.contentWidth > table.width + 0.5;
                if (!scrolls) {
                    for (const label of labels) {
                        const right = label.mapToItem(table, label.width, 0).x;
                        window.check(right <= table.width + 0.5, where + "'" + label.text + "' reaches " + right + " past the table's width " + table.width);
                    }
                }
                if (room < 400) {
                    window.check(scrolls, where + "a narrow table scrolls sideways");
                }
            }
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
