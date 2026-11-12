// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: status icon

// Opens StatusIcon.qml with TileStatusWords.qml (US5.4, D-734, D-736) and
// checks that each state has words for the screen reader and a hover
// sentence, that an unknown word shows as "unknown", that a click asks for
// the whole tile and that while arranging it takes no click.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 400
    height: 200
    visible: true
    property bool failed: false
    property int opened: 0

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    TileStatusWords {
        id: words
    }

    StatusIcon {
        id: icon

        width: 48
        height: 48
        name: "Warnings"
        status: "to-do"
        words: words.words("notice.alarms", status, 2)
        sentence: words.sentence("notice.alarms", status, 2)
        clickHint: words.clickHint
        onOpened: window.opened += 1
    }

    Timer {
        interval: 400
        running: true
        onTriggered: {
            window.check(icon.Accessible.name === "Warnings: 2 warnings apply", "the screen reader hears the name and the state, got " + icon.Accessible.name);
            window.check(icon.hint === "2 of your warnings apply today. Click to see the whole tile.", "the hover text says what it means, got " + icon.hint);

            const types = ["notice.alarms", "notice.unsorted", "notice.fetch_gap", "bank.update_all"];
            const states = ["fine", "to-do", "running", "failed", "unknown"];
            for (const type of types) {
                for (const state of states) {
                    window.check(words.words(type, state, 1) !== "", "short words for " + type + " " + state);
                }
            }
            window.check(words.sentence("bank.update_all", "failed", 1).startsWith("The last update didn't work for 1 account"), "a failed update names how many accounts");

            icon.status = "something else";
            window.check(icon.shown === "unknown", "an unknown word shows as unknown");

            icon.clicked();
            window.check(window.opened === 1, "a click asks for the whole tile");
            icon.arranging = true;
            window.check(!icon.enabled, "while arranging the icon takes no click");

            console.log(window.failed ? "status: FAILED" : "status: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
