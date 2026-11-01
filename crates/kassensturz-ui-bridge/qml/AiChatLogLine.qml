// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One line of the tool-call log (M5.8, D-047, D-192): what the user asked,
// what was sent to the provider (the exact request, after anything the
// user left out), which function of KassenSturz the AI called and what it
// got back. The texts come from the user, the book and the model, so they
// are shown as plain text (T-34). Hidden with the rest of AI while the
// switch is off (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: line

    required property AiSwitch ai
    // A line as the chat object reports it: a `kind` and its fields.
    required property var entry

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The long text behind the line: the request sent, or the function's
    // arguments and result; empty if the line has none.
    readonly property string details: {
        switch (entry.kind) {
        case "sent":
            return entry.body;
        case "called":
            return entry.arguments + "\n\n" + entry.result;
        default:
            return "";
        }
    }

    // The line in a sentence.
    function summary(): string {
        switch (entry.kind) {
        case "asked":
            return i18nContext.i18nc("@info %1 is what the user asked", "You asked: %1", entry.question);
        case "sent":
            return i18nContext.i18nc("@info %1 is a number", "Request %1 was sent to the provider.", entry.round);
        case "called":
            return entry.error !== ""
                ? i18nContext.i18nc("@info %1 is the name of a function of KassenSturz", "The AI used “%1”, which could not answer.", entry.function)
                : i18nContext.i18nc("@info %1 is the name of a function of KassenSturz", "The AI used “%1” and got the answer.", entry.function);
        case "notOffered":
            return i18nContext.i18ncp("@info %1 is a number", "The AI asked for %1 function that KassenSturz doesn't offer. It was not run.", "The AI asked for %1 functions that KassenSturz doesn't offer. They were not run.", entry.calls);
        case "overview":
            return i18nContext.i18nc("@info", "This AI can't use functions, so it gets a short overview of your book with each request instead.");
        case "answered":
            return entry.unverified > 0
                ? i18nContext.i18ncp("@info %1 is a number", "The AI answered. %1 number in the answer is not from your book.", "The AI answered. %1 numbers in the answer are not from your book.", entry.unverified)
                : i18nContext.i18nc("@info", "The AI answered. Every number in the answer is from your book.");
        case "failed":
            return i18nContext.i18nc("@info %1 is an error code", "The question ended without an answer (%1).", entry.code);
        default:
            return "";
        }
    }

    visible: ai.enabled
    spacing: 0

    Controls.ItemDelegate {
        Layout.fillWidth: true
        text: Plain.plain(line.summary())
        icon.name: line.details === "" ? "" : (more.visible ? "arrow-down" : "arrow-right")
        enabled: line.details !== ""
        Accessible.name: text
        onClicked: {
            ActionLog.taken("AiChatLogLine_visible")
            more.visible = !more.visible
        }
    }

    Controls.TextArea {
        id: more

        Layout.fillWidth: true
        Layout.preferredHeight: Kirigami.Units.gridUnit * 10
        visible: false
        readOnly: true
        wrapMode: TextEdit.WrapAnywhere
        textFormat: TextEdit.PlainText
        font.family: "monospace"
        text: line.details
        Accessible.name: line.i18nContext.i18nc("@label:textbox the details of one line of the log", "Details of this line")
    }
}
