// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: request review

// Opens AiRequestReview.qml, the look at a request before it goes (M5.8,
// D-035), on the stand-ins for the Rust objects (fakes/), and checks that a
// request that comes while the window is still closing after "Send" is
// shown once the window has closed, not lost (B-375): a provider that
// answers at once, e.g. with a call of a function KassenSturz doesn't
// offer, sends the next request for a look within the closing animation.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 800
    height: 700
    visible: true
    property bool failed: false
    property int step: 0

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    AiSwitch {
        id: ai
    }

    AiChat {
        id: chat
    }

    AiRequestReview {
        id: review

        ai: ai
        chat: chat
    }

    // The window of the review, a popup among the item's data.
    function dialog() {
        for (let i = 0; i < review.data.length; ++i) {
            if (review.data[i].customFooterActions !== undefined) {
                return review.data[i];
            }
        }
        return null;
    }

    function request(body) {
        return {address: "https://ai.example.org/v1", model: "example-model", body: body, leftOut: 0};
    }

    Timer {
        interval: 700
        repeat: true
        running: true
        onTriggered: {
            const shown = window.dialog();
            switch (window.step) {
            case 0:
                window.check(shown !== null, "the review has a window");
                review.show(window.request("first"));
                break;
            case 1:
                window.check(shown.visible && review.waiting, "the first request is shown");
                // "Send", and at once the next request, as a provider that
                // answers at once makes it.
                shown.customFooterActions[0].trigger();
                review.show(window.request("second"));
                break;
            case 2:
                window.check(shown.visible, "the second request is shown");
                window.check(review.request.body === "second", "with its own text: " + review.request.body);
                window.check(review.waiting, "and waits for the user");
                window.check(JSON.stringify(chat.reviewAnswers) === JSON.stringify(["send"]), "one send, no stop: " + JSON.stringify(chat.reviewAnswers));
                shown.customFooterActions[0].trigger();
                break;
            case 3:
                window.check(!shown.visible && !review.waiting, "closed after the second send");
                window.check(JSON.stringify(chat.reviewAnswers) === JSON.stringify(["send", "send"]), "two sends: " + JSON.stringify(chat.reviewAnswers));
                // Stopped while it is shown: it closes and says nothing more.
                review.show(window.request("third"));
                break;
            case 4:
                window.check(shown.visible, "the third request is shown");
                review.hide();
                break;
            default:
                window.check(!shown.visible && !review.waiting, "closed when the question ended");
                window.check(chat.reviewAnswers.length === 2, "the end of the question is no answer: " + JSON.stringify(chat.reviewAnswers));
                Qt.exit(window.failed ? 1 : 0);
            }
            window.step += 1;
        }
    }
}
