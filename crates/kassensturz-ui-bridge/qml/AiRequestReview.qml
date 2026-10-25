// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The look at a request before it goes (M5.8, D-035, D-210, threat T-22):
// the consent text promises that the user can see exactly what a provider on
// another computer receives and leave words out. The request is shown as it
// will be sent; leaving a phrase out shows it again; "Send" lets it go and
// "Don't send" ends the question, nothing having been sent. Closing the
// window counts as "Don't send". The body is the provider's wire format and
// is shown as it is, as plain text. Hidden with the rest of AI while the
// switch is off (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Item {
    id: review

    required property AiSwitch ai
    required property AiChat chat

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The request that waits: `address`, `model`, `body` and `leftOut`.
    property var request: ({})
    // Whether the user decided, so that closing the window afterwards is
    // not a second decision.
    property bool decided: false

    // Shows `request`, new or again after a phrase was left out.
    function show(request: var) {
        review.request = request;
        if (!dialog.visible) {
            decided = false;
            leaveOut.text = "";
            dialog.open();
        }
    }

    // The question ended (stopped, say) while the window was open.
    function hide() {
        decided = true;
        dialog.close();
    }

    function leaveOutPhrase() {
        const phrase = leaveOut.text.trim();
        if (phrase === "") {
            return;
        }
        chat.reviewLeaveOut(phrase);
        leaveOut.text = "";
    }

    visible: ai.enabled
    width: 0
    height: 0

    Kirigami.Dialog {
        id: dialog

        title: review.i18nContext.i18nc("@title:window", "Look at the request before it is sent")
        padding: Kirigami.Units.largeSpacing
        preferredWidth: Kirigami.Units.gridUnit * 32
        closePolicy: Controls.Popup.NoAutoClose
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: review.i18nContext.i18nc("@action:button send the request that is shown", "Send")
                icon.name: "mail-send"
                onTriggered: {
                    review.decided = true;
                    dialog.close();
                    review.chat.reviewSend();
                }
            },
            Kirigami.Action {
                text: review.i18nContext.i18nc("@action:button end the question without sending the request", "Don't send")
                icon.name: "dialog-cancel"
                onTriggered: dialog.close()
            }
        ]
        onClosed: {
            if (!review.decided) {
                review.decided = true;
                review.chat.reviewStop();
            }
        }

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: review.i18nContext.i18nc("@info %1 is a web address, %2 the name of an AI model", "This is exactly what goes to %1 (model %2). It holds your question, KassenSturz's instructions and what the AI asked from your book so far. Read it, leave out anything you don't want to send, then send it or stop.", review.request.address ?? "", review.request.model ?? "")
            }

            Controls.TextArea {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 14
                readOnly: true
                wrapMode: TextEdit.WrapAnywhere
                textFormat: TextEdit.PlainText
                font.family: "monospace"
                text: review.request.body ?? ""
                Accessible.name: review.i18nContext.i18nc("@label:textbox", "The request that would be sent")
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Controls.TextField {
                    id: leaveOut

                    Layout.fillWidth: true
                    placeholderText: review.i18nContext.i18nc("@info:placeholder", "A word or phrase to leave out")
                    inputMethodHints: Qt.ImhNoPredictiveText
                    Accessible.name: review.i18nContext.i18nc("@label:textbox", "A word or phrase to leave out of the request")
                    onAccepted: review.leaveOutPhrase()
                }

                Controls.Button {
                    text: review.i18nContext.i18nc("@action:button", "Leave it out")
                    enabled: leaveOut.text.trim() !== ""
                    onClicked: review.leaveOutPhrase()
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: (review.request.leftOut ?? 0) > 0
                textFormat: Text.PlainText
                text: review.i18nContext.i18ncp("@info %1 is a number", "Left out in %1 place.", "Left out in %1 places.", review.request.leftOut ?? 0)
            }
        }
    }
}
