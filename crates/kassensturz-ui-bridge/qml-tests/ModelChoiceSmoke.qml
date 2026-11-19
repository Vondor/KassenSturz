// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: AI model list

// Opens AiSettingsPage.qml on stand-ins for the Rust objects (fakes/) and
// checks the model field (B-396, D-871): once the provider lists its
// models it is a list with the first chat model chosen, a search-only
// model can't be chosen, "Enter another name…" ends the list, and a
// provider that names no models leaves a plain text field.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 1200
    visible: true
    property bool failed: false

    AiSwitch {
        id: ai
    }

    AiSettings {
        id: settings
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

    Timer {
        interval: 300
        running: true
        onTriggered: {
            window.check(!page.modelListShown, "no list before a test");
            settings.testFinished({ models: ["text-embed-mini", "chat-small", "chat-large"] });
            window.check(page.modelListShown, "the list is shown once the provider lists models");
            const choices = page.modelChoices;
            window.check(choices.length === 4, "three models and 'Enter another name', got " + choices.length);
            window.check(choices[0].search === true && choices[1].search === false, "only the embedding model is search-only");
            window.check(choices[3].value === "", "the last entry is 'Enter another name'");
            window.check(page.chosenModel === "chat-small", "the first chat model is chosen, got " + page.chosenModel);
            settings.testFinished({ code: "ai.not-found" });
            window.check(!page.modelListShown && page.noModelList, "no list: a plain text field with the calm hint");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
