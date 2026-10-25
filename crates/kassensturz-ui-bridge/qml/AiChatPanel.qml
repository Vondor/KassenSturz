// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The chat with the AI (M5.8, D-047, D-210 to D-213): the content of the
// sidebar on the right of the window. The user types a question; the answer
// is labelled as the AI's and shown as plain text (T-34); numbers in it that
// no function of KassenSturz produced are named (D-188). The second tab shows
// the tool-call log, everything that was asked and sent (D-192). Changes the
// AI prepares are never applied here: the message offers the review screen
// (`AiChangeReviewPage`), whose confirm button is the only way to apply them
// (D-200). When the provider is on another computer, each request is shown
// first (`AiRequestReview`). Hidden with the rest of AI while the switch is
// off (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: panel

    required property AiSwitch ai
    required property AiChat chat
    required property AiSettings settings

    // The user wants to choose the provider.
    signal settingsClicked()
    // The user wants to read the changes of the set with this ID.
    signal reviewChangesClicked(string setId)
    // The user wants the sidebar closed.
    signal closeClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The words other files borrow, so that none of them names the AI
    // itself while the switch is off (D-013).
    readonly property string toggleText: i18nContext.i18nc("@action:inmenu opens the chat sidebar", "Chat with the AI")
    readonly property string appliedName: i18nContext.i18nc("@info a change you made", "Applied changes the AI prepared")

    // The messages of the conversation, oldest first, and the tool-call log.
    property var messages: []
    property var logLines: []
    // What settings.load() found: whether a provider is saved, and how its
    // key is kept.
    property var form: ({})
    readonly property bool hasProvider: (form.address ?? "") !== "" && (form.model ?? "") !== ""
        && (form.addressError ?? "") === ""
    readonly property bool asksForKey: (form.keyUse ?? "") === "ask"
    // Whether the user pressed Stop and the question has not ended yet.
    property bool stopping: false

    function refresh() {
        form = settings.load();
    }

    function add(message: var) {
        messages = messages.concat([message]);
        Qt.callLater(() => messageList.positionViewAtEnd());
    }

    // The review screen or the user's choice ended with a set.
    function markHandled(setId: string, applied: bool) {
        messages = messages.map(message => message.setId === setId ? ({ kind: "changesDone", applied: applied }) : message);
    }

    function startOver() {
        chat.reset();
        messages = [];
        logLines = [];
        stopping = false;
    }

    function ask() {
        const question = questionField.text.trim();
        if (question === "" || chat.busy) {
            return;
        }
        const started = chat.ask(question, keyField.text);
        if (started.started !== true) {
            if (started.code !== undefined) {
                add({ kind: "error", result: started });
            }
            return;
        }
        add({ kind: "question", text: question });
        questionField.text = "";
        keyField.text = "";
    }

    Component.onCompleted: refresh()

    visible: ai.enabled
    spacing: Kirigami.Units.smallSpacing

    Connections {
        target: panel.chat

        function onLogged(entry: var) {
            panel.logLines = panel.logLines.concat([entry]);
        }

        function onReviewing(request: var) {
            review.show(request);
        }

        function onFinished(result: var) {
            panel.stopping = false;
            review.hide();
            if (result.code !== undefined) {
                panel.add({ kind: "error", result: result });
                return;
            }
            panel.add({
                kind: "answer",
                text: result.text,
                unverified: result.unverified,
                cut: result.finish === "length",
                filtered: result.finish === "other",
                overview: result.overview
            });
            for (let i = 0; i < result.sets.length; i++) {
                panel.add({ kind: "changes", setId: result.sets[i].id, count: result.sets[i].count });
            }
        }
    }

    AiRequestReview {
        id: review

        ai: panel.ai
        chat: panel.chat
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.smallSpacing

        Kirigami.Heading {
            Layout.fillWidth: true
            level: 3
            elide: Text.ElideRight
            text: panel.i18nContext.i18nc("@title the chat sidebar", "Chat with the AI")
            textFormat: Text.PlainText
        }

        Controls.ToolButton {
            icon.name: "edit-clear-all"
            enabled: panel.messages.length > 0 || panel.logLines.length > 0
            text: panel.i18nContext.i18nc("@action:button forget the conversation and start a new one", "New conversation")
            display: Controls.AbstractButton.IconOnly
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: text
            Accessible.name: text
            onClicked: panel.startOver()
        }

        Controls.ToolButton {
            icon.name: "window-close"
            text: panel.i18nContext.i18nc("@action:button close the chat sidebar", "Close the chat")
            display: Controls.AbstractButton.IconOnly
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: text
            Accessible.name: text
            onClicked: panel.closeClicked()
        }
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        type: Kirigami.MessageType.Information
        visible: panel.settings.fake
        text: panel.i18nContext.i18nc("@info", "Test mode: the answers come from a made-up model built into KassenSturz. Nothing is sent anywhere.")
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        type: Kirigami.MessageType.Information
        visible: !panel.hasProvider
        text: panel.i18nContext.i18nc("@info", "Choose which AI to use before you ask a question.")
        actions: [
            Kirigami.Action {
                text: panel.i18nContext.i18nc("@action:button", "AI provider and privacy…")
                icon.name: "configure"
                onTriggered: panel.settingsClicked()
            }
        ]
    }

    Controls.Label {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.smallSpacing
        Layout.rightMargin: Kirigami.Units.smallSpacing
        wrapMode: Text.Wrap
        font: Kirigami.Theme.smallFont
        opacity: 0.8
        textFormat: Text.PlainText
        text: panel.i18nContext.i18nc("@info", "Answers come from an AI and can be wrong. Numbers in them are checked against your book. KassenSturz gives no financial, tax or legal advice.")
    }

    Controls.TabBar {
        id: tabs

        Layout.fillWidth: true

        Controls.TabButton {
            text: panel.i18nContext.i18nc("@title:tab the conversation", "Conversation")
        }

        Controls.TabButton {
            text: panel.i18nContext.i18nc("@title:tab what was asked and sent, see the glossary", "What the AI looked at")
        }
    }

    StackLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        currentIndex: tabs.currentIndex

        // The conversation.
        ListView {
            id: messageList

            clip: true
            spacing: Kirigami.Units.largeSpacing
            model: panel.messages

            Controls.ScrollBar.vertical: Controls.ScrollBar {}

            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 2
                visible: panel.messages.length === 0
                text: panel.i18nContext.i18nc("@info", "Ask a question about your book, for example how much you spent on food last month, or ask for help sorting your transactions.")
            }

            delegate: AiChatMessage {
                required property var modelData

                width: ListView.view.width
                ai: panel.ai
                message: modelData
                onReviewClicked: setId => panel.reviewChangesClicked(setId)
                onDiscardClicked: setId => {
                    panel.chat.discard(setId);
                    panel.markHandled(setId, false);
                }
                onSettingsClicked: panel.settingsClicked()
            }
        }

        // The tool-call log (D-192).
        ListView {
            id: logList

            clip: true
            spacing: Kirigami.Units.smallSpacing
            model: panel.logLines

            Controls.ScrollBar.vertical: Controls.ScrollBar {}

            header: Controls.Label {
                width: logList.width
                padding: Kirigami.Units.smallSpacing
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: panel.i18nContext.i18nc("@info", "Everything the AI was asked and everything that was sent to it, in order. It is kept in memory only and forgotten when you start a new conversation or close KassenSturz.")
            }

            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 2
                visible: panel.logLines.length === 0
                text: panel.i18nContext.i18nc("@info", "Nothing was asked yet.")
            }

            delegate: AiChatLogLine {
                required property var modelData

                width: ListView.view.width
                ai: panel.ai
                entry: modelData
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.smallSpacing
        visible: panel.chat.busy
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: panel.stopping
            ? panel.i18nContext.i18nc("@info", "Stopping… a request that is already on its way can't be called back.")
            : panel.i18nContext.i18nc("@info", "Waiting for the AI…")
    }

    SecretField {
        id: keyField

        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.smallSpacing
        Layout.rightMargin: Kirigami.Units.smallSpacing
        visible: panel.asksForKey
        enabled: !panel.chat.busy
        placeholderText: panel.i18nContext.i18nc("@info:placeholder", "The provider's key for this question")
        Accessible.name: panel.i18nContext.i18nc("@label:textbox", "The provider's key for this question")
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.smallSpacing

        Controls.TextField {
            id: questionField

            Layout.fillWidth: true
            enabled: !panel.chat.busy && panel.hasProvider
            placeholderText: panel.i18nContext.i18nc("@info:placeholder", "Ask about your book…")
            inputMethodHints: Qt.ImhNoPredictiveText
            Accessible.name: panel.i18nContext.i18nc("@label:textbox", "Your question for the AI")
            onAccepted: panel.ask()
        }

        Controls.Button {
            visible: !panel.chat.busy
            enabled: questionField.text.trim() !== "" && panel.hasProvider
            icon.name: "mail-send"
            text: panel.i18nContext.i18nc("@action:button send the question", "Ask")
            onClicked: panel.ask()
        }

        Controls.Button {
            visible: panel.chat.busy
            enabled: !panel.stopping
            icon.name: "process-stop"
            text: panel.i18nContext.i18nc("@action:button stop the question", "Stop")
            onClicked: {
                panel.stopping = true;
                panel.chat.stop();
            }
        }
    }
}
