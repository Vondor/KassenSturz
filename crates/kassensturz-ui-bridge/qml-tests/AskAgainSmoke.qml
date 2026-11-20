// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: AI ask again

// Opens AiSettingsPage.qml on stand-ins for the Rust objects (fakes/) with a
// provider on another computer, an agreement and the choice not to be shown
// each request before it goes (D-935, D-936): the page says so under "What is
// sent" and has a button to be asked again, which takes the choice back and
// hides the line; without the choice neither is there.
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
    property int step: 0

    AiSwitch {
        id: ai
    }

    AiSettings {
        id: settings

        form: ({ address: "https://ai.example.org/v1", model: "example-model", keyUse: "none", place: "other", network: "internet", consent: "given", consentDay: "19.11.2026", reviewOff: true })
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
        return null;
    }

    Timer {
        interval: 700
        repeat: true
        running: true
        onTriggered: {
            const line = window.find(page, "askAgainLine");
            const button = window.find(page, "askAgainButton");
            switch (window.step) {
            case 0:
                window.check(line !== null && line.visible, "the page says the requests are not shown any more");
                window.check(button !== null && button.visible, "there is a button to be asked again");
                button.clicked();
                break;
            case 1:
                window.check(JSON.stringify(settings.reviewOffCalls) === "[false]", "the choice was taken back: " + JSON.stringify(settings.reviewOffCalls));
                window.check(!line.visible && !button.visible, "the line and the button are gone");
                // Without an agreement there is nothing to say either.
                settings.form = Object.assign({}, settings.form, { consent: "missing", reviewOff: true });
                page.reload();
                break;
            default:
                window.check(!line.visible && !button.visible, "nothing is said without an agreement");
                Qt.exit(window.failed ? 1 : 0);
            }
            window.step += 1;
        }
    }
}
