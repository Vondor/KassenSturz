// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Loads PropertiesPanel.qml (M6a.6) on a stand-in for ComponentProperties and
// checks what a user sees and what the panel passes on: one control for every
// field of `view.fields`, none for anything else, and every change goes to the
// model as the call it names. A formula field (M6b.7) is checked too: the
// result and what it counted, a mistake with its place marked, the names
// that complete and the list of functions. `tools/check-panel.sh` runs it
// as part of the app's module; it exits 1 and prints why when something is
// wrong.

import QtQuick
import QtQuick.Controls as Controls
import io.github.vondor.kassensturz

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
                {key: "legend", place: "props", kind: "flag", text: "", flag: true, word: "", words: []},
                {key: "formula", place: "params", kind: "formula", text: "=Pets - Travel", flag: false, word: "", words: []}
            ]
        })

        function setDataset(id) { window.calls.push("setDataset:" + id); }
        function setText(key, value) { window.calls.push("setText:" + key + ":" + value); }
        function setFlag(key, value) { window.calls.push("setFlag:" + key + ":" + value); }
        function setWord(key, value) { window.calls.push("setWord:" + key + ":" + value); }
        function setFormula(key, value) { window.calls.push("setFormula:" + key + ":" + value); }
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

    // A card and a bar that show the result of a formula (M6b.7).
    ComponentPreview {
        id: cardPreview

        visible: false
        subject: ({preview: {shows: "card", usesFormula: true, formula: "=Pets - Travel", target: "", compare: "previous_period", measure: "formula"}, fields: []})
        period: "last-month"
    }

    ComponentPreview {
        id: barPreview

        visible: false
        subject: ({preview: {shows: "progress", usesFormula: true, formula: "=Pets", target: "=400", compare: "previous_period", measure: "savings-rate"}, fields: []})
        period: "last-month"
    }

    ComponentPreview {
        id: brokenPreview

        visible: false
        subject: ({preview: {shows: "card", usesFormula: true, formula: "=Nothing", target: "", compare: "previous_period", measure: "formula"}, fields: []})
        period: "last-month"
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
            // (The formula field's list of functions is a box of its own.)
            const shown = boxes.filter(box => box.visible && box.textRole === "name");
            window.check(shown.length === 1, "one choice box, got " + shown.length);
            window.check(texts.length === 1 && texts[0].text === "Where it went", "the title field");
            window.check(flags.length === 1 && flags[0].checked, "the legend box is ticked");
            window.check(shown[0].count === 2 && shown[0].currentIndex === 0, "two words, the first chosen");

            // The groups of money spent can be ticked; the series also take
            // categories and groups of money received.
            const groups = panel.idOptions("groups");
            window.check(groups.length === 1 && groups[0].id === "g1", "only the groups of money spent: " + JSON.stringify(groups));
            // Limiting a chart to categories offers the groups and the
            // categories of money spent, not the ones of money received.
            const only = panel.idOptions("categories");
            window.check(only.length === 2 && only[0].id === "g1" && only[1].id === "c1", "spending only: " + JSON.stringify(only));
            const series = panel.idOptions("series");
            window.check(series.length === 3 && series[2].id === "c1" && series[2].name === "Home > Rent", "groups then categories: " + JSON.stringify(series));

            shown[0].activated(1);
            texts[0].text = "New title";
            texts[0].editingFinished();
            flags[0].toggled();
            const expected = ["setWord:grouping:top_level_category", "setText:title:New title", "setFlag:legend:true"];
            window.check(JSON.stringify(window.calls) === JSON.stringify(expected), "calls: " + JSON.stringify(window.calls));

            // The words for a title: four to pick, and they are worked out for
            // the period, in the language of the app.
            const wordButtons = window.find(panel, "Button", []).filter(button => typeof button.text === "string" && button.text.startsWith("$") && button.text.length > 1);
            window.check(wordButtons.length === 4, "four title words to pick, got " + wordButtons.length);
            const october = Qt.locale().monthName(9, Locale.LongFormat);
            window.check(panel.titleWords.resolve("Spending in $month", "2026-10") === "Spending in " + october, "month word: " + panel.titleWords.resolve("Spending in $month", "2026-10"));
            window.check(panel.titleWords.resolve("$period", "2026-10") === october + " 2026", "period word");
            window.check(panel.titleWords.resolve("$year", "last-year") === String(new Date().getFullYear() - 1), "year word");
            window.check(panel.titleWords.resolve("Plain $months title", "2026-10") === "Plain $months title", "other text stays as typed");

            // The formula field (M6b.7): the formula as it was kept, the result
            // with what it counted, and every change passed on as one call.
            const formulas = window.find(panel, "FormulaField", []);
            window.check(formulas.length === 1, "one formula field, got " + formulas.length);
            const formula = formulas[0];
            const input = window.find(formula, "TextField", [])[0];
            window.check(input.text === "=Pets - Travel", "the formula as kept: " + input.text);
            formula.check();
            window.check(formula.result !== undefined && formula.result.value === "312,40 €", "the result: " + JSON.stringify(formula.result));
            const counted = formula.counts(formula.result.counted);
            window.check(counted === "Pets (with Vet, Cat food and 2 more), Travel in September 2026", "what it counted: " + counted);
            window.check(formula.signature({name: "IF", arguments: ["test", "yes", "no"]}) === "IF(test; yes; no)", "the signature in the user's form");
            window.check(formula.help("PERCENT_OF") !== "" && formula.help("SUM") !== "", "a plain line for each function");

            // Typing a name offers the names that fit; taking one writes it
            // where the word was and keeps the focus in the field.
            input.text = "=Pe";
            input.cursorPosition = 3;
            input.textEdited();
            window.check(formula.completion.names.length === 2, "two names fit: " + JSON.stringify(formula.completion));
            formula.chosen = 1;
            formula.insertName(formula.completion.names[1]);
            window.check(input.text === "='Pet food'" && input.cursorPosition === 11, "the name was written: " + input.text);
            input.text = "=Nothing";
            formula.check();
            window.check(formula.mistake !== undefined && formula.mistake.before === "=" && formula.mistake.after === "Nothing", "the place of the mistake is marked");
            const messages = window.find(formula, "FormError", []).filter(message => message.visible);
            window.check(messages.length === 1 && messages[0].text.indexOf("Nothing is not a category or one of My groups (character 2)") === 0, "the mistake in words: " + (messages.length > 0 ? messages[0].text : "none"));
            input.text = "=PERCENT_OF(Pets; 300)";
            input.editingFinished();
            window.check(window.calls[window.calls.length - 1] === "setFormula:formula:=PERCENT_OF(Pets; 300)", "the formula is passed on: " + JSON.stringify(window.calls));
            formula.insertFunction("SUM");
            window.check(input.text.indexOf("SUM()") >= 0, "a function was written: " + input.text);

            // The card shows the formula's figure like a key figure; the bar fills
            // by the figure over its target; a mistake takes the figure's place.
            const cardLabels = window.find(cardPreview, "Label", []).map(label => label.text);
            window.check(cardLabels.indexOf("312,40 €") >= 0 && cardLabels.indexOf("Your formula") >= 0, "the card shows the figure: " + JSON.stringify(cardLabels));
            const meters = window.find(barPreview, "ProgressMeter", []);
            window.check(meters.length === 1 && meters[0].permille === 780 && meters[0].valueText === "312,40 € of 400,00 €", "the bar: " + (meters.length > 0 ? meters[0].permille + " " + meters[0].valueText : "none"));
            const broken = window.find(brokenPreview, "FormError", []).filter(message => message.result.code !== undefined);
            window.check(broken.length === 1 && broken[0].result.code === "report.formula-unknown-name" && broken[0].messageFor(broken[0].result).indexOf("Nothing is not a category") === 0, "the mistake takes the place of the figure");
            window.check(window.find(brokenPreview, "KpiCard", []).length === 0, "no card for a formula with a mistake");

            // A refused change shows a message and nothing else.
            fake.view = Object.assign({}, fake.view, {error: {code: "report.document-invalid", reason: "bad-choice", path: "params.grouping"}});
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
