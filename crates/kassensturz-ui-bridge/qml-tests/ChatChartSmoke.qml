// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens AiChartCard.qml, the chart preview in the chat (M6c.9, D-355, D-402),
// on stand-ins for the Rust objects (fakes/) and checks that the card shows
// the AI-made label, the title as text, a preview and a warning for a number
// in the title that no function produced; that nothing is confirmed before
// *Keep*; that *Keep* confirms the set once and says so; that *Change…*
// throws the set away and hands the title, the component and the period on;
// and that *Leave out* throws it away without handing anything on.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 640
    height: 900
    visible: true
    property bool failed: false
    property var handedOn: []
    property var outcomes: []
    property var added: []

    AiSwitch {
        id: ai
    }

    AiChat {
        id: chat
    }

    pageStack.initialPage: Kirigami.Page {
        ColumnLayout {
            anchors.fill: parent

            AiChartCard {
                id: keeping

                Layout.fillWidth: true
                ai: ai
                chat: chat
                setId: "7"
                unverified: ["12345.67"]
                onAddToOverviewClicked: (component, period) => window.added.push([component, period])
                onHandled: outcome => window.outcomes.push("keep:" + outcome)
            }

            AiChartCard {
                id: changing

                Layout.fillWidth: true
                ai: ai
                chat: chat
                setId: "8"
                onChangeClicked: (title, component, period) => window.handedOn.push([title, component, period])
                onHandled: outcome => window.outcomes.push("change:" + outcome)
            }

            AiChartCard {
                id: leaving

                Layout.fillWidth: true
                ai: ai
                chat: chat
                setId: "9"
                onHandled: outcome => window.outcomes.push("leave:" + outcome)
            }

            // A message that is not a chart, and one made again for a chart
            // that was dealt with: neither asks the book for anything (the
            // chat hides the first, which is not a chart).
            AiChartCard {
                id: other

                Layout.fillWidth: true
                ai: ai
                chat: chat
            }

            AiChartCard {
                id: again

                Layout.fillWidth: true
                ai: ai
                chat: chat
                setId: "10"
                outcome: "kept"
            }
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function find(item, type, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child.toString().startsWith(type + "_") || child.toString().startsWith(type + "(")) {
                out.push(child);
            }
            find(child, type, out);
        }
        return out;
    }

    // The text of every visible item below `item` that has one.
    function texts(item, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child.visible && typeof child.text === "string") {
                out.push(child.text);
            }
            texts(child, out);
        }
        return out;
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            const all = window.texts(keeping, []);
            window.check(all.includes("Chart prepared by the AI"), "the AI-made label is shown");
            window.check(all.includes("Where the money goes"), "the title is shown");
            window.check(all.includes("This year"), "the period is shown in words");
            window.check(window.find(keeping, "ComponentPreview", []).length === 1, "one preview");
            window.check(all.some(text => text.includes("12345.67")), "a number in the title that no function produced is named");
            window.check(!window.texts(changing, []).some(text => text.includes("Don't rely")), "and only for the card that has one");
            window.check(all.includes("Keep") && all.includes("Change…") && all.includes("Leave out"), "the three buttons");
            window.check(chat.confirmed.length === 0 && chat.discarded.length === 0, "nothing happens before a button is pressed");
            window.check(other.chart === null && other.outcome === "", "a message that is not a chart loads nothing");
            window.check(!window.texts(again, []).includes("Where the money goes"), "a card made again for a dealt-with chart loads nothing");

            keeping.keep();
            window.check(chat.confirmed.length === 1 && chat.confirmed[0] === "7", "Keep confirms the set");
            window.check(chat.discarded.length === 0, "and throws nothing away");
            window.check(keeping.outcome === "kept", "and says it was kept");
            window.check(window.texts(keeping, []).some(text => text.startsWith("Kept.")), "in words");
            window.check(!window.texts(keeping, []).includes("Keep"), "the buttons are gone");

            window.check(window.texts(keeping, []).includes("Add to Overview"), "after Keep the card offers Add to Overview");
            window.check(window.added.length === 0, "and has put nothing on the Overview yet");
            keeping.addToOverview();
            window.check(window.added.length === 1 && window.added[0][1] === "this-year", "Add to Overview hands the period on");
            window.check(JSON.parse(window.added[0][0]).dataset === "spending_by_category", "and the component");
            window.check(keeping.outcome === "added" && window.texts(keeping, []).some(text => text.startsWith("Kept and added")), "and says so");
            window.check(!window.texts(keeping, []).includes("Add to Overview"), "the button is gone");
            keeping.addToOverview();
            window.check(window.added.length === 1, "pressed again it does nothing");
            window.check(!window.texts(changing, []).includes("Add to Overview") && !window.texts(leaving, []).includes("Add to Overview"), "a chart not kept has no such button");

            changing.change();
            window.check(chat.discarded.includes("8"), "Change… throws the AI's set away");
            window.check(chat.confirmed.length === 1, "and does not keep it");
            window.check(window.handedOn.length === 1 && window.handedOn[0][0] === "Where the money goes" && window.handedOn[0][2] === "this-year", "and hands the title and period on");
            window.check(JSON.parse(window.handedOn[0][1]).dataset === "spending_by_category", "and the component");

            leaving.leaveOut();
            window.check(chat.discarded.includes("9") && window.handedOn.length === 1, "Leave out throws the set away and hands nothing on");

            window.check(window.outcomes.join(",") === "keep:kept,keep:added,change:builder,leave:left", "each card tells what became of its chart: " + window.outcomes.join(","));
            console.log(window.failed ? "chat chart: FAILED" : "chat chart: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
