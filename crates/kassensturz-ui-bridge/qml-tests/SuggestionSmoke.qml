// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: chat suggestion stays visible and editable

// Loads AiChatPanel.qml with the stand-ins and checks what *Describe a
// chart* does to the question field (B-386, D-859): into an empty field the
// example goes as editable text with the note "Suggestion, you can change
// it" under it, the cursor at its end and nothing selected; once the user
// changes it the note goes; text the user typed is never replaced or added
// to, the example is offered next to it instead, and only "Use it instead"
// puts it into the field. What is sent is exactly what the field shows.
// The chart button follows the permission while the chat is open (B-402).
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 500
    height: 700
    visible: true
    property bool failed: false

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    // The item of a type with this text, under `item`.
    function find(item, test) {
        if (item === null || item === undefined) {
            return null;
        }
        if (test(item)) {
            return item;
        }
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            const found = find(children[i], test);
            if (found !== null) {
                return found;
            }
        }
        return null;
    }

    AiSwitch {
        id: aiSwitch
    }

    AiChat {
        id: chat
    }

    AiSettings {
        id: settings
    }

    pageStack.initialPage: Kirigami.Page {
        AiChatPanel {
            id: panel

            anchors.fill: parent
            ai: aiSwitch
            chat: chat
            settings: settings
        }
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            const field = window.find(panel, item => item.Accessible.name === "Your question for the AI" && item.cursorPosition !== undefined);
            const note = window.find(panel, item => item.text === "Suggestion, you can change it");
            window.check(field !== null && note !== null, "the field and the note are there");
            if (field === null || note === null) {
                Qt.exit(1);
                return;
            }
            window.check(!note.visible, "no note while the field is empty");

            // Into an empty field: the example, editable, with the note.
            panel.describeChart();
            const example = field.text;
            window.check(example !== "" && panel.suggested && note.visible, "the example is in the field with the note: '" + example + "'");
            window.check(field.selectedText === "" && field.cursorPosition === example.length, "nothing is selected, the cursor is at the end");
            window.check(field.font.italic, "the example is set apart");

            // The user changes it: it is theirs now.
            field.insert(field.cursorPosition, ", please");
            window.check(!panel.suggested && !note.visible && !field.font.italic, "a changed example is the user's own text");
            panel.ask();
            window.check(chat.asked[chat.asked.length - 1] === example + ", please", "what is sent is what the field showed: " + chat.asked[chat.asked.length - 1]);
            window.check(field.text === "" && !note.visible, "after asking the field is empty");

            // The user typed first: their text stays as it is.
            field.text = "Make me a chart";
            panel.describeChart();
            window.check(field.text === "Make me a chart", "the user's text is not replaced or added to: '" + field.text + "'");
            window.check(panel.offered === example && !note.visible, "the example is offered next to it");
            const use = window.find(panel, item => item.text === "Use it instead" && item.clicked !== undefined);
            window.check(use !== null && use.visible, "with a button to use it instead");
            if (use !== null) {
                use.clicked();
            }
            window.check(field.text === example && panel.suggested && note.visible && panel.offered === "", "only that button puts the example in");

            // Pressed again on an unchanged example, it stays one example.
            panel.describeChart();
            window.check(field.text === example && panel.suggested, "pressed again, still one example");

            // The chart permission switched on while the chat stays open
            // shows the button at once, and off hides it (B-402).
            const describe = window.find(panel, item => item.text === "Describe a chart" && item.clicked !== undefined);
            window.check(describe !== null && !describe.visible, "no chart button without the permission");
            settings.setAllows("AllowNewViews", true);
            window.check(describe !== null && describe.visible, "the chart button shows once allowed, without reopening");
            settings.setAllows("AllowNewViews", false);
            window.check(describe !== null && !describe.visible, "and goes once taken away");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
