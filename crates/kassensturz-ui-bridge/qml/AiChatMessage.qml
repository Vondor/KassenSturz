// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One message of the chat (M5.8): the user's question, the AI's answer, an
// error, or the changes the AI prepared. The AI's answer is untrusted text:
// it is shown as plain text and labelled as the AI's, and the numbers in it
// that no function of KassenSturz produced are named, so nobody takes them
// for figures from the book (D-188, threats T-26 and T-34). Hidden with the
// rest of AI while the switch is off (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: item

    required property AiSwitch ai
    // What the panel keeps for this message: its `kind` (`question`,
    // `answer`, `error`, `changes` or `changesDone`) and the fields of it.
    required property var message

    // The user wants to read the changes of the set with this ID.
    signal reviewClicked(string setId)
    // The user throws the set with this ID away.
    signal discardClicked(string setId)
    signal settingsClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    visible: ai.enabled
    spacing: Kirigami.Units.smallSpacing

    // The user's question.
    Rectangle {
        Layout.alignment: Qt.AlignRight
        Layout.maximumWidth: item.width - Kirigami.Units.gridUnit * 2
        visible: item.message.kind === "question"
        implicitWidth: questionLabel.implicitWidth + Kirigami.Units.largeSpacing * 2
        implicitHeight: questionLabel.implicitHeight + Kirigami.Units.largeSpacing
        radius: Kirigami.Units.cornerRadius
        color: Kirigami.Theme.highlightColor

        Controls.Label {
            id: questionLabel

            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            wrapMode: Text.Wrap
            color: Kirigami.Theme.highlightedTextColor
            textFormat: Text.PlainText
            text: item.message.kind === "question" ? item.message.text : ""
        }
    }

    // The AI's answer.
    Kirigami.AbstractCard {
        Layout.fillWidth: true
        visible: item.message.kind === "answer"

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                font.bold: true
                textFormat: Text.PlainText
                text: item.i18nContext.i18nc("@label the answer was written by an AI, not by KassenSturz", "Answer from the AI")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: item.message.kind === "answer" ? item.message.text : ""
                visible: item.message.kind === "answer" && item.message.text.trim() !== ""
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                visible: item.message.kind === "answer" && item.message.text.trim() === ""
                text: item.i18nContext.i18nc("@info", "The AI sent no text this time. You can ask again, or ask in other words.")
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                type: Kirigami.MessageType.Warning
                visible: item.message.kind === "answer" && item.message.unverified.length > 0
                text: visible ? Plain.plain(item.i18nContext.i18nc("@info %1 is a list of numbers", "Don't rely on these numbers in the answer: they don't come from your book, so the AI may have made them up: %1", item.message.unverified.join(", "))) : ""
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                type: Kirigami.MessageType.Information
                visible: item.message.kind === "answer" && item.message.cut === true
                text: item.i18nContext.i18nc("@info", "The answer was cut off because it got too long. Ask about less at a time.")
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                type: Kirigami.MessageType.Information
                visible: item.message.kind === "answer" && item.message.filtered === true
                text: item.i18nContext.i18nc("@info", "The provider cut this answer short or held it back for a reason of its own, for example its safety rules. You can ask again in other words.")
            }
        }
    }

    // Why a question was not answered.
    AiErrorMessage {
        Layout.fillWidth: true
        ai: item.ai
        visible: item.message.kind === "error"
        result: item.message.kind === "error" ? item.message.result : ({})
    }

    // Changes the AI prepared: nothing is applied before the user confirms
    // on the review screen (D-200).
    Kirigami.AbstractCard {
        Layout.fillWidth: true
        visible: item.message.kind === "changes"

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: item.message.kind !== "changes" ? ""
                    : item.message.fromTool === true
                        ? item.i18nContext.i18ncp("@info %1 is a number", "One of your own AI tools prepared %1 change for you. Nothing is changed until you have read it and confirmed.", "One of your own AI tools prepared %1 changes for you. Nothing is changed until you have read them and confirmed.", parseInt(item.message.count))
                        : item.i18nContext.i18ncp("@info %1 is a number", "The AI prepared %1 change for you. Nothing is changed until you have read it and confirmed.", "The AI prepared %1 changes for you. Nothing is changed until you have read them and confirmed.", parseInt(item.message.count))
            }

            RowLayout {
                Layout.fillWidth: true

                Controls.Button {
                    icon.name: "document-edit"
                    text: item.i18nContext.i18nc("@action:button", "Read and confirm…")
                    onClicked: item.reviewClicked(item.message.setId)
                }

                Controls.Button {
                    icon.name: "edit-delete"
                    text: item.i18nContext.i18nc("@action:button throw the prepared changes away", "Throw away")
                    onClicked: item.discardClicked(item.message.setId)
                }
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: item.message.kind === "changesDone"
        wrapMode: Text.Wrap
        font: Kirigami.Theme.smallFont
        textFormat: Text.PlainText
        text: item.message.kind === "changesDone"
            ? (item.message.applied
                ? item.i18nContext.i18nc("@info", "The prepared changes were made. You can take them back with Undo.")
                : item.i18nContext.i18nc("@info", "The prepared changes were thrown away. Nothing was changed."))
            : ""
    }
}
