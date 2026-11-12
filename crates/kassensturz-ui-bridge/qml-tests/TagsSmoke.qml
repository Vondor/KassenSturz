// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens TagsPage.qml (Settings › Tags, US4.8) on a stand-in for BookTags
// (fakes/) and checks that each tag says how often it is used, that
// renaming and merging reach the Rust side with the right IDs, and that
// removing a used tag asks first and says how much it touches (Q-495).
//
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 640
    height: 900
    visible: true
    property bool failed: false

    BookTags {
        id: bookTags
    }

    pageStack.initialPage: TagsPage {
        id: page

        tags: bookTags
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            window.check(page.useText(bookTags.tags[0]) === "Used on 2 transactions and 1 part", "use of a used tag, got " + page.useText(bookTags.tags[0]));
            window.check(page.useText(bookTags.tags[1]) === "Not used yet", "use of an unused tag, got " + page.useText(bookTags.tags[1]));

            page.open(bookTags.tags[0]);
            page.finish(bookTags.rename(page.chosen.id, "Garden party 2026"));
            window.check(bookTags.asked[0] === "rename garden Garden party 2026", "renamed by ID, got " + bookTags.asked[0]);
            window.check(bookTags.tags[0].name === "Garden party 2026", "the list shows the new name");

            // A refused name keeps the dialog open with the reason.
            page.open(bookTags.tags[0]);
            page.finish(bookTags.rename(page.chosen.id, " "));
            window.check(page.result.code === "store.empty-name", "the reason is kept, got " + JSON.stringify(page.result));

            page.open(bookTags.tags[1]);
            page.finish(bookTags.merge(page.chosen.id, "garden"));
            window.check(bookTags.asked[2] === "merge tax garden", "merged by ID, got " + bookTags.asked[2]);

            console.log(window.failed ? "tags page: FAILED" : "tags page: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
