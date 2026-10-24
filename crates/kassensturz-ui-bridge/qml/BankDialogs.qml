// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What a bank asks while it is being contacted (M4.6, D-151): a TAN, a
// question such as "approve this in your banking app, then press
// Approved", or, when no PIN is remembered or saved, the PIN (M4.7). The bank's words are shown as plain text (D-143): they are
// data for the user to read, never instructions. Every request gets
// exactly one answer, also when the dialog is closed, since the bank
// connection waits for it. The PIN and the TAN are typed into a
// `SecretField`, which shows dots until the user asks to see them (D-162).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Item {
    id: dialogs

    required property BankConnection bank

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The made-up bank of `KASSENSTURZ_FAKE_BANK` has English words in its
    // code, which the German app would show as they are, so for it, and
    // only for the sentences it is known to ask, the words come from the
    // catalog. A real bank's words are shown as they come (D-143).
    function bankWords(text: string): string {
        if (!dialogs.bank.fake) {
            return text;
        }
        switch (text) {
        case "Type the TAN your TAN app shows for this login.":
            return dialogs.i18nContext.i18nc("@info the made-up bank asks for a TAN", "Type the TAN your TAN app shows for this login.");
        case "Waiting for approval":
            return dialogs.i18nContext.i18nc("@title the made-up bank waits for an approval", "Waiting for approval");
        case "Approve this login in your banking app, then choose Approved.":
            return dialogs.i18nContext.i18nc("@info the made-up bank waits for an approval", "Approve this login in your banking app, then choose Approved.");
        case "Approved":
            return dialogs.i18nContext.i18nc("@action:button the made-up bank's approval was given", "Approved");
        case "Abort":
            return dialogs.i18nContext.i18nc("@action:button stop the login", "Abort");
        default:
            return text;
        }
    }

    Connections {
        target: dialogs.bank

        function onTanRequested(request: var) {
            tanDialog.request = request;
            tanDialog.answered = false;
            tanField.text = "";
            tanField.hide();
            tanDialog.open();
            tanField.forceActiveFocus();
        }

        function onPinRequested(request: var) {
            pinDialog.request = request;
            pinDialog.answered = false;
            pinField.text = "";
            pinField.hide();
            pinMode.currentIndex = 0;
            pinDialog.open();
            pinField.forceActiveFocus();
        }

        function onQuestionAsked(question: var) {
            questionDialog.question = question;
            questionDialog.answered = false;
            questionDialog.open();
        }
    }

    Kirigami.Dialog {
        id: tanDialog

        property var request: ({})
        property bool answered: false

        function send() {
            if (tanField.text.trim().length === 0) {
                return;
            }
            answered = true;
            dialogs.bank.answerTan(tanField.text);
            close();
        }

        title: dialogs.i18nContext.i18nc("@title:window", "Your bank asks for a TAN")
        padding: Kirigami.Units.largeSpacing
        preferredWidth: Kirigami.Units.gridUnit * 24
        closePolicy: Controls.Popup.CloseOnEscape
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: dialogs.i18nContext.i18nc("@action:button send the TAN to the bank", "Send TAN")
                icon.name: "dialog-ok"
                enabled: tanField.text.trim().length > 0
                onTriggered: tanDialog.send()
            },
            Kirigami.Action {
                text: dialogs.i18nContext.i18nc("@action:button", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: tanDialog.close()
            }
        ]
        onClosed: {
            if (!answered) {
                answered = true;
                dialogs.bank.cancelTan();
            }
        }

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: dialogs.i18nContext.i18nc("@info", "Your bank wants you to confirm this with a TAN. The bank's message below says what it is for and where the TAN comes from, for example your TAN app or a text message:")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                // The bank's own words: data, shown as plain text (D-143).
                textFormat: Text.PlainText
                font.italic: true
                text: dialogs.bankWords(tanDialog.request.challenge ?? "")
                Accessible.name: dialogs.i18nContext.i18nc("@info:whatsthis", "Your bank's message")
            }

            SecretField {
                id: tanField

                Layout.fillWidth: true
                placeholderText: dialogs.i18nContext.i18nc("@info:placeholder", "TAN")
                Accessible.name: dialogs.i18nContext.i18nc("@label:textbox", "TAN")
                maximumLength: Math.max(1, tanDialog.request.maxLength ?? 64)
                onAccepted: tanDialog.send()
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
                text: dialogs.i18nContext.i18nc("@info", "KassenSturz sends the TAN to your bank once and doesn't keep it. It only reads from your bank; it never sends money.")
            }
        }
    }

    Kirigami.Dialog {
        id: pinDialog

        property var request: ({})
        property bool answered: false

        function send() {
            if (pinField.text.length === 0) {
                return;
            }
            answered = true;
            dialogs.bank.answerPin(pinField.text, pinMode.currentValue);
            pinField.text = "";
            close();
        }

        title: dialogs.i18nContext.i18nc("@title:window", "Your bank asks for your PIN")
        padding: Kirigami.Units.largeSpacing
        preferredWidth: Kirigami.Units.gridUnit * 24
        closePolicy: Controls.Popup.CloseOnEscape
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: dialogs.i18nContext.i18nc("@action:button send the PIN to the bank", "Send PIN")
                icon.name: "dialog-ok"
                enabled: pinField.text.length > 0
                onTriggered: pinDialog.send()
            },
            Kirigami.Action {
                text: dialogs.i18nContext.i18nc("@action:button", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: pinDialog.close()
            }
        ]
        onClosed: {
            pinField.text = "";
            if (!answered) {
                answered = true;
                dialogs.bank.cancelPin();
            }
        }

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                type: Kirigami.MessageType.Warning
                visible: pinDialog.request.rejected ?? false
                text: dialogs.i18nContext.i18nc("@info", "Your bank said the PIN you typed was wrong. KassenSturz has forgotten it. Check it before you try again: after a few wrong PINs a bank blocks the login.")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                // The bank's name comes from the bank lists: plain text
                // (D-143).
                textFormat: Text.PlainText
                text: dialogs.i18nContext.i18nc("@info %1 is the name of a bank", "Type the PIN you use for %1.", pinDialog.request.bank ?? "")
            }

            SecretField {
                id: pinField

                Layout.fillWidth: true
                placeholderText: dialogs.i18nContext.i18nc("@info:placeholder", "PIN")
                Accessible.name: dialogs.i18nContext.i18nc("@label:textbox", "PIN")
                onAccepted: pinDialog.send()
            }

            Controls.ComboBox {
                id: pinMode

                Layout.fillWidth: true
                Accessible.name: dialogs.i18nContext.i18nc("@label:listbox", "Remember the PIN:")
                textRole: "text"
                valueRole: "value"
                // Saving is offered unless the user chose to save PINs
                // nowhere in Settings (D-163).
                model: {
                    const choices = [
                        { value: "ask", text: dialogs.i18nContext.i18nc("@item:inlistbox keep the PIN", "No, ask me each time") },
                        { value: "remember", text: dialogs.i18nContext.i18nc("@item:inlistbox keep the PIN", "Until I close KassenSturz") },
                    ];
                    if (dialogs.bank.pinStorage !== "nowhere")
                        choices.push({ value: "save", text: dialogs.i18nContext.i18nc("@item:inlistbox keep the PIN", "Save it in my keychain") });
                    return choices;
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
                text: dialogs.i18nContext.i18nc("@info", "KassenSturz sends the PIN to your bank and keeps it only as long as you choose. It only reads from your bank; it never sends money.")
            }
        }
    }

    Kirigami.Dialog {
        id: questionDialog

        property var question: ({})
        property bool answered: false

        function choose(index: int) {
            answered = true;
            dialogs.bank.answerQuestion(index);
            close();
        }

        title: dialogs.i18nContext.i18nc("@title:window", "Your bank asks")
        padding: Kirigami.Units.largeSpacing
        preferredWidth: Kirigami.Units.gridUnit * 24
        closePolicy: Controls.Popup.CloseOnEscape
        standardButtons: Kirigami.Dialog.NoButton
        onClosed: {
            if (!answered) {
                answered = true;
                dialogs.bank.answerQuestion(-1);
            }
        }

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.bold: true
                text: dialogs.bankWords(questionDialog.question.title ?? "")
                visible: text.length > 0
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                // The bank's own words: data, shown as plain text (D-143).
                textFormat: Text.PlainText
                text: dialogs.bankWords(questionDialog.question.text ?? "")
            }

            // The bank's buttons, in its order: the first is the usual answer.
            RowLayout {
                Layout.alignment: Qt.AlignRight

                Repeater {
                    model: questionDialog.question.buttons ?? []

                    delegate: Controls.Button {
                        id: button

                        required property string modelData
                        required property int index

                        text: dialogs.bankWords(modelData)
                        highlighted: index === 0
                        // The label comes from the bank's library: plain
                        // text too.
                        contentItem: Controls.Label {
                            text: button.text
                            textFormat: Text.PlainText
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                        onClicked: questionDialog.choose(index)
                    }
                }
            }
        }
    }
}
