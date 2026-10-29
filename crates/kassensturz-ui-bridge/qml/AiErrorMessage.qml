// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Shows why something with the AI provider didn't work (M5.7). The Rust side
// only returns a code (D-034); every message is written here, says what
// happened and what to do next (AGENTS.md §6), and is hidden with the rest
// of AI while the switch is off (D-013). One message for each `ai.`,
// `ai-chat.` and `ai-changes.` code of `docs/ERROR_CODES.md`, which a test
// checks.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: message

    required property AiSwitch ai

    // What a call returned: empty, or the error's code and its parameters.
    property var result: ({})
    // Whether the message offers a button to the AI settings, for places
    // outside the settings page (the chat); only for errors that settings
    // can fix.
    property bool offerSettings: false
    // Whether the message offers to add `/v1` to the address, for the
    // settings page.
    property bool offerV1: false

    // The user wants the AI settings.
    signal settingsClicked()
    // The user wants `/v1` added to the provider's address.
    signal useV1Clicked()

    // Errors that a change in the settings can fix. Not the switch itself,
    // a request that was too large, or a provider that asks to wait.
    readonly property bool settingsHelp: result.code !== undefined
        && String(result.code).startsWith("ai.")
        && ["ai.off", "ai.settings-changed", "ai.request-too-large", "ai.rate-limited"].indexOf(result.code) < 0

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function messageFor(result: var): string {
        switch (result.code) {
        case "ai.off":
            return i18nContext.i18nc("@info", "AI features are switched off. Turn them on in Settings.");
        case "ai.no-provider":
            return i18nContext.i18nc("@info", "Type the provider's address and the model's name first.");
        case "ai.bad-address":
            return i18nContext.i18nc("@info %1 is an example web address", "This address can't be used. It starts with http:// or https://, has the server's name, and has no login name, password, ? or # part, for example %1.", "http://localhost:11434/v1");
        case "ai.plain-http":
            return i18nContext.i18nc("@info", "This address is on another computer and starts with http://, which isn't encrypted, so KassenSturz won't send anything to it. Use the provider's https:// address. Only a provider on this computer may use http://.");
        case "ai.no-consent":
            return i18nContext.i18nc("@info", "This provider is on another computer, and you haven't agreed yet to what KassenSturz sends to it. Nothing was sent. Read what is sent and agree below, or use a provider on this computer.");
        case "ai.settings-changed":
            return i18nContext.i18nc("@info", "The provider's settings changed after this request was prepared, so it wasn't sent. Look at the request again and send it.");
        case "ai.request-too-large":
            return i18nContext.i18nc("@info", "This request is too large to send at once. Ask about less at a time.");
        case "ai.key-needed":
            return i18nContext.i18nc("@info", "The provider needs a key. Type it in the Key field and save, or choose that it needs none.");
        case "ai.key-rejected":
            return i18nContext.i18nc("@info", "The provider refused the key. Check the key, and at the provider whether it is still valid, then type it again.");
        case "ai.keychain":
            return i18nContext.i18nc("@info", "The system keychain couldn't be reached, so the key couldn't be saved, read or removed. Make sure the keychain (for example KWallet) is unlocked, or choose to type the key each time.");
        case "ai.keepassxc":
            return i18nContext.i18nc("@info", "KeePassXC couldn't be reached, or it said no, so the key couldn't be saved, read or removed. Start KeePassXC, open and unlock your database, make sure Browser Integration is on, and try again; or choose to type the key each time.");
        case "ai.unreachable":
            return i18nContext.i18nc("@info", "The provider couldn't be reached. Check the address, that the provider is running (for a local server such as Ollama or LM Studio) and your internet connection.");
        case "ai.untrusted-server":
            return i18nContext.i18nc("@info", "The server's security certificate didn't pass the checks of this computer, so KassenSturz sent nothing to it. Check the address; a server at home needs a certificate this computer trusts.");
        case "ai.timeout":
            return i18nContext.i18nc("@info", "The provider took too long to answer. Try again; a local model may need a faster computer or a smaller model.");
        case "ai.redirect":
            return i18nContext.i18nc("@info", "The provider sent KassenSturz to another address, which KassenSturz doesn't follow. Enter the provider's final address.");
        case "ai.not-found":
            return i18nContext.i18nc("@info", "The provider doesn't know this address or this model. Check both.");
        case "ai.missing-v1":
            return i18nContext.i18nc("@info", "The provider didn't understand this address, but it answers when “/v1” is added to the end of it. Many providers, such as LM Studio and Ollama, serve their interface there.");
        case "ai.rate-limited":
            return i18nContext.i18nc("@info", "The provider asks you to wait before the next request. Try again in a little while.");
        case "ai.context-too-small":
            return i18nContext.i18nc("@info", "The model's context, its working memory, is too small for this question. In the program that runs the model, load it again with a larger context length (8192 or more; 16384 is better), or ask about less at a time.");
        case "ai.provider-failed":
            return i18nContext.i18nc("@info %1 is a number", "The provider answered with an error (HTTP %1). Try again later. If it keeps happening, check the provider's settings and the key: some providers report a wrong key this way.", result.status);
        case "ai.reply-too-large":
            return i18nContext.i18nc("@info", "The provider's answer was too large, so KassenSturz didn't read it. Ask about less at a time.");
        case "ai.bad-reply":
            return i18nContext.i18nc("@info", "The provider's answer isn't what KassenSturz expects from an OpenAI-compatible provider. Check that the address points to such an interface, often ending in /v1.");
        case "ai-chat.cancelled":
            return i18nContext.i18nc("@info", "You stopped the question. Ask again when you like.");
        case "ai-chat.stopped":
            return i18nContext.i18nc("@info", "Nothing was sent: you didn't let the request go. Ask again, and leave out what you don't want to send.");
        case "ai-chat.too-many-rounds":
            return i18nContext.i18nc("@info", "The AI kept asking for more data without answering, so KassenSturz stopped it. Ask a narrower question, or try another model.");
        case "ai-chat.tool-call-as-text":
            return i18nContext.i18nc("@info", "The model wrote a function call as plain text instead of making the call, so nothing was done. Ask again, or try another model: this one doesn't work well with KassenSturz's functions.");
        case "ai-chat.book-unreadable":
            return i18nContext.i18nc("@info", "Your book couldn't be read to answer the question. Make sure the book is still where it was, then ask again.");
        case "ai-changes.out-of-date":
            return i18nContext.i18nc("@info", "These changes are out of date: a transaction in them was changed or removed, or they were thrown away. Nothing was changed. Ask the AI to prepare them again.");
        case "ai-changes.not-allowed":
            return i18nContext.i18nc("@info", "These changes hold a kind of change that you have turned off in the AI settings since the AI prepared them. Nothing was changed. Turn it on again under “What the AI may do”, or ask the AI to prepare the changes again.");
        case "ai-changes.not-applied":
            return i18nContext.i18nc("@info", "Your book refused one of the changes, for example because a category of that name was added in the meantime. Nothing was changed. Ask the AI to prepare them again.");
        case "ai-changes.not-editable":
            return i18nContext.i18nc("@info", "This change can't be changed that way, so your change wasn't kept. Leave the change out, or take the AI's version.");
        case "ai-changes.edit-text":
            return i18nContext.i18nc("@info", "A name, title or text to look for can't be empty, can be at most 100 characters long (a chart's title 200) and can't hold line breaks or hidden characters. Your change wasn't kept; type it again.");
        case "ai-changes.edit-name-taken":
            return i18nContext.i18nc("@info", "That name is already used there. Your change wasn't kept; pick another name.");
        case "ai-changes.edit-in-use":
            return i18nContext.i18nc("@info", "Another change uses this new category, for example a Rule that books to it, so it can't be left out alone. Change or leave out that change first.");
        case "ai-changes.edit-refused":
            return i18nContext.i18nc("@info", "KassenSturz can't make the change this way, for example because the category is of another kind or currency, or the chart doesn't suit its figures. Your change wasn't kept; choose something else, or take the AI's version.");
        default:
            return i18nContext.i18nc("@info", "Something went wrong with the provider. Check its address and settings, then try again.");
        }
    }

    visible: ai.enabled

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        type: Kirigami.MessageType.Error
        visible: message.result.code !== undefined
        actions: [
            Kirigami.Action {
                text: message.i18nContext.i18nc("@action:button", "Add /v1 to the address")
                icon.name: "list-add"
                visible: message.offerV1 && message.result.code === "ai.missing-v1"
                onTriggered: message.useV1Clicked()
            },
            Kirigami.Action {
                text: message.i18nContext.i18nc("@action:button", "Open the AI settings")
                icon.name: "configure"
                visible: message.offerSettings && message.settingsHelp
                onTriggered: message.settingsClicked()
            }
        ]
        // An InlineMessage shows markup as markup: nothing from a provider
        // may look like it (D-143).
        text: visible ? Plain.plain(message.messageFor(message.result)) : ""
    }
}
