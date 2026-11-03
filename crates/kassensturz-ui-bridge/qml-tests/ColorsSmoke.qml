// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Loads ChartColorsCard.qml, the chart colors in Settings (M7a.10, D-589), on
// a stand-in for ChartPaletteSettings (fakes/) and checks what a person sees
// and what the card asks of the object: eight rows with the colors, a typed
// `#rrggbb` or three numbers reach `setColor`, a text that is no color and a
// part above 255 are said in words, a warning is shown at the place it is
// about (and a neighbour warning at both), "Standard" and "Back to standard
// for all" put colors back, and the card never keeps a number of its own.
// `tools/check-panel.sh` runs it as part of the app's module and fails on
// the first FAIL line or on any QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import io.github.vondor.kassensturz

Window {
    id: window

    width: 640
    height: 900
    visible: true
    property bool failed: false
    property int step: 0

    ChartPaletteSettings {
        id: fake
    }

    ChartColorsCard {
        id: card

        width: parent.width
        chartColors: fake
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
        for (const child of item.children ?? []) {
            findAll(child, predicate, found);
        }
        return found;
    }

    function fields() {
        return findAll(card, item => item.maximumLength !== undefined && item.editingFinished !== undefined, []);
    }

    function shown() {
        return findAll(card, item => item.text !== undefined && item.visible && item.clicked === undefined, []).map(item => item.text);
    }

    function button(text, which) {
        return findAll(card, item => item.clicked !== undefined && item.text === text, [])[which ?? 0];
    }

    function type(field, text) {
        field.text = text;
        field.editingFinished();
    }

    Timer {
        interval: 200
        running: true
        repeat: true
        onTriggered: {
            window.step += 1;
            switch (window.step) {
            case 1: {
                const fields = window.fields();
                window.check(fields.length === 8, "a row for each of the eight places: " + fields.length);
                window.check(fields[0].text === "#0072B2" && fields[7].text === "#999999", "the fields show the colors: " + fields[0].text + " " + fields[7].text);
                const texts = window.shown();
                window.check(texts.includes("1. Blue") && texts.includes("8. Grey"), "the places are named: " + texts);
                window.check(window.button("Back to standard for all").enabled === false, "nothing to put back at the start");
                window.check(window.button("Standard", 0).enabled === false, "no color is changed at the start");
                // A color as #rrggbb in any case.
                window.type(fields[0], "#1a2B3c");
                break;
            }
            case 2: {
                window.check(fake.calls.includes("setColor:0:#1a2B3c"), "the typed code reached the object: " + fake.calls);
                window.check(fake.colors[0] === "#1A2B3C", "the object holds the color as text: " + fake.colors[0]);
                window.check(window.fields()[0].text === "#1A2B3C", "the field shows the color as the object wrote it");
                window.check(window.button("Back to standard for all").enabled === true, "a changed palette can be put back");
                window.check(window.button("Standard", 0).enabled === true, "a changed color can be put back");
                window.check(window.shown().includes("1. Own color") && !window.shown().includes("1. Blue"), "a changed color is no longer called by the standard name: " + window.shown());
                // Three numbers.
                window.type(window.fields()[1], "0, 114, 178");
                break;
            }
            case 3: {
                window.check(fake.colors[1] === "#0072B2", "three numbers make the color: " + fake.colors[1]);
                // Not a color, and a part above 255: said in words, nothing changes.
                window.type(window.fields()[2], "nonsense");
                break;
            }
            case 4: {
                window.check(fake.colors[2] === "#009E73", "text that is no color changes nothing");
                window.check(window.shown().some(t => t.includes("That is not a color")), "it says it is no color: " + window.shown());
                window.check(window.fields()[2].text === "nonsense", "the typed text stays to be corrected");
                window.type(window.fields()[3], "256, 0, 0");
                break;
            }
            case 5: {
                window.check(window.shown().some(t => t.includes("0 to 255")), "it says what a part may be");
                // The text goes away once a fine color is typed.
                window.type(window.fields()[2], "#00ff00");
                window.type(window.fields()[3], "#CC79A8");
                break;
            }
            case 6: {
                window.check(!window.shown().some(t => t.includes("That is not a color") || t.includes("0 to 255")), "the words go with the problem");
                // Warnings: one color hard to see, two neighbours alike.
                fake.warningList = [{kind: "contrast", slot: 0, other: -1}, {kind: "alike", slot: 4, other: 5}];
                window.type(window.fields()[7], "#888888");
                break;
            }
            case 7: {
                const texts = window.shown();
                window.check(texts.filter(t => t.includes("hard to see against the background")).length === 1, "the contrast warning is shown once: " + texts);
                window.check(texts.some(t => t.includes("very much like the next one (Sky blue)")), "the warning names the next color: " + texts);
                window.check(texts.some(t => t.includes("very much like the one before it (Orange)")), "the neighbour is told as well: " + texts);
                window.check(fake.calls.some(c => c.startsWith("warnings:#")), "the background goes over as #rrggbb text: " + fake.calls.filter(c => c.startsWith("warnings")));
                window.button("Standard", 0).clicked();
                break;
            }
            case 8: {
                window.check(fake.colors[0] === "#0072B2" && fake.calls.includes("reset:0"), "Standard puts one color back: " + fake.colors[0]);
                window.check(window.shown().includes("1. Blue"), "the standard name is back with the standard color: " + window.shown());
                window.button("Back to standard for all").clicked();
                break;
            }
            case 9: {
                window.check(fake.calls.includes("resetAll"), "Back to standard for all reached the object");
                window.check(fake.colors.every((color, at) => color === fake.standard[at]), "every color is the standard one again");
                window.check(window.fields()[3].text === "#CC79A7", "the fields follow: " + window.fields()[3].text);
                Qt.exit(window.failed ? 1 : 0);
                break;
            }
            }
        }
    }
}
