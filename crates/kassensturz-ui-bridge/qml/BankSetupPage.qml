// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Connects KassenSturz to a bank (M4.6, Q-035, D-145, D-151): the user
// types the IBAN of one of their accounts, KassenSturz finds the bank from
// it and asks only what that bank needs (the login name, a customer ID
// only where the bank gives one, the PIN and how long to keep it, D-057).
// Where the bank lists lack something, the user fills it in by hand
// (`docs/BANK_LIST.md`). While KassenSturz connects, the bank may ask for
// a TAN or an approval in its app (BankDialogs.qml). At the end the bank's
// accounts are linked to accounts in the book (D-148), so their
// transactions can be fetched into them.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property BankConnection bank

    // "iban", "details", "connecting" or "accounts".
    property string step: "iban"
    // What lookUp() returned for the IBAN.
    property var lookup: ({})
    // What linkChoices() returned after the setup.
    property var choices: ({})

    // The book changed: the accounts were linked.
    signal linked()
    signal done()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    BankAccountNames {
        id: accountNames
    }

    readonly property bool canContinue: ["ready", "unknown-bank", "no-server", "guessed-server", "unknown-login-kind"].includes(lookup.state ?? "")

    title: i18nContext.i18nc("@title", "Connect a bank")
    header: ColumnLayout {
        spacing: 0

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            position: Kirigami.InlineMessage.Position.Header
            type: Kirigami.MessageType.Information
            visible: page.bank.fake
            text: page.i18nContext.i18nc("@info", "Trying it out: a made-up bank stands in for yours. Its PIN is 12345 and its TAN 123456; nothing reaches a real bank.")
        }

        FormError {
            id: error
            Layout.fillWidth: true
        }
    }

    function findBank() {
        const result = bank.lookUp(ibanField.text);
        error.result = result.code !== undefined ? result : ({});
        if (result.code !== undefined) {
            return;
        }
        lookup = result;
        urlField.text = result.url;
        if (canContinue) {
            step = "details";
            loginField.forceActiveFocus();
        }
    }

    function connect() {
        const result = bank.setUp({
            iban: lookup.iban,
            url: urlField.text,
            loginName: loginField.text,
            customerId: customerField.text,
            customerIdRule: lookup.customerId,
            pin: pinField.text,
            pinMode: pinMode.currentValue,
        });
        error.result = result;
        if (result.code === undefined) {
            step = "connecting";
        }
    }

    function link() {
        const ids = [];
        for (let index = 0; index < accountChoices.count; ++index) {
            ids.push(accountChoices.itemAt(index).chosenId);
        }
        const result = bank.link(ids);
        error.result = result;
        if (result.code === undefined) {
            if (result.linked > 0) {
                linked();
            }
            done();
        }
    }

    Connections {
        target: page.bank

        function onSetupFinished(result: var) {
            pinField.text = "";
            error.result = result.code !== undefined ? result : ({});
            if (result.code !== undefined) {
                page.step = "details";
                return;
            }
            const choices = page.bank.linkChoices();
            page.choices = choices.code === undefined ? choices : ({});
            error.result = choices.code !== undefined ? choices : ({});
            page.step = "accounts";
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        // Step 1: the IBAN.
        Kirigami.FormLayout {
            Layout.fillWidth: true
            visible: page.step === "iban"

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                text: page.i18nContext.i18nc("@info", "KassenSturz can fetch your transactions from your bank, so you don't have to type them. It only reads; it never sends money. Start with the IBAN of one of your accounts at the bank: you find it on your bank card or in your bank's app.")
            }

            Controls.TextField {
                id: ibanField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "IBAN:")
                placeholderText: page.i18nContext.i18nc("@info:placeholder an example IBAN", "For example DE89 3704 0044 0532 0130 00")
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhPreferUppercase
                onAccepted: page.findBank()
                onTextEdited: page.lookup = ({})
            }

            Kirigami.InlineMessage {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                type: page.lookup.state === "chip-card-only" ? Kirigami.MessageType.Warning : Kirigami.MessageType.Information
                visible: page.lookup.state === "no-fints" || page.lookup.state === "chip-card-only"
                text: page.lookup.state === "no-fints"
                    ? page.i18nContext.i18nc("@info", "Banks outside Germany can't be connected yet: the system KassenSturz uses for this (FinTS) is a German one. Import the files your bank offers instead.")
                    : page.i18nContext.i18nc("@info", "This bank offers logins with a chip card only, which KassenSturz can't set up yet. Import the files your bank offers instead.")
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Find my bank")
                icon.name: "go-next"
                enabled: ibanField.text.trim().length > 0
                onClicked: page.findBank()
            }
        }

        // Step 2: what this bank needs.
        Kirigami.FormLayout {
            Layout.fillWidth: true
            visible: page.step === "details" || page.step === "connecting"
            enabled: page.step === "details"

            Controls.Label {
                Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Bank:")
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                // From the bank lists: plain text (D-143).
                textFormat: Text.PlainText
                text: (page.lookup.bankName ?? "") !== ""
                    ? page.lookup.bankName
                    : page.i18nContext.i18nc("@info %1 is a bank code", "Bank code %1", page.lookup.bankCode ?? "")
            }

            Controls.Label {
                Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Account:")
                textFormat: Text.PlainText
                text: page.lookup.iban ?? ""
            }

            Kirigami.InlineMessage {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                type: Kirigami.MessageType.Information
                visible: page.lookup.state !== undefined && page.lookup.state !== "ready"
                text: {
                    switch (page.lookup.state) {
                    case "unknown-bank":
                        return page.i18nContext.i18nc("@info", "KassenSturz's bank lists don't know this bank yet. If your bank offers FinTS (some call it HBCI), type the address it gives for it below; your bank's website or support can tell you.");
                    case "no-server":
                        return page.i18nContext.i18nc("@info", "KassenSturz's bank lists don't have this bank's FinTS address. If your bank offers FinTS (some call it HBCI), type the address it gives for it below; your bank's website or support can tell you.");
                    case "guessed-server":
                        return page.i18nContext.i18nc("@info", "KassenSturz's bank lists don't have this bank's FinTS address. Banks of your bank's kind nearby use the address below, so it is probably yours, but this is a guess. If connecting fails, check the address on your bank's website or ask its support, and type the right one.");
                    case "unknown-login-kind":
                        return page.i18nContext.i18nc("@info", "KassenSturz's bank lists don't say how you log in at this bank. KassenSturz tries a login with a PIN and a TAN, which most banks use.");
                    default:
                        return "";
                    }
                }
            }

            Controls.TextField {
                id: urlField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox the bank's FinTS server address", "FinTS address:")
                visible: page.lookup.askUrl ?? false
                placeholderText: "https://"
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
            }

            Controls.TextField {
                id: loginField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Login name:")
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                onAccepted: customerField.visible ? customerField.forceActiveFocus() : pinField.forceActiveFocus()
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
                text: page.i18nContext.i18nc("@info", "The name or number you log in to your bank's website with. Banks call it differently, for example login name, user ID or customer number.")
            }

            Controls.TextField {
                id: customerField
                Kirigami.FormData.label: page.lookup.customerId === "required"
                    ? page.i18nContext.i18nc("@label:textbox", "Customer ID:")
                    : page.i18nContext.i18nc("@label:textbox", "Customer ID (if any):")
                visible: page.lookup.customerId !== "none"
                placeholderText: page.lookup.customerId === "required"
                    ? ""
                    : page.i18nContext.i18nc("@info:placeholder", "Only if your bank gave you one")
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                onAccepted: pinField.forceActiveFocus()
            }

            Kirigami.PasswordField {
                id: pinField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "PIN:")
                onAccepted: page.connect()
            }

            Controls.ComboBox {
                id: pinMode
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Remember the PIN:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { value: "ask", text: page.i18nContext.i18nc("@item:inlistbox keep the PIN", "No, ask me each time") },
                    { value: "remember", text: page.i18nContext.i18nc("@item:inlistbox keep the PIN", "Until I close KassenSturz") },
                    { value: "save", text: page.i18nContext.i18nc("@item:inlistbox keep the PIN", "Save it in my keychain") },
                ]
            }

            Controls.Label {
                textFormat: Text.PlainText
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
                text: {
                    switch (pinMode.currentValue) {
                    case "remember":
                        return page.i18nContext.i18nc("@info", "KassenSturz keeps the PIN in memory only, never in a file, and forgets it when you close it.");
                    case "save":
                        return page.i18nContext.i18nc("@info", "KassenSturz saves the PIN in your system's keychain (for example KWallet), not in your book. You can remove it in Settings at any time.");
                    default:
                        return page.i18nContext.i18nc("@info", "KassenSturz forgets the PIN as soon as it has talked to your bank. This is the safest choice.");
                    }
                }
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Connect")
                icon.name: "network-connect"
                enabled: page.step === "details"
                onClicked: page.connect()
            }
        }

        // While KassenSturz talks to the bank.
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            visible: page.step === "connecting"
            spacing: Kirigami.Units.largeSpacing

            Controls.BusyIndicator {
                running: page.step === "connecting"
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                wrapMode: Text.Wrap
                text: page.i18nContext.i18nc("@info", "Talking to your bank… This can take a minute. Your bank may ask for a TAN, or for your approval in its app.")
            }
        }

        // Step 3: link the bank's accounts to the book's. A plain column,
        // since a FormLayout would put the repeated rows after the button.
        ColumnLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
            visible: page.step === "accounts"
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: page.i18nContext.i18nc("@info", "Connected. Your bank shows these accounts. Choose which account in KassenSturz each one fills, so its transactions can be fetched into it.")
            }

            Repeater {
                id: accountChoices

                model: page.choices.found ?? []

                delegate: ColumnLayout {
                    id: choice

                    required property var modelData

                    // The book account chosen, or empty for none.
                    readonly property string chosenId: box.currentValue ?? ""

                    Layout.fillWidth: true

                    // The bank's name and number of the account: plain
                    // text (D-143).
                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        font.bold: true
                        text: {
                            const what = accountNames.name(choice.modelData, page.bank.fake);
                            const number = choice.modelData.iban !== "" ? choice.modelData.iban : choice.modelData.number;
                            return number !== "" ? page.i18nContext.i18nc("@label %1 is the bank's name of an account, %2 its IBAN or number", "%1, %2", what, number) : what;
                        }
                    }

                    Controls.ComboBox {
                        id: box

                        Layout.fillWidth: true
                        Accessible.name: page.i18nContext.i18nc("@label:listbox", "Account in KassenSturz")
                        textRole: "text"
                        valueRole: "value"
                        model: [{ value: "", text: page.i18nContext.i18nc("@item:inlistbox", "Don't fill an account from it") }].concat(
                            Array.from(page.choices.accounts ?? []).map(account => ({ value: account.id, text: account.name })))
                        Component.onCompleted: currentIndex = Math.max(0, indexOfValue(choice.modelData.suggested))
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
                visible: page.choices.accounts !== undefined && page.choices.accounts.length === 0
                text: page.i18nContext.i18nc("@info", "Your book has no account yet that the bank could fill. Add your accounts first; you can link them to the bank's accounts when you get new transactions.")
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Done")
                icon.name: "dialog-ok"
                onClicked: page.link()
            }
        }
    }
}
