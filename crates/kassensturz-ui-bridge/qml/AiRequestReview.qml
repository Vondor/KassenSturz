// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The look at a request before it goes (M5.8, D-035, D-210, threat T-22):
// the consent text promises that the user can see exactly what a provider on
// another computer receives and leave words out. The request is shown as it
// will be sent; leaving a phrase out shows it again; "Send" lets it go and
// "Don't send" ends the question, nothing having been sent. Closing the
// window counts as "Don't send". What the request holds is said first in a
// few plain words; the exact body, which is the provider's wire format and
// is shown as it is, as plain text, and the field to leave a phrase out are
// one step away under "Details" (D-935). A user who trusts the provider can
// tick "Don't ask again for this provider", which Settings can take back
// (D-936). Hidden with the rest of AI while the switch is off (D-013).

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

    // The request that waits: `address`, `model`, `body`, `leftOut`, `error`
    // (empty when the request may be sent), what it holds (`questions`,
    // `bookAnswers`, `functions`) and `canStopAsking`.
    property var request: ({})
    // Whether the user decided, so that closing the window afterwards is
    // not a second decision.
    property bool decided: false
    // A new request came while the window was still closing after "Send":
    // it opens again once it has closed. A provider that answers at once,
    // e.g. with a call of a function KassenSturz doesn't offer, sends the
    // next request for a look within the closing animation, and that
    // request was never shown, so the question waited forever (B-375).
    property bool reopen: false
    // Whether a request waits for the user's look, so the chat doesn't
    // count that time as the AI's.
    readonly property bool waiting: reopen || (dialog.visible && !decided)

    // Shows `request`, new or again after a phrase was left out.
    function show(request: var) {
        review.request = request;
        if (dialog.visible && decided) {
            reopen = true;
        } else if (!dialog.visible) {
            decided = false;
            leaveOut.text = "";
            details.checked = false;
            stopAsking.checked = false;
            dialog.open();
        }
    }

    // The question ended (stopped, say) while the window was open.
    function hide() {
        reopen = false;
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
                enabled: (review.request.error ?? "") === ""
                onTriggered: {
                    ActionLog.taken("AiRequestReview_send")
                    review.decided = true;
                    dialog.close();
                    // The choice not to be asked again goes with Send: the
                    // user who stops the question has chosen nothing.
                    if (stopAsking.visible && stopAsking.checked) {
                        review.chat.reviewSendAndStopAsking();
                    } else {
                        review.chat.reviewSend();
                    }
                }
            },
            Kirigami.Action {
                text: review.i18nContext.i18nc("@action:button end the question without sending the request", "Don't send")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("AiRequestReview_donTSend")
                    dialog.close()
                }
            }
        ]
        onClosed: {
            if (review.reopen) {
                review.reopen = false;
                review.decided = false;
                leaveOut.text = "";
                details.checked = false;
                stopAsking.checked = false;
                dialog.open();
                return;
            }
            if (!review.decided) {
                review.decided = true;
                review.chat.reviewStop();
            }
        }

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            // What goes, in a few words (D-935).
            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: review.i18nContext.i18nc("@info %1 is a web address, %2 the name of an AI model", "KassenSturz is about to send a request to %1 (model %2). Look at what it holds, then send it or stop.", review.request.address ?? "", review.request.model ?? "")
            }

            Controls.Label {
                id: summary

                objectName: "reviewSummary"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: {
                    const lines = [
                        review.i18nContext.i18ncp("@info %1 is a number", "What you wrote: %1 message.", "What you wrote: %1 messages.", review.request.questions ?? 0)
                    ];
                    lines.push((review.request.bookAnswers ?? 0) > 0
                        ? review.i18nContext.i18ncp("@info %1 is a number", "From your book, what the AI asked for: %1 answer.", "From your book, what the AI asked for: %1 answers.", review.request.bookAnswers)
                        : review.i18nContext.i18nc("@info", "Nothing from your book yet: the AI has not asked for anything."));
                    lines.push((review.request.functions ?? 0) > 0
                        ? review.i18nContext.i18ncp("@info %1 is a number", "KassenSturz's instructions, and the name of %1 function the AI may ask for.", "KassenSturz's instructions, and the names of %1 functions the AI may ask for.", review.request.functions)
                        : review.i18nContext.i18nc("@info", "KassenSturz's instructions."));
                    return lines.join("\n");
                }
            }

            Controls.Button {
                id: details

                objectName: "reviewDetailsButton"
                checkable: true
                icon.name: checked ? "arrow-up" : "arrow-down"
                text: Plain.plain(checked
                    ? review.i18nContext.i18nc("@action:button hide the exact text of the request", "Hide details")
                    : review.i18nContext.i18nc("@action:button show the exact text of the request", "Details…"))
                onToggled: {
                    ActionLog.taken("AiRequestReview_details")
                }
            }

            // The exact request, and what leaving a phrase out does to it.
            ColumnLayout {
                Layout.fillWidth: true
                visible: details.checked
                spacing: Kirigami.Units.largeSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: review.i18nContext.i18nc("@info", "This is exactly what is sent. Read it and leave out anything you don't want to send.")
                }

                Controls.TextArea {
                    objectName: "reviewBody"
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
                        onAccepted: {
                            ActionLog.taken("AiRequestReview_wordPhraseLeave")
                            review.leaveOutPhrase()
                        }
                    }

                    Controls.Button {
                        text: review.i18nContext.i18nc("@action:button", "Leave it out")
                        enabled: leaveOut.text.trim() !== ""
                        onClicked: {
                            ActionLog.taken("AiRequestReview_leaveOut")
                            review.leaveOutPhrase()
                        }
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: (review.request.leftOut ?? 0) > 0
                textFormat: Text.PlainText
                text: review.i18nContext.i18ncp("@info %1 is a number", "Left out in %1 place.", "Left out in %1 places.", review.request.leftOut ?? 0)
            }

            AiErrorMessage {
                objectName: "reviewError"
                Layout.fillWidth: true
                ai: review.ai
                visible: review.ai.enabled && (review.request.error ?? "") !== ""
                result: (review.request.error ?? "") !== "" ? ({ code: review.request.error }) : ({})
            }

            // For a provider the user trusts (D-935): not for the fake model.
            Controls.CheckBox {
                id: stopAsking

                objectName: "reviewStopAsking"
                Layout.fillWidth: true
                visible: review.request.canStopAsking === true
                text: review.i18nContext.i18nc("@option:check", "Don't ask again for this provider")
                onToggled: {
                    ActionLog.taken("AiRequestReview_dontAskAgain")
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: stopAsking.visible
                textFormat: Text.PlainText
                opacity: 0.7
                text: review.i18nContext.i18nc("@info", "Requests to this provider are then sent without being shown first. You can turn this back on in Settings, under AI, in What is sent.")
            }
        }
    }
}
