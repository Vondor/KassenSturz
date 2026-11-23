// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: AI figure labels in chat answers
// check-panel-desktop: AI figure labels in chat answers (desktop style)

// Loads AiChatPanel.qml with the stand-ins and sends finished() answers as
// the Rust bridge emits them (D-952, #1358): one answer with a single-label
// figure and an ambiguous two-label figure, and one answer with no labelled
// figures. Checks that AiChatPanel forwards `labelled` through its ListModel
// into AiChatMessage, that the heading and each formatted label line appear
// without any QML warnings, and that the block stays hidden when empty.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 520
    height: 800
    visible: true
    property bool failed: false
    property int step: 0

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function collectTexts(item, out) {
        if (item === null || item === undefined) {
            return;
        }
        if (item.visible && typeof item.text === "string" && item.text !== "") {
            out.push(item.text);
        }
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            collectTexts(children[i], out);
        }
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
        interval: 400
        repeat: true
        running: true
        onTriggered: {
            if (window.step === 0) {
                // Emit an answer with one unambiguous and one two-label figure,
                // shaped exactly as ai_chat_model.rs builds it.
                chat.finished({
                    text: "Balance is 2.208,25 EUR and groceries were 45,25 EUR.",
                    unverified: [],
                    labelled: [
                        {
                            number: "2.208,25",
                            labels: [
                                { kind: "account-balance", name: "Main Checking", from: "2026-11-23", to: "" }
                            ]
                        },
                        {
                            number: "45,25",
                            labels: [
                                { kind: "category-total", name: "Groceries", from: "2026-11-01", to: "2026-11-30" },
                                { kind: "payee-total", name: "Redwood Market", from: "2026-11-01", to: "2026-11-30" }
                            ]
                        }
                    ],
                    finish: "stop",
                    overview: false,
                    contextCut: false,
                    sets: [],
                    charts: []
                });
            } else if (window.step === 1) {
                const texts = [];
                window.collectTexts(panel, texts);
                const joined = texts.join("\n");
                window.check(
                    texts.indexOf("What the figures in this answer are in your book") >= 0,
                    "figure-label heading is shown; got:\n" + joined
                );
                window.check(
                    texts.indexOf("2.208,25: balance of the account Main Checking on 2026-11-23") >= 0,
                    "single-label account-balance line is shown; got:\n" + joined
                );
                window.check(
                    texts.indexOf("45,25 is one of these: the category Groceries, with everything inside it, from 2026-11-01 to 2026-11-30; the other party Redwood Market from 2026-11-01 to 2026-11-30") >= 0,
                    "ambiguous two-label line is shown; got:\n" + joined
                );

                // Now reset and emit an answer with no labelled figures: heading must stay hidden.
                panel.startOver();
                chat.finished({
                    text: "No figures here.",
                    unverified: [],
                    labelled: [],
                    finish: "stop",
                    overview: false,
                    contextCut: false,
                    sets: [],
                    charts: []
                });
            } else {
                const texts = [];
                window.collectTexts(panel, texts);
                window.check(
                    texts.indexOf("What the figures in this answer are in your book") < 0,
                    "figure-label heading is hidden when labelled is empty"
                );
                Qt.exit(window.failed ? 1 : 0);
            }
            window.step += 1;
        }
    }
}
