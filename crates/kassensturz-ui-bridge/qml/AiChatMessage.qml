// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One message of the chat (M5.8): the user's question, the AI's answer, an
// error, the changes the AI prepared, or a problem report it drafted, which
// the user reads before opening KassenSturz's report page in the browser
// (D-228). The AI's answer is untrusted text:
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
    // The AI settings, for which of the notices the user wants (D-324).
    required property AiSettings settings
    // What the panel keeps for this message: its `kind` (`question`,
    // `answer`, `error`, `changes`, `changesDone`, `chart`, `chartFailed`,
    // `page` or `issue`) and the fields
    // of it.
    required property var message

    // The `AiChat`, for the chart cards.
    property var chat: null

    // The user wants to read the changes of the set with this ID.
    signal reviewClicked(string setId)
    // The user throws the set with this ID away.
    signal discardClicked(string setId)
    signal settingsClicked()
    // The user wants the chart of a card made in the Statistic Builder.
    signal chartChangeClicked(string title, string component, string period)
    // The user dealt with the chart of a card: `kept`, `builder` or `left`.
    signal chartHandled(string outcome)
    // The user wants the Statistic Builder, starting from the dataset the AI
    // understood (empty for none), after it gave up on a chart.
    signal builderClicked(string dataset)
    // The user opened the report page (`opened`) or copied the report's text
    // (`copied`).
    signal issueUsed(string how)

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
                visible: item.message.kind === "answer" && item.message.overview === true && item.settings.notice("overview")
                text: item.i18nContext.i18nc("@info", "This answer comes from a short overview of your accounts, because this model does not call KassenSturz's functions. Questions the overview does not cover cannot be answered exactly. A model that supports “tools” or “function calling” works best.")
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                type: Kirigami.MessageType.Warning
                visible: item.message.kind === "answer" && item.message.contextCut === true && item.settings.notice("context")
                text: item.i18nContext.i18nc("@info", "The AI may not have seen all of this conversation: the provider counted far fewer tokens than KassenSturz sent. This happens with a local model whose context length is small. Raise the context length in the program that runs the model (16000 or more), or start a new chat.")
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                type: Kirigami.MessageType.Information
                visible: item.message.kind === "answer" && item.message.cut === true && item.settings.notice("cut")
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
        offerSettings: true
        onSettingsClicked: item.settingsClicked()
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

    // A chart the AI prepared, drawn from the book, with Keep, Change… and
    // Leave out (M6c.9, D-355).
    AiChartCard {
        Layout.fillWidth: true
        ai: item.ai
        chat: item.chat
        setId: item.message.kind === "chart" ? item.message.setId : ""
        unverified: item.message.kind === "chart" ? (item.message.unverified ?? []) : []
        outcome: item.message.kind === "chart" ? (item.message.outcome ?? "") : ""
        onChangeClicked: (title, component, period) => item.chartChangeClicked(title, component, period)
        onHandled: outcome => item.chartHandled(outcome)
    }

    // The AI could not prepare the chart, even after two corrections: said
    // plainly, with the way on (D-355, D-402).
    Kirigami.AbstractCard {
        Layout.fillWidth: true
        visible: item.message.kind === "chartFailed"

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: item.i18nContext.i18nc("@info", "The AI could not prepare this chart, even after two corrections. Nothing was changed. You can make the chart yourself in the Statistic Builder.")
            }

            Controls.Button {
                icon.name: "view-statistics"
                text: item.i18nContext.i18nc("@action:button open the Statistic Builder to make a chart yourself", "Open the Statistic Builder")
                onClicked: item.builderClicked(item.message.kind === "chartFailed" ? (item.message.dataset ?? "") : "")
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

    // A page the AI opened for the user (D-265): only a note, the page was
    // shown by the window. Opening a page changes nothing.
    Controls.Label {
        Layout.fillWidth: true
        visible: item.message.kind === "page"
        wrapMode: Text.Wrap
        font: Kirigami.Theme.smallFont
        textFormat: Text.PlainText
        text: item.message.kind === "page" ? item.pageNote(item.message.page, item.message.hasId === true) : ""
    }

    // What to tell the user the AI opened.
    function pageNote(page: string, hasId: bool): string {
        switch (page) {
        case "overview":
            return item.i18nContext.i18nc("@info the AI opened a page", "The AI opened the overview.");
        case "accounts":
            return item.i18nContext.i18nc("@info the AI opened a page", "The AI opened your accounts.");
        case "categories":
            return item.i18nContext.i18nc("@info the AI opened a page", "The AI opened your categories.");
        case "account":
            return item.i18nContext.i18nc("@info the AI opened a page", "The AI opened the transactions of an account.");
        default:
            return hasId
                ? item.i18nContext.i18nc("@info the AI opened a page", "The AI opened your Rules and selected one.")
                : item.i18nContext.i18nc("@info the AI opened a page", "The AI opened your Rules.");
        }
    }

    // A problem report the AI drafted: shown exactly as the page would get
    // it. KassenSturz sends nothing; the page opens in the user's browser,
    // where they decide (D-173, D-228).
    Kirigami.AbstractCard {
        Layout.fillWidth: true
        visible: item.message.kind === "issue"

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                font.bold: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: item.i18nContext.i18nc("@label a report of a problem in KassenSturz, written by an AI", "Problem report drafted by the AI")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: item.i18nContext.i18nc("@info", "Read it first. KassenSturz sends nothing: the button opens KassenSturz's report page on GitHub in your browser with this text filled in, and there you can still change it and decide whether to send it. Reports on GitHub can be read by anyone.")
            }

            Controls.Label {
                Layout.fillWidth: true
                font.bold: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: item.message.kind === "issue" ? item.message.title : ""
            }

            Controls.TextArea {
                id: issueBody

                Layout.fillWidth: true
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                textFormat: TextEdit.PlainText
                text: item.message.kind === "issue" ? item.message.body : ""
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: item.message.kind === "issue" && item.message.wholeInLink !== true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: item.i18nContext.i18nc("@info", "The text is too long to fill in for you. Copy it, open the page and paste it there.")
            }

            RowLayout {
                Layout.fillWidth: true

                Controls.Button {
                    icon.name: "internet-services"
                    text: item.i18nContext.i18nc("@action:button opens the problem report page in the browser", "Open the report page")
                    onClicked: {
                        Qt.openUrlExternally(item.message.link);
                        item.issueUsed("opened");
                    }
                }

                Controls.Button {
                    icon.name: "edit-copy"
                    text: item.i18nContext.i18nc("@action:button copies the report's text", "Copy the text")
                    onClicked: {
                        issueBody.selectAll();
                        issueBody.copy();
                        issueBody.deselect();
                        item.issueUsed("copied");
                    }
                }
            }
        }
    }
}
