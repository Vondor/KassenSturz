// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Loads PropertiesPanel.qml (M6a.6) on a stand-in for ComponentProperties and
// checks what a user sees and what the panel passes on: one control for every
// field of `view.fields`, none for anything else, and every change goes to the
// model as the call it names. `tools/check-panel.sh` runs it; it exits 1 and
// prints why when something is wrong.

import QtQuick
import QtQuick.Controls as Controls

Window {
    id: window

    width: 480
    height: 480
    visible: true

    property var calls: []
    property bool failed: false

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    // The shape ComponentProperties.view has (see properties_model.rs).
    QtObject {
        id: fake

        property var view: ({
            type: "chart.pie",
            dataset: "spending_by_category",
            datasets: ["spending_by_category"],
            json: "{}",
            fields: [
                {key: "grouping", place: "params", kind: "choice", text: "", flag: false,
                 word: "category", words: ["category", "top_level_category"]},
                {key: "groups", place: "params", kind: "ids", text: "", flag: false, word: "", words: [], ids: ["g1"]},
                {key: "title", place: "props", kind: "text", text: "Where it went", flag: false, word: "", words: []},
                {key: "legend", place: "props", kind: "flag", text: "", flag: true, word: "", words: []}
            ]
        })

        function setDataset(id) { window.calls.push("setDataset:" + id); }
        function setText(key, value) { window.calls.push("setText:" + key + ":" + value); }
        function setFlag(key, value) { window.calls.push("setFlag:" + key + ":" + value); }
        function setWord(key, value) { window.calls.push("setWord:" + key + ":" + value); }
    }

    PropertiesPanel {
        id: panel

        anchors.fill: parent
        model: fake
        choices: QtObject {
            property var view: ({
                groups: [{id: "g1", name: "Fixed", income: false}, {id: "g2", name: "Pay", income: true}],
                categories: [{id: "c1", name: "Rent", path: "Home > Rent", income: false}]
            })
        }
    }

    function find(item, type, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child.toString().startsWith(type)) {
                out.push(child);
            }
            find(child, type, out);
        }
        return out;
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            const boxes = window.find(panel, "ComboBox", []);
            const texts = window.find(panel, "TextField", []).filter(field => field.maximumLength === 200);
            const flags = window.find(panel, "CheckBox", []).filter(box => box.text === "Show the legend");
            // One choice (the group-by), one title, one legend. The dataset box
            // is hidden: this chart can show only one dataset.
            const shown = boxes.filter(box => box.visible);
            window.check(shown.length === 1, "one choice box, got " + shown.length);
            window.check(texts.length === 1 && texts[0].text === "Where it went", "the title field");
            window.check(flags.length === 1 && flags[0].checked, "the legend box is ticked");
            window.check(shown[0].count === 2 && shown[0].currentIndex === 0, "two words, the first chosen");

            // The groups of money spent can be ticked; the series also take
            // categories and groups of money received.
            const groups = panel.idOptions("groups");
            window.check(groups.length === 1 && groups[0].id === "g1", "only the groups of money spent: " + JSON.stringify(groups));
            const series = panel.idOptions("series");
            window.check(series.length === 3 && series[2].id === "c1" && series[2].name === "Home > Rent", "groups then categories: " + JSON.stringify(series));

            shown[0].activated(1);
            texts[0].text = "New title";
            texts[0].editingFinished();
            flags[0].toggled();
            const expected = ["setWord:grouping:top_level_category", "setText:title:New title", "setFlag:legend:true"];
            window.check(JSON.stringify(window.calls) === JSON.stringify(expected), "calls: " + JSON.stringify(window.calls));

            // A refused change shows a message and nothing else.
            fake.view = Object.assign({}, fake.view, {error: {code: "report.document-invalid", reason: "bad-choice", path: "params.grouping"}});
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
