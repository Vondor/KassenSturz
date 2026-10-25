// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Shows why something with the AI provider didn't work (M5.7). The Rust side
// only returns a code (D-034); every message is written here, says what
// happened and what to do next (AGENTS.md §6), and is hidden with the rest
// of AI while the switch is off (D-013). One message for each `ai.` code of
// `docs/ERROR_CODES.md`, which a test checks.

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
        case "ai.rate-limited":
            return i18nContext.i18nc("@info", "The provider asks you to wait before the next request. Try again in a little while.");
        case "ai.provider-failed":
            return i18nContext.i18nc("@info %1 is a number", "The provider answered with an error (HTTP %1). Try again later; if it keeps happening, check the provider's settings.", result.status);
        case "ai.reply-too-large":
            return i18nContext.i18nc("@info", "The provider's answer was too large, so KassenSturz didn't read it. Ask about less at a time.");
        case "ai.bad-reply":
            return i18nContext.i18nc("@info", "The provider's answer isn't what KassenSturz expects from an OpenAI-compatible provider. Check that the address points to such an interface, often ending in /v1.");
        default:
            return i18nContext.i18nc("@info", "Something went wrong with the provider. Check its address and settings, then try again.");
        }
    }

    visible: ai.enabled

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        type: Kirigami.MessageType.Error
        visible: message.result.code !== undefined
        // An InlineMessage shows markup as markup: nothing from a provider
        // may look like it (D-143).
        text: visible ? Plain.plain(message.messageFor(message.result)) : ""
    }
}
