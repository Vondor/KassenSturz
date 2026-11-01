// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens FilterBar.qml, FilterPicker.qml and FilterNotes.qml on stand-ins for
// DashboardLayout and CategoryGroups (fakes/) and checks the filter bar
// (M6c.7): its three buttons say what is chosen, a choice reaches the layout
// as the call it names, Reset filters appears only for a choice made for now
// and goes back, a choice the book no longer has is listed so it can be taken
// out, and the notes under a component give the words for every note the
// report layer sends. `tools/check-panel.sh` runs it and fails on the first
// FAIL line or on any QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 700
    visible: true
    property bool failed: false
    property int step: 0

    DashboardLayout {
        id: dashboard
    }

    CategoryGroups {
        id: book
    }

    pageStack.initialPage: Kirigami.Page {
        id: page

        title: "Filters"

        ColumnLayout {
            width: page.width

            FilterBar {
                id: bar

                Layout.fillWidth: true
                layout: dashboard
                choices: book
            }

            FilterNotes {
                id: notes

                Layout.fillWidth: true
            }
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function last() {
        return dashboard.calls[dashboard.calls.length - 1];
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

    function buttons() {
        return findAll(bar, item => item.text !== undefined && item.clicked !== undefined && item.visible, []);
    }

    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            switch (window.step++) {
            case 0: {
                const texts = window.buttons().map(button => button.text);
                window.check(texts.length === 3, "period, accounts and categories, and no reset yet: " + texts);
                window.check(texts[0].includes("Period"), "the first button is the period: " + texts[0]);
                window.check(texts[1].includes("Accounts") && texts[1].includes("all"), "nothing chosen reads all: " + texts[1]);
                window.check(texts[2].includes("Categories"), "the third is the categories: " + texts[2]);
                // A choice goes to the layout by the call it names.
                bar.set("accounts", "a-cash");
                window.check(window.last() === "setFilter:accounts:a-cash", "a choice reaches the layout: " + window.last());
                break;
            }
            case 1: {
                const texts = window.buttons().map(button => button.text);
                window.check(texts.length === 4, "Reset filters appears for a choice made for now: " + texts);
                window.check(texts[1].includes("1"), "one account chosen reads so: " + texts[1]);
                window.check(dashboard.view.filters.forNow, "the choice is for now");
                bar.reset();
                window.check(window.last() === "resetFilters", "reset reaches the layout: " + window.last());
                break;
            }
            case 2: {
                window.check(window.buttons().length === 3, "Reset filters is gone again");
                // A choice the book doesn't have is still listed, to take out.
                bar.set("categories", "c-gone");
                break;
            }
            case 3: {
                const picker = window.findAll(bar, item => item.gone !== undefined, [])[1];
                window.check(picker !== undefined && picker.gone.length === 1 && picker.gone[0] === "c-gone", "a removed category is listed as gone: " + (picker ? picker.gone : "no picker"));
                bar.reset();
                // The notes: the words of every note the report layer sends.
                notes.filter = {
                    own: ["period", "categories"], filtered: ["accounts"], accounts: ["Cash box"], accountsMore: 2, categories: [], categoriesMore: 0,
                    notes: [{kind: "not-applied", dimension: "accounts", id: ""}, {kind: "gone", dimension: "categories", id: "c-gone"}, {kind: "chooses-nothing", dimension: "accounts", id: ""}],
                    nothing: true
                };
                break;
            }
            case 4: {
                window.check(notes.visible, "the notes are shown when there is something to say");
                const labels = window.findAll(notes, item => item.text !== undefined && item.visible, []).map(item => item.text);
                window.check(labels.length === 5, "own mark, filtered mark and three notes: " + labels.length + " " + JSON.stringify(labels));
                window.check(labels.some(text => text.includes("Own filter") && text.includes("period")), "the own mark names the dimensions");
                window.check(labels.some(text => text.includes("Cash box") && text.includes("2")), "the filtered mark names what is kept and how many more");
                window.check(labels.some(text => text.includes("Not filtered by account")), "the account note");
                window.check(labels.some(text => text.includes("no longer exists")), "the gone note");
                window.check(labels.some(text => text.includes("chooses nothing")), "the chooses-nothing note");
                notes.filter = undefined;
                break;
            }
            default: {
                window.check(!notes.visible, "no notes, no space");
                if (window.failed) {
                    console.log("FAIL: the filter smoke test");
                }
                Qt.exit(window.failed ? 1 : 0);
            }
            }
        }
    }
}
