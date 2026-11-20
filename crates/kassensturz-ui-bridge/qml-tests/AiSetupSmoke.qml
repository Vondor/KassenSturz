// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: AI one setup screen

// Opens AiSettingsPage.qml on stand-ins for the Rust objects (fakes/) and
// checks the one setup screen for a provider on another computer (B-391,
// D-866): it shows what leaves the computer, the warning for http:// only
// when it applies, and one box that agrees to both; "Save and test" saves,
// agrees and tests in that order, and no second consent dialog opens.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 1400
    visible: true
    property bool failed: false

    AiSwitch {
        id: ai
    }

    AiSettings {
        id: settings

        // Nothing saved yet.
        form: ({ address: "", model: "", keyUse: "none", place: "this", consent: "notNeeded" })
    }

    pageStack.initialPage: AiSettingsPage {
        id: page

        ai: ai
        settings: settings
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    // The first item under `item` with the objectName `name`.
    function find(item, name) {
        if (item.objectName === name) {
            return item;
        }
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            const found = window.find(children[i], name);
            if (found !== null) {
                return found;
            }
        }
        if (item.contentItem !== undefined && item.contentItem !== null && children.indexOf(item.contentItem) < 0) {
            return window.find(item.contentItem, name);
        }
        return null;
    }

    Timer {
        interval: 300
        running: true
        onTriggered: {
            const address = window.find(page, "addressField");
            const agree = window.find(page, "agreeBox");
            const save = window.find(page, "saveButton");
            window.check(address !== null && agree !== null && save !== null, "the address field, the box and Save are found");
            if (address === null || agree === null || save === null) {
                Qt.exit(1);
                return;
            }
            window.check(!page.oneScreen, "no setup screen without an address");

            // On this computer: nothing to agree to.
            address.text = "http://localhost:1234/v1";
            page.checkTyped();
            window.check(!page.remote && !page.oneScreen, "a provider on this computer needs no agreement");

            // http:// at home: the warning, one box for both, and saving
            // waits for it.
            address.text = "http://192.168.1.20:8080/v1";
            page.checkTyped();
            window.check(page.oneScreen && page.plainHttp, "an http address at home shows the one screen with its warning");
            window.check(agree.visible && agree.text === "I have read this and agree, also to the unencrypted connection.", "the one box names the unencrypted connection, got " + agree.text);
            window.check(!save.enabled, "saving waits until the user agrees");
            agree.toggle();
            agree.toggled();
            window.check(save.enabled && save.text === "Save and test", "agreeing offers Save and test, got " + save.text);
            settings.calls = [];
            page.save();
            window.check(JSON.stringify(settings.calls) === JSON.stringify(["save true", "consent", "test"]), "saved with http allowed, then agreed, then tested, got " + JSON.stringify(settings.calls));
            window.check(!agree.checked, "the box is cleared after saving");

            // https:// at home: no http warning; saving without agreeing
            // keeps everything unsent and opens no dialog.
            address.text = "https://192.168.1.21/v1";
            page.checkTyped();
            window.check(page.oneScreen && !page.plainHttp, "an https address at home shows the one screen without the http warning");
            window.check(agree.text === "I have read this and agree.", "the plain box, got " + agree.text);
            window.check(save.enabled && save.text === "Save", "saving without agreeing is possible, got " + save.text);
            settings.calls = [];
            page.save();
            window.check(settings.calls.indexOf("consent") < 0, "no agreement without the box");

            // B-408: after "Save and test" without a model, the model chosen
            // from the list is saved with the agreement kept, without
            // agreeing again.
            settings.keepsSaved = true;
            settings.form = ({ address: "", model: "", keyUse: "none", place: "this", consent: "notNeeded" });
            page.reload();
            address.text = "http://192.168.1.22:8080/v1";
            page.checkTyped();
            agree.toggle();
            agree.toggled();
            settings.calls = [];
            page.save();
            window.check(settings.form.consent === "given", "agreed with the first save");
            settings.testFinished({ models: ["chat-small", "chat-large"] });
            window.check(page.chosenModel === "chat-small", "the first model is chosen, got " + page.chosenModel);
            window.check(!page.oneScreen, "choosing a model doesn't ask to agree again");
            window.check(save.enabled && save.text === "Save", "the chosen model can be saved, got " + save.text + " " + save.enabled);
            settings.calls = [];
            page.save();
            window.check(JSON.stringify(settings.calls) === JSON.stringify(["save true"]), "saved with http still allowed and no second agreement, got " + JSON.stringify(settings.calls));
            window.check(settings.form.model === "chat-small" && settings.form.consent === "given", "the model is saved and the agreement kept");

            // B-431: "Show what the test sends" shows the request in a box
            // as wide as the page allows, not one character wide. The layout
            // settles after the box shows, so the size is read by a second
            // timer.
            const toggle = window.find(page, "testRequestToggle");
            const box = window.find(page, "testRequestBox");
            window.check(toggle !== null && box !== null, "the test request button and box are found");
            if (toggle === null || box === null) {
                Qt.exit(1);
                return;
            }
            window.check(toggle.visible, "a saved provider with a model offers to show the test request");
            toggle.checked = true;
            window.check(box.visible, "the box shows after the button is pressed");
            sizeTimer.start();
        }
    }

    Timer {
        id: sizeTimer

        interval: 300
        onTriggered: {
            const box = window.find(page, "testRequestBox");
            window.check(box.width > Kirigami.Units.gridUnit * 15, "the test request box is wide, got " + box.width);
            window.check(box.height >= Kirigami.Units.gridUnit * 5, "the test request box is tall, got " + box.height);
            // B-431: the whole request fits in the box, and what follows it
            // on the page starts below it instead of lying under it. With a
            // fixed height the text ran over the banners below.
            window.check(box.height + 1 >= box.contentHeight, "the whole request fits in the box, height " + box.height + " for " + box.contentHeight);
            const siblings = box.parent.children;
            let below = null;
            for (let i = siblings.indexOf(box) + 1; i < siblings.length && below === null; ++i) {
                if (siblings[i].visible && siblings[i].height > 0) {
                    below = siblings[i];
                }
            }
            window.check(below !== null, "something follows the test request box");
            if (below !== null) {
                window.check(below.y >= box.y + box.height, "what follows the box starts below it, at " + below.y + " for a box ending at " + (box.y + box.height));
            }
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
