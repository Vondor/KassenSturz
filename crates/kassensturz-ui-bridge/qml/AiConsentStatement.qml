// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The plain statement of what KassenSturz sends to a provider on another
// computer (D-204, AGENTS.md §8): the provider's address, each thing that is
// sent, and what is never sent. The consent dialog of the AI settings shows
// it, and so does the chat when a question was held back because the user
// hasn't agreed yet (B-314), so the user reads the same words in both places
// before agreeing. Hidden with the rest of AI while the switch is off
// (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: statement

    required property AiSwitch ai
    required property AiSettings settings
    // What settings.load() found: the provider's address, and whether the
    // connection is unencrypted and to which network.
    property var form: settings.load()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The sentence of the consent statement for one thing that is sent
    // (`settings.statement()`); a test holds this list to the statement.
    function statementText(word: string): string {
        switch (word) {
        case "questions":
            return i18nContext.i18nc("@info what is sent", "What you type into the chat.");
        case "book-data":
            return i18nContext.i18nc("@info what is sent", "Information from your open book that the AI asks for to answer you: the names of accounts and categories, balances and totals, transactions with the names of whoever was paid or paid you and the notes you wrote, and your Rules. Only as much as your question needs, never the whole book.");
        case "notes":
            return i18nContext.i18nc("@info what is sent", "The notes you wrote for the AI in these settings, if you wrote any. They go with every question.");
        case "instructions":
            return i18nContext.i18nc("@info what is sent", "KassenSturz's own instructions to the AI, and the descriptions of the functions it may use to look things up or to prepare changes for you to confirm.");
        case "key":
            return i18nContext.i18nc("@info what is sent", "Your key for this provider, if it needs one, so that it accepts the request.");
        default:
            return "";
        }
    }

    visible: ai.enabled
    spacing: Kirigami.Units.largeSpacing

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: statement.i18nContext.i18nc("@info %1 is a web address", "The provider at %1 is on another computer, even if that computer is in your home. When you ask the AI something, this leaves your computer and goes to it:", statement.form.address ?? "")
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: statement.form.unencrypted === true
        type: statement.form.network === "home" ? Kirigami.MessageType.Warning : Kirigami.MessageType.Error
        text: statement.i18nContext.i18nc("@info", "You allowed an unencrypted connection, so all of this travels without encryption and others on the way could read it.")
    }

    Repeater {
        model: statement.settings.statement()

        delegate: RowLayout {
            id: item

            required property string modelData

            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.alignment: Qt.AlignTop
                text: "•"
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: statement.statementText(item.modelData)
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: statement.i18nContext.i18nc("@info", "Never sent: your bank logins, PINs and TANs, links to your bank, and the keys of other providers.")
    }
}
