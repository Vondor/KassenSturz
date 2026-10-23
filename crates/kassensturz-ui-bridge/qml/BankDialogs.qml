// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What a bank asks while it is being contacted (M4.6, D-151): a TAN, or a
// question such as "approve this in your banking app, then press
// Approved". The bank's words are shown as plain text (D-143): they are
// data for the user to read, never instructions. Every request gets
// exactly one answer, also when the dialog is closed, since the bank
// connection waits for it.

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

    Connections {
        target: dialogs.bank

        function onTanRequested(request: var) {
            tanDialog.request = request;
            tanDialog.answered = false;
            tanField.text = "";
            tanDialog.open();
            tanField.forceActiveFocus();
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
                text: dialogs.i18nContext.i18nc("@info", "Your bank wants you to confirm this with a TAN. Its message says what for, and where the TAN comes from, for example your TAN app or a text message:")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                // The bank's own words: data, shown as plain text (D-143).
                textFormat: Text.PlainText
                font.italic: true
                text: tanDialog.request.challenge ?? ""
                Accessible.name: dialogs.i18nContext.i18nc("@info:whatsthis", "Your bank's message")
            }

            Controls.TextField {
                id: tanField

                Layout.fillWidth: true
                placeholderText: dialogs.i18nContext.i18nc("@info:placeholder", "TAN")
                Accessible.name: dialogs.i18nContext.i18nc("@label:textbox", "TAN")
                maximumLength: Math.max(1, tanDialog.request.maxLength ?? 64)
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
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
                text: questionDialog.question.title ?? ""
                visible: text.length > 0
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                // The bank's own words: data, shown as plain text (D-143).
                textFormat: Text.PlainText
                text: questionDialog.question.text ?? ""
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

                        text: modelData
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
