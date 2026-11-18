// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: testing guide
// check-panel-desktop: testing guide in the desktop style

// Opens TestingGuidePage.qml (Settings › About › Testing KassenSturz, #1030,
// D-792) and checks that step 1 shows the version and the kind of install,
// that the log button is there only in developer mode, that Back and Next
// walk the three steps and stop at both ends, that the example book button
// reaches its signal, and that the note carries what the tester typed and
// chose plus the version, the install and whether a log was written.
//
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 640
    height: 900
    visible: true
    property bool failed: false
    property int examples: 0
    property int logs: 0
    property int dones: 0

    pageStack.initialPage: TestingGuidePage {
        id: page

        version: "0.261118.3 (abc1234)"
        installKind: "snap"
        developerOn: true
        onExampleBookClicked: window.examples += 1
        onShowLogClicked: window.logs += 1
        onDoneClicked: window.dones += 1
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function child(item, name) {
        const items = item.children ?? [];
        for (let i = 0; i < items.length; ++i) {
            const found = items[i].objectName === name ? items[i] : child(items[i], name);
            if (found) {
                return found;
            }
        }
        if (item.contentItem && item.contentItem !== item && items.indexOf(item.contentItem) < 0) {
            return item.contentItem.objectName === name ? item.contentItem : child(item.contentItem, name);
        }
        return null;
    }

    function found(name) {
        const item = child(page, name);
        check(item !== null, name + " is on the page");
        return item;
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            // Step 1: the install.
            window.check(page.step === 0, "the guide starts at step 1");
            window.check(window.found("installStep").visible, "step 1 shows");
            window.check(!window.found("noteStep").visible, "step 3 waits");
            window.check(window.found("versionRow").description === "0.261118.3 (abc1234)", "the version, got " + window.found("versionRow").description);
            window.check(window.found("installRow").description === "Snap", "the install, got " + window.found("installRow").description);
            for (const kind of ["flatpak", "windows", "macos", "other", "something new"]) {
                window.check(page.installWords(kind) !== "", "words for " + kind);
            }
            window.check(page.installWords("something new") === page.installWords("other"), "an unknown kind reads as another way");
            window.check(window.found("showLogButton").visible, "the log button in developer mode");
            window.found("showLogButton").clicked();
            window.check(window.logs === 1, "the log button asks for the log");
            window.check(!window.found("backButton").enabled, "no way back from step 1");

            // Step 2: the example book.
            window.found("nextButton").clicked();
            window.check(page.step === 1 && window.found("tryStep").visible, "Next shows step 2");
            window.found("exampleBookButton").clicked();
            window.check(window.examples === 1, "the example book button asks for one");

            // Step 3: the note.
            window.found("nextButton").clicked();
            window.check(page.step === 2 && window.found("noteStep").visible, "Next shows step 3");
            window.check(!window.found("nextButton").visible && window.found("doneButton").visible, "the last step offers Done, not Next");
            page.goTo(7);
            window.check(page.step === 2, "the steps stop at the last one");

            window.found("titleField").text = "Undo button greyed out";
            window.found("whereChoice").currentIndex = 3;
            window.found("wantedArea").text = "Take back a made-up payment";
            window.found("expectedArea").text = "It is gone";
            window.found("happenedArea").text = "Nothing <b>happened</b>";
            window.found("oftenChoice").currentIndex = 1;
            window.found("badChoice").currentIndex = 2;
            const note = window.found("notePreview").text;
            const lines = note.split("\n");
            window.check(lines.length === 12, "the note has 12 lines, got " + lines.length);
            window.check(lines[1] === "Title: Undo button greyed out", "the title line, got " + lines[1]);
            window.check(lines[2] === "Where: Transactions and transfers", "the place, got " + lines[2]);
            window.check(lines[5] === "What happened: Nothing <b>happened</b>", "typed text stays as typed, got " + lines[5]);
            window.check(lines[6] === "How often: Sometimes", "how often, got " + lines[6]);
            window.check(lines[7] === "How bad: A small thing", "how bad, got " + lines[7]);
            window.check(lines[8] === "Real data in this note: no", "no real data by default, got " + lines[8]);
            window.check(lines[9] === "Version: 0.261118.3 (abc1234)", "the version line, got " + lines[9]);
            window.check(lines[10] === "Installed as: Snap", "the install line, got " + lines[10]);
            window.check(lines[11] === "Developer mode: on, a log was written", "the log line, got " + lines[11]);

            window.found("realDataCheck").toggle();
            window.check(window.found("notePreview").text.split("\n")[8] === "Real data in this note: yes", "the real data line follows the box");

            window.found("copyButton").clicked();
            window.check(page.copied, "Copy says the note is on the clipboard");
            window.found("wantedArea").text = "Something else";
            window.check(!page.copied, "a change takes the copied notice away");

            page.developerOn = false;
            window.check(window.found("notePreview").text.split("\n")[11] === "Developer mode: off, no log", "the log line without developer mode");
            page.goTo(0);
            window.check(!window.found("showLogButton").visible, "no log button outside developer mode");
            page.goTo(-3);
            window.check(page.step === 0, "the steps stop at the first one");

            page.goTo(2);
            window.found("doneButton").clicked();
            window.check(window.dones === 1, "Done closes the guide");

            console.log(window.failed ? "testing guide: FAILED" : "testing guide: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
