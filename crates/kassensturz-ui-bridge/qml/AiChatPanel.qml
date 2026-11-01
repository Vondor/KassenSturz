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
// first (`AiRequestReview`). Changes the user's own AI tools prepare arrive
// here too, looked for once a second, and are reviewed the same way (M5.10,
// D-218). Hidden with the rest of AI while the switch is off (D-013).

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
    // The user wants the chat in its own window, or back in the sidebar.
    signal detachClicked()
    // The user's own AI tools prepared changes: the sidebar should show.
    signal changesArrived()
    // The user wants the Statistic Builder, on the chart the AI prepared
    // (`component`, as JSON text, with its title and period), or after the
    // AI gave up on one: with only the `dataset` it understood (D-355).
    // The user wants a kept chart on the Overview (D-428).
    signal addToOverviewRequested(string component, string period)
    signal statisticBuilderRequested(string title, string component, string period, string dataset)
    // The AI asked to open the page `name` (`overview`, `accounts`,
    // `categories`, `rules` or `account`), about the account or Rule `id`
    // when it has one (D-265). It only shows a page; nothing is changed.
    signal openPageRequested(string name, string id)

    // The panel sits in a window of its own, not in the sidebar (D-250).
    property bool detached: false
    // The name of the dashboard the Overview shows, which a kept chart is
    // added to (M6d.1, D-438).
    property string dashboardName: ""

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The words other files borrow, so that none of them names the AI
    // itself while the switch is off (D-013).
    // The title of the window the chat can be detached into (D-250).
    readonly property string windowTitle: i18nContext.i18nc("@title:window", "Chat with the AI")

    readonly property string appliedName: i18nContext.i18nc("@info a change you made", "Applied changes the AI prepared")

    // The messages of the conversation, oldest first, and the tool-call log.
    // Lists with a model of their own, not arrays that are made again for
    // each new line: a list shown from a new array deletes every delegate and
    // builds them all again, and Kirigami's InlineMessage, which builds the
    // buttons of its actions a moment after it appears, was deleted before it
    // was done, which Kirigami reports as "Could not create delegate for
    // ToolBarLayout" (M5-B26).
    ListModel {
        id: messages

        dynamicRoles: true
    }

    ListModel {
        id: logLines

        dynamicRoles: true
    }
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
        features = ["categories", "month", "rules"].filter(feature => featureAllowed(feature));
        chartAllowed = settings.allows("AllowNewViews");
    }

    // Whether the user lets the AI prepare charts; the button that asks for
    // one is hidden, not grayed out, when not (D-239).
    property bool chartAllowed: false

    // *Describe a chart* (M6c.9, D-355): puts an example into the field for
    // the user to change, and asks nothing yet.
    function describeChart() {
        questionField.text = i18nContext.i18nc("@info:placeholder an example of how to describe a chart, which the user changes", "My spending on food each month this year, as bars");
        questionField.forceActiveFocus();
        questionField.selectAll();
    }

    // The built-in features that are shown: one whose changes the user
    // doesn't let the AI prepare is hidden, not grayed out (D-239).
    property var features: []

    function featureAllowed(feature: string): bool {
        switch (feature) {
        case "categories":
            return settings.allows("AllowSortTransactions") && !settings.locked("LockCategory");
        case "month":
            return settings.allows("AllowReadCategories") && settings.allows("AllowReadTransactions");
        case "rules":
            return settings.allows("AllowNewRules");
        default:
            return true;
        }
    }

    function add(message: var) {
        messages.append({ message: message });
        messageList.followEnd = true;
    }

    // The user dealt with the chart card of the set `setId`; the card is
    // made again from the model when the list is built again, and must then
    // not load a set that is gone.
    function markChart(setId: string, outcome: string) {
        for (let i = 0; i < messages.count; i++) {
            const message = messages.get(i).message;
            if (message.kind === "chart" && message.setId === setId) {
                messages.set(i, { message: { kind: "chart", setId: setId, unverified: message.unverified, outcome: outcome } });
            }
        }
    }

    // The review screen or the user's choice ended with a set.
    function markHandled(setId: string, applied: bool) {
        for (let i = 0; i < messages.count; i++) {
            if (messages.get(i).message.setId === setId) {
                messages.set(i, { message: { kind: "changesDone", applied: applied } });
            }
        }
    }

    function startOver() {
        chat.reset();
        messages.clear();
        logLines.clear();
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

    // What the button of a built-in feature (M5.9, D-222) shows in the
    // conversation as the user's question; the model gets the fixed
    // question of the feature.
    function featureLabel(feature: string): string {
        switch (feature) {
        case "categories":
            return i18nContext.i18nc("@action:button asks the AI to suggest categories for the transactions that have none", "Suggest categories");
        case "month":
            return i18nContext.i18nc("@action:button asks the AI to explain this month's money in and out", "Explain this month");
        default:
            return i18nContext.i18nc("@action:button asks the AI to prepare Rules for payees that come back", "Prepare Rules");
        }
    }

    // Asks the question of a built-in feature. `accountId` limits the
    // category suggestions to one account, empty for all of them. Called
    // by the buttons below and by the register's button.
    function askFeature(feature: string, accountId: string) {
        if (chat.busy || !hasProvider || !featureAllowed(feature)) {
            return;
        }
        const started = chat.askFeature(feature, accountId, keyField.text);
        if (started.started !== true) {
            if (started.code !== undefined) {
                add({ kind: "error", result: started });
            }
            return;
        }
        add({ kind: "question", text: featureLabel(feature) });
        keyField.text = "";
    }

    Component.onCompleted: refresh()

    visible: ai.enabled
    spacing: Kirigami.Units.smallSpacing

    // Changes from the user's own AI tools come in as request files; the
    // object looks only while the write tools are on too (D-218).
    Timer {
        interval: 1000
        repeat: true
        running: panel.ai.enabled && panel.chat.available
        onTriggered: panel.chat.checkRequests()
    }

    Connections {
        target: panel.chat

        function onLogged(entry: var) {
            logLines.append({ entry: entry });
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
                overview: result.overview,
                contextCut: result.contextCut === true
            });
            for (let i = 0; i < result.sets.length; i++) {
                panel.add({ kind: "changes", setId: result.sets[i].id, count: result.sets[i].count });
            }
            for (let i = 0; i < result.charts.length; i++) {
                panel.add({ kind: "chart", setId: result.charts[i].id, unverified: Array.from(result.charts[i].unverified), outcome: "" });
            }
            if (result.chartFailed !== undefined) {
                panel.add({ kind: "chartFailed", dataset: result.chartFailed.dataset });
            }
            if (result.page !== undefined) {
                panel.add({ kind: "page", page: result.page.name, hasId: result.page.id !== "" });
                panel.openPageRequested(result.page.name, result.page.id);
            }
            if (result.issue !== undefined) {
                panel.add({
                    kind: "issue",
                    title: result.issue.title,
                    body: result.issue.body,
                    link: result.issue.link,
                    wholeInLink: result.issue.wholeInLink
                });
            }
        }

        function onRequested(sets: var) {
            for (let i = 0; i < sets.length; i++) {
                panel.add({ kind: "changes", setId: sets[i].id, count: sets[i].count, fromTool: true });
            }
            panel.changesArrived();
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
            enabled: messages.count > 0 || logLines.count > 0
            text: panel.i18nContext.i18nc("@action:button forget the conversation and start a new one", "New conversation")
            display: Controls.AbstractButton.IconOnly
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: text
            Accessible.name: text
            onClicked: {
                ActionLog.taken("AiChatPanel_newConversation")
                panel.startOver()
            }
        }

        Controls.ToolButton {
            icon.name: panel.detached ? "window-pin" : "window-new"
            text: panel.detached
                ? panel.i18nContext.i18nc("@action:button put the chat back into the sidebar", "Move back to the sidebar")
                : panel.i18nContext.i18nc("@action:button show the chat in a window of its own", "Open in its own window")
            display: Controls.AbstractButton.IconOnly
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: text
            Accessible.name: text
            onClicked: {
                ActionLog.taken("AiChatPanel_detachClicked")
                panel.detachClicked()
            }
        }

        Controls.ToolButton {
            visible: !panel.detached
            icon.name: "window-close"
            text: panel.i18nContext.i18nc("@action:button close the chat sidebar", "Close the chat")
            display: Controls.AbstractButton.IconOnly
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: text
            Accessible.name: text
            onClicked: {
                ActionLog.taken("AiChatPanel_closeChat")
                panel.closeClicked()
            }
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
                onTriggered: {
                    ActionLog.taken("AiChatPanel_aiProviderPrivacy")
                    panel.settingsClicked()
                }
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

            // Whether the newest message stays in view. The delegates get
            // their height after they are created, so the end is looked for
            // again whenever the content or the list changes size; the user
            // scrolling up on purpose ends it until the next message.
            property bool followEnd: true

            function stickToEnd() {
                if (followEnd) {
                    Qt.callLater(() => positionViewAtEnd());
                }
            }

            clip: true
            spacing: Kirigami.Units.largeSpacing
            model: messages
            // A conversation is short: keeping every message built makes the
            // content height exact, so the scroll bar doesn't jump while the
            // list estimates the height of messages it hasn't built.
            cacheBuffer: 100000

            onContentHeightChanged: stickToEnd()
            onHeightChanged: stickToEnd()
            onAtYEndChanged: {
                if (moving) {
                    followEnd = atYEnd;
                }
            }

            Controls.ScrollBar.vertical: Controls.ScrollBar {
                id: messageBar
            }

            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 2
                visible: messages.count === 0
                text: panel.i18nContext.i18nc("@info", "Ask a question about your book, for example how much you spent on food last month, or ask for help sorting your transactions.")
            }

            delegate: AiChatMessage {
                // Beside the scroll bar, not under it.
                width: ListView.view.width - messageBar.width - Kirigami.Units.smallSpacing
                ai: panel.ai
                chat: panel.chat
                settings: panel.settings
                dashboardName: panel.dashboardName
                onChartChangeClicked: (title, component, period) => panel.statisticBuilderRequested(title, component, period, "")
                onBuilderClicked: dataset => panel.statisticBuilderRequested("", "", "", dataset)
                onChartAddToOverviewClicked: (component, period) => panel.addToOverviewRequested(component, period)
                onChartHandled: outcome => panel.markChart(message.setId, outcome)
                onReviewClicked: setId => panel.reviewChangesClicked(setId)
                onDiscardClicked: setId => {
                    panel.chat.discard(setId);
                    panel.markHandled(setId, false);
                }
                onSettingsClicked: panel.settingsClicked()
                onIssueUsed: how => panel.chat.issueUsed(how)
            }
        }

        // The tool-call log (D-192).
        ListView {
            id: logList

            clip: true
            spacing: Kirigami.Units.smallSpacing
            model: logLines

            Controls.ScrollBar.vertical: Controls.ScrollBar {
                id: logBar
            }

            header: Controls.Label {
                width: logList.width - logBar.width - Kirigami.Units.smallSpacing
                padding: Kirigami.Units.smallSpacing
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: panel.i18nContext.i18nc("@info", "Everything the AI was asked and everything that was sent to it, in order. It is kept in memory only and forgotten when you start a new conversation or close KassenSturz.")
            }

            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 2
                visible: logLines.count === 0
                text: panel.i18nContext.i18nc("@info", "Nothing was asked yet.")
            }

            delegate: AiChatLogLine {
                width: ListView.view.width - logBar.width - Kirigami.Units.smallSpacing
                ai: panel.ai
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.smallSpacing
        // Always laid out, so the list above doesn't change size, and its
        // scroll bar doesn't jump, each time the AI starts and stops thinking.
        Layout.minimumHeight: Kirigami.Units.gridUnit * 1.5
        opacity: panel.chat.busy ? 1 : 0
        Accessible.ignored: !panel.chat.busy
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: panel.stopping
            ? panel.i18nContext.i18nc("@info", "Stopping… a request that is already on its way can't be called back.")
            : panel.i18nContext.i18nc("@info", "Waiting for the AI…")
    }

    // The user chose "keep waiting" for a slow AI in the settings (D-246):
    // after `slowAfterSeconds` of waiting, a warning with a way out shows
    // above the input. With the other choice KassenSturz gives up by itself.
    readonly property int slowAfterSeconds: 60
    property bool keepsWaiting: false
    property int waitedSeconds: 0

    Timer {
        interval: 1000
        repeat: true
        running: panel.chat.busy
        onRunningChanged: {
            panel.waitedSeconds = 0;
            panel.keepsWaiting = running && panel.settings.slowAnswer() === "warn";
        }
        onTriggered: panel.waitedSeconds += 1
    }

    Kirigami.InlineMessage {
        id: slowWarning

        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.smallSpacing
        Layout.rightMargin: Kirigami.Units.smallSpacing
        type: Kirigami.MessageType.Warning
        visible: panel.chat.busy && panel.keepsWaiting && !panel.stopping && panel.waitedSeconds >= panel.slowAfterSeconds
        text: panel.i18nContext.i18nc("@info", "The AI is taking long to answer. A model on a slow computer can need minutes. You can keep waiting or cancel the question.")
        actions: [
            Kirigami.Action {
                icon.name: "dialog-cancel"
                text: panel.i18nContext.i18nc("@action:button cancel the question that takes long", "Cancel")
                onTriggered: {
                    ActionLog.taken("AiChatPanel_cancel")
                    panel.stopping = true;
                    panel.chat.stop();
                }
            }
        ]
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

    // The built-in features: one press asks a fixed question. What they
    // prepare is reviewed like anything else the AI prepares (D-222).
    Flow {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.smallSpacing
        Layout.rightMargin: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing
        visible: panel.hasProvider

        Repeater {
            model: panel.features

            Controls.Button {
                required property string modelData

                flat: true
                enabled: !panel.chat.busy
                text: panel.featureLabel(modelData)
                onClicked: {
                    ActionLog.taken("AiChatPanel_askFeature")
                    panel.askFeature(modelData, "")
                }
            }
        }

        Controls.Button {
            flat: true
            visible: panel.chartAllowed
            enabled: !panel.chat.busy
            icon.name: "view-statistics"
            text: panel.i18nContext.i18nc("@action:button puts an example of how to describe a chart into the question field", "Describe a chart")
            onClicked: {
                ActionLog.taken("AiChatPanel_describeChart")
                panel.describeChart()
            }
        }
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
            onAccepted: {
                ActionLog.taken("AiChatPanel_questionAi")
                panel.ask()
            }
        }

        Controls.Button {
            visible: !panel.chat.busy
            enabled: questionField.text.trim() !== "" && panel.hasProvider
            icon.name: "mail-send"
            text: panel.i18nContext.i18nc("@action:button send the question", "Ask")
            onClicked: {
                ActionLog.taken("AiChatPanel_ask")
                panel.ask()
            }
        }

        Controls.Button {
            visible: panel.chat.busy
            enabled: !panel.stopping
            icon.name: "process-stop"
            text: panel.i18nContext.i18nc("@action:button stop the question", "Stop")
            onClicked: {
                ActionLog.taken("AiChatPanel_stop")
                panel.stopping = true;
                panel.chat.stop();
            }
        }
    }
}
