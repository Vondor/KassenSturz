// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Loads SpecPage.qml, the description field of a custom chart (M6e.3), on
// stand-ins for ChartDescription and ComponentProperties and checks what a
// user sees and what the page passes on: the check runs on the text a moment
// after typing, only "Use this description" stores it, a refused text shows
// why and cannot be used, and the parts that are not supported are listed
// with "Draw it without these". `tools/check-panel.sh` runs it as part of the
// app's module and fails on the first FAIL line or on any QML warning about
// a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import io.github.vondor.kassensturz

Window {
    id: window

    width: 480
    height: 800
    visible: true
    property bool failed: false
    property int step: 0
    property var calls: []

    QtObject {
        id: fake

        function setText(key, value) { window.calls.push("setText:" + key + ":" + value); }
        function setFlag(key, value) { window.calls.push("setFlag:" + key + ":" + value); }
    }

    SpecPage {
        id: page

        width: parent.width
        model: fake
        stored: "{\"data\": {\"name\": \"income_expense_by_month\"}}"
        partial: false
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function findAll(item, predicate, found) {
        if (predicate(item)) {
            found.push(item);
        }
        for (const child of item.data) {
            if (child && child.data !== undefined) {
                findAll(child, predicate, found);
            }
        }
        return found;
    }

    // The stand-in the page made for itself.
    function description() {
        return page.data.filter(o => o && o.checked !== undefined && o.answer !== undefined)[0];
    }

    function buttons() {
        return findAll(page, item => item.text !== undefined && item.clicked !== undefined && item.visible, []);
    }

    function button(text) {
        return buttons().filter(b => b.text === text)[0];
    }

    function area() {
        return findAll(page, item => item.placeholderText !== undefined, [])[0];
    }

    Timer {
        interval: 700
        running: true
        repeat: true
        onTriggered: {
            const use = "Use this description";
            switch (window.step++) {
            case 0: {
                window.check(window.description() !== undefined, "the page has its description object");
                window.check(window.area().text === page.stored, "the field starts with the stored text");
                window.check(window.button(use) !== undefined, "there is a Use button");
                window.check(window.button(use).enabled === false, "Use is off while nothing changed");
                window.check(window.button("Open a chart description…") !== undefined, "a file can be opened");
                window.check(window.description().checked[0] === page.stored, "the stored text was checked at once");
                // A new text is checked after a pause and stored only on the button.
                window.area().text = "{\"mark\": \"bar\"}";
                break;
            }
            case 1: {
                const asked = window.description().checked;
                window.check(asked[asked.length - 1] === "{\"mark\": \"bar\"}", "the typed text was checked: " + asked.length);
                window.check(window.calls.length === 0, "typing stores nothing: " + window.calls);
                window.check(window.button(use).enabled === true, "Use is on for a changed text that can be kept");
                window.button(use).clicked();
                window.check(window.calls[0] === "setText:spec:{\"mark\": \"bar\"}", "Use stores the text: " + window.calls);
                // The text is refused: nothing to use, and it says why.
                window.description().answer = ({ok: false, dataset: "", canDraw: false, count: 0, unsupported: [], text: "", opened: 0,
                    error: {code: "report.spec-invalid", reason: "syntax", path: ""}});
                window.area().text = "{";
                break;
            }
            case 2: {
                window.check(window.button(use).enabled === false, "Use is off for a text that was refused");
                // Parts that are not supported are listed, with the choice.
                window.description().answer = ({ok: true, dataset: "income_expense_by_month", canDraw: true, count: 2, text: "", opened: 0,
                    unsupported: [{path: "transform[0].calculate", reason: "expression"}, {path: "data.url", reason: "data-source"}]});
                page.stored = "{\"mark\": \"bar\"}";
                break;
            }
            case 3: {
                const texts = window.findAll(page, item => item.text !== undefined && item.visible && item.clicked === undefined, []).map(item => item.text);
                window.check(texts.includes("transform[0].calculate"), "the path is listed: " + texts);
                window.check(texts.includes("data.url"), "the second path is listed");
                window.check(texts.some(t => t.includes("Calculations")), "the reason is in words: " + texts);
                window.check(texts.some(t => t.includes("not drawn until")), "it says the chart waits for the choice");
                window.calls = [];
                window.button("Draw it without these").clicked();
                window.check(window.calls[0] === "setFlag:partial:true", "Draw it without these sets the flag: " + window.calls);
                page.partial = true;
                break;
            }
            case 4: {
                window.check(window.button("Draw it without these") === undefined, "the choice is gone once made");
                window.check(window.button("Don't leave them out") !== undefined, "it can be taken back");
                window.calls = [];
                window.button("Don't leave them out").clicked();
                window.check(window.calls[0] === "setFlag:partial:false", "taking it back clears the flag: " + window.calls);
                // A file fills the field, so the user sees what was read.
                window.description().openFile("file:///tmp/example.json");
                break;
            }
            case 5: {
                window.check(window.area().text === "from the file", "a file fills the field: " + window.area().text);
                window.check(window.calls.length === 1, "reading a file stores nothing: " + window.calls);
                Qt.exit(window.failed ? 1 : 0);
                break;
            }
            }
        }
    }
}
