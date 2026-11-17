// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: status icon

// Opens StatusIcon.qml with TileStatusWords.qml (US5.4, D-734, D-736) and
// checks that each state has words for the screen reader and a hover
// sentence, that an unknown word shows as "unknown", that a click asks for
// the whole tile and that while arranging it takes no click; and that a
// tile's emblem (D-750) is drawn with the state as a corner badge.
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

    StatusIcon {
        id: bell

        x: 60
        width: 48
        height: 48
        name: "Warnings"
        status: "to-do"
        emblem: "notice.alarms"
    }

    StatusIcon {
        id: plain

        x: 120
        width: 48
        height: 48
        status: "fine"
        emblem: "something.else"
    }

    function child(item, name) {
        for (let i = 0; i < item.children.length; ++i) {
            const found = item.children[i].objectName === name ? item.children[i] : child(item.children[i], name);
            if (found) {
                return found;
            }
        }
        return null;
    }

    Timer {
        interval: 400
        running: true
        onTriggered: {
            window.check(icon.Accessible.name === "Warnings: 2 warnings apply", "the screen reader hears the name and the state, got " + icon.Accessible.name);
            window.check(icon.hint === "2 of your warnings apply today. Click to see the whole tile.", "the hover text says what it means, got " + icon.hint);

            const types = ["notice.alarms", "notice.unsorted", "notice.fetch_gap", "bank.update_all", "forecast.month", "plan.coming_up"];
            const states = ["fine", "to-do", "running", "failed", "unknown"];
            for (const type of types) {
                for (const state of states) {
                    window.check(words.words(type, state, 1) !== "", "short words for " + type + " " + state);
                }
            }
            window.check(words.sentence("bank.update_all", "failed", 1).startsWith("The last update didn't work for 1 account"), "a failed update names how many accounts");
            window.check(words.words("forecast.month", "to-do", 2) === "2 planned payments to check", "the forecast names the payments to check, got " + words.words("forecast.month", "to-do", 2));
            window.check(words.words("plan.coming_up", "fine", 0) === "Nothing coming up" && words.words("plan.coming_up", "fine", 3) === "3 payments coming up", "coming up counts the payments");

            icon.status = "something else";
            window.check(icon.shown === "unknown", "an unknown word shows as unknown");

            icon.clicked();
            window.check(window.opened === 1, "a click asks for the whole tile");
            icon.arranging = true;
            window.check(!icon.enabled, "while arranging the icon takes no click");

            const emblem = window.child(bell, "emblem");
            const badge = window.child(bell, "state");
            window.check(emblem !== null && emblem.visible && emblem.width > 0, "a known tile draws its emblem");
            window.check(badge !== null && Math.abs(badge.width - emblem.width / 2) <= 1, "beside an emblem the state is a badge half its size, got " + (badge ? badge.width : "none"));
            window.check(bell.flagged && bell.background.border.width === 1, "something to do frames the icon");
            window.check(!window.child(plain, "emblem").visible && window.child(plain, "state").width === window.child(plain, "emblem").width, "an unknown tile draws no emblem and the state fills the icon");
            window.check(!plain.flagged && !icon.flagged, "fine, or no emblem, draws no frame");

            console.log(window.failed ? "status: FAILED" : "status: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
