// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The AI provider and the user's consent (M5.7, D-202 to D-205): where the
// AI runs (address and model), how its key is kept, a test of the
// connection, and the plain statement of what KassenSturz sends to a
// provider on another computer, which the user agrees to or takes back.
// Last, whether the user's own AI tools may prepare changes, which the user
// confirms in the chat sidebar (M5.10, D-219). Everything here is kept for
// this computer, never in a book (D-168). The page is hidden with the rest
// of AI while the switch is off (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AiSwitch ai
    required property AiSettings settings

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // What settings.load() found: the saved provider and consent.
    property var form: ({})
    // What the last save, key removal or test answered.
    property var saveResult: ({})
    property var keyResult: ({})
    property var testResult: ({})
    // The hint under the address: where it points, or why it can't be used.
    property var addressCheck: ({})
    // Whether something was saved just now, for the note under the buttons.
    property bool justSaved: false
    // Whether the settings file couldn't be written.
    property bool notWritten: false
    // Whether the kept key was removed just now.
    property bool keyRemoved: false
    // The example request of the consent dialog.
    property var sample: ({})

    // Whether the provider part of the form differs from what is saved.
    readonly property bool providerChanged: addressField.text.trim() !== (form.address ?? "")
        || modelField.text.trim() !== (form.model ?? "")
        || keyUseBox.currentValue !== (form.keyUse ?? "none")
    // Whether a provider is saved that can be used.
    readonly property bool hasProvider: (form.address ?? "") !== "" && (form.model ?? "") !== ""
        && (form.addressError ?? "") === ""

    // The sentence of the consent statement for one thing that is sent
    // (`settings.statement()`); a test holds this list to the statement.
    function statementText(word: string): string {
        switch (word) {
        case "questions":
            return i18nContext.i18nc("@info what is sent", "What you type into the chat.");
        case "book-data":
            return i18nContext.i18nc("@info what is sent", "Information from your open book that the AI asks for to answer you: the names of accounts and categories, balances and totals, transactions with the names of whoever was paid or paid you and the notes you wrote, and your Rules. Only as much as your question needs, never the whole book.");
        case "instructions":
            return i18nContext.i18nc("@info what is sent", "KassenSturz's own instructions to the AI, and the descriptions of the functions it may use to look things up or to prepare changes for you to confirm.");
        case "key":
            return i18nContext.i18nc("@info what is sent", "Your key for this provider, if it needs one, so that it accepts the request.");
        default:
            return "";
        }
    }

    // Reads what is saved into the page and the fields.
    function reload() {
        form = settings.load();
        addressField.text = form.address ?? "";
        modelField.text = form.model ?? "";
        keyUseBox.currentIndex = Math.max(0, keyUseBox.indexOfValue(form.keyUse ?? "none"));
        addressCheck = (form.address ?? "") !== "" ? settings.check(form.address) : ({});
    }

    function save() {
        justSaved = false;
        notWritten = false;
        keyRemoved = false;
        keyResult = ({});
        testResult = ({});
        const result = settings.save(addressField.text, modelField.text, keyUseBox.currentValue, keyField.text);
        if (result.saved === true) {
            keyField.text = "";
            keyField.hide();
            saveResult = ({});
            keyResult = (result.keyError ?? "") !== "" ? ({ code: result.keyError }) : ({});
            justSaved = true;
            reload();
            if (result.consentNeeded === true) {
                consentDialog.show();
            }
        } else if (result.code !== undefined) {
            saveResult = result;
        } else {
            saveResult = ({});
            notWritten = true;
        }
    }

    function removeKey() {
        keyResult = settings.forgetKey();
        justSaved = false;
        if (keyResult.ok === true) {
            keyResult = ({});
            keyRemoved = true;
        }
    }

    title: i18nContext.i18nc("@title", "AI provider and privacy")
    visible: ai.enabled
    Component.onCompleted: reload()

    Connections {
        target: page.settings

        function onTestFinished(result: var) {
            page.testResult = result;
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Information
            visible: page.settings.fake
            text: page.i18nContext.i18nc("@info", "Test mode: this copy of KassenSturz answers as a made-up model built into it. Nothing you do on this page, or ask later, is sent anywhere.")
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@info", "Choose the AI that answers your questions. It can run on this computer, for example with Ollama or LM Studio, or at a company that offers the OpenAI-compatible interface. These settings are kept on this computer only, never in your book.")
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.TextField {
                id: addressField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Address:")
                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                placeholderText: "http://localhost:11434/v1"
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
                onEditingFinished: page.addressCheck = text.trim() !== "" ? page.settings.check(text) : ({})
                onAccepted: modelField.forceActiveFocus()
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font: Kirigami.Theme.smallFont
                visible: page.addressCheck.place !== undefined
                text: page.addressCheck.place === "this"
                    ? page.i18nContext.i18nc("@info", "This address is on this computer. What you ask never leaves it.")
                    : page.i18nContext.i18nc("@info", "This address is on another computer, even if it is at home. Before anything is sent, you read what is sent and agree.")
            }

            Controls.TextField {
                id: modelField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Model:")
                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                placeholderText: page.i18nContext.i18nc("@info:placeholder the name of an AI model", "The model's name, as the provider calls it")
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
            }

            Controls.ComboBox {
                id: keyUseBox
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "The provider's key:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { value: "none", text: page.i18nContext.i18nc("@item:inlistbox how the provider's key is kept", "The provider needs no key") },
                    { value: "ask", text: page.i18nContext.i18nc("@item:inlistbox how the provider's key is kept", "I type it each time") },
                    { value: "remember", text: page.i18nContext.i18nc("@item:inlistbox how the provider's key is kept", "Keep it until I close KassenSturz") },
                    { value: "save", text: page.i18nContext.i18nc("@item:inlistbox how the provider's key is kept", "Save it with my other saved secrets") },
                ]
            }

            SecretField {
                id: keyField
                Kirigami.FormData.label: keyUseBox.currentValue === "ask"
                    ? page.i18nContext.i18nc("@label:textbox", "Key for the test:")
                    : page.i18nContext.i18nc("@label:textbox", "Key:")
                visible: keyUseBox.currentValue !== "none"
                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                placeholderText: page.i18nContext.i18nc("@info:placeholder", "Paste the key here")
                onAccepted: page.save()
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font: Kirigami.Theme.smallFont
                visible: keyUseBox.currentValue !== "none"
                text: {
                    switch (keyUseBox.currentValue) {
                    case "ask":
                        return page.i18nContext.i18nc("@info", "KassenSturz asks for the key whenever the AI needs it and forgets it at once. The key you type here is used for the test only.");
                    case "remember":
                        return page.i18nContext.i18nc("@info", "KassenSturz keeps the key in memory only, never in a file, and forgets it when you close it. The key is never shown again, here or anywhere.");
                    default:
                        return page.i18nContext.i18nc("@info", "KassenSturz saves the key where it saves bank PINs (see Password storage in Settings), never in your book or in a file of its own. The key is never shown again, here or anywhere. Leave the field empty to keep the saved key.");
                    }
                }
            }

            RowLayout {
                spacing: Kirigami.Units.largeSpacing

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Save")
                    icon.name: "document-save"
                    enabled: addressField.text.trim() !== "" && modelField.text.trim() !== ""
                        && (page.providerChanged || keyField.text !== "")
                    onClicked: page.save()
                }

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Test connection")
                    icon.name: "network-connect"
                    enabled: page.hasProvider && !page.providerChanged && !page.settings.testing
                    onClicked: {
                        page.testResult = ({});
                        page.justSaved = false;
                        page.keyRemoved = false;
                        page.settings.testConnection(keyField.text);
                    }
                }

                Controls.BusyIndicator {
                    running: page.settings.testing
                    visible: running
                }
            }

            Controls.Button {
                visible: page.settings.fake
                text: page.i18nContext.i18nc("@action:button", "Use the built-in test model")
                icon.name: "tools-wizard"
                onClicked: {
                    addressField.text = page.form.testAddress ?? "";
                    modelField.text = page.form.testModel ?? "";
                    keyUseBox.currentIndex = keyUseBox.indexOfValue("none");
                    page.addressCheck = page.settings.check(addressField.text);
                }
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                visible: page.providerChanged && page.hasProvider
                text: page.i18nContext.i18nc("@info", "Save first; then you can test the connection.")
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                visible: page.justSaved
                text: page.i18nContext.i18nc("@info", "Saved on this computer.")
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                visible: page.notWritten
                text: page.i18nContext.i18nc("@info", "KassenSturz couldn't save these settings. Check that your settings folder can be written to, then save again.")
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                visible: page.keyRemoved
                text: page.i18nContext.i18nc("@info", "The key was removed from this computer.")
            }
        }

        AiErrorMessage {
            Layout.fillWidth: true
            ai: page.ai
            result: page.saveResult
        }

        AiErrorMessage {
            Layout.fillWidth: true
            ai: page.ai
            result: page.keyResult
        }

        AiErrorMessage {
            Layout.fillWidth: true
            ai: page.ai
            result: page.testResult
        }

        // What a test found.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Positive
            visible: page.testResult.models !== undefined
            text: visible ? Plain.plain(page.i18nContext.i18ncp("@info %1 is a number", "Connected. The provider offers %1 model.", "Connected. The provider offers %1 models.", page.testResult.models.length)) : ""
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            visible: page.testResult.models !== undefined && page.testResult.modelOffered === false
            text: visible ? Plain.plain(page.i18nContext.i18nc("@info %1 is the name of a model", "The provider doesn't list the model “%1”. Choose one of its models below, or check the name.", page.form.model ?? "")) : ""
        }

        Controls.ComboBox {
            id: offeredModels
            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
            visible: page.testResult.models !== undefined && page.testResult.models.length > 0
            model: page.testResult.models ?? []
            displayText: page.i18nContext.i18nc("@item:inlistbox", "Use one of the provider's models…")
            onActivated: {
                modelField.text = currentText;
                page.testResult = ({});
            }
        }

        // The consent: what is sent, and whether the user agreed.
        Kirigami.Heading {
            level: 2
            visible: page.hasProvider
            text: page.i18nContext.i18nc("@title:group", "What is sent")
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            visible: page.hasProvider
            text: {
                switch (page.form.consent) {
                case "given":
                    return page.i18nContext.i18nc("@info %1 is a date", "You agreed on %1 to what KassenSturz sends to this provider. You can read it again, or take your agreement back; nothing is sent to this provider while it is withdrawn.", page.form.consentDay);
                case "missing":
                    return page.i18nContext.i18nc("@info", "This provider is on another computer. Nothing is sent to it until you have read what KassenSturz sends and agreed.");
                default:
                    return page.i18nContext.i18nc("@info", "This provider is on this computer, so what you ask never leaves it and there is nothing to agree to.");
                }
            }
        }

        RowLayout {
            visible: page.hasProvider && page.form.consent !== "notNeeded"
            spacing: Kirigami.Units.largeSpacing

            Controls.Button {
                text: page.form.consent === "given"
                    ? page.i18nContext.i18nc("@action:button", "Read it again…")
                    : page.i18nContext.i18nc("@action:button", "Read what is sent and agree…")
                icon.name: "view-readermode"
                onClicked: consentDialog.show()
            }

            Controls.Button {
                visible: page.form.consent === "given"
                text: page.i18nContext.i18nc("@action:button", "Take my agreement back")
                icon.name: "dialog-cancel"
                onClicked: {
                    page.notWritten = !page.settings.withdrawConsent();
                    page.reload();
                }
            }
        }

        // A key kept for the provider can be removed without changing the
        // provider.
        Controls.Button {
            visible: page.hasProvider && (page.form.keyUse ?? "none") !== "none" && (page.form.keyUse ?? "none") !== "ask"
            text: page.i18nContext.i18nc("@action:button", "Remove the key from this computer")
            icon.name: "edit-delete"
            onClicked: page.removeKey()
        }

        // The user's own AI tools (D-035, D-219): they can read the book
        // once AI is on; preparing changes is a second, separate yes.
        Kirigami.Heading {
            level: 2
            text: page.i18nContext.i18nc("@title:group", "Your own AI tools")
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@info", "AI programs on this computer that you set up to use KassenSturz can read your book. If you allow it, they can also prepare changes: new categories, Rules and categories for transactions. Their changes are shown in the chat sidebar, and nothing changes until you have read them and confirmed. KassenSturz must be open for that.")
        }

        Controls.Switch {
            id: writeToolsSwitch

            text: page.i18nContext.i18nc("@option:check", "Let my AI tools prepare changes")
            checked: page.settings.writeTools()
            onToggled: {
                page.notWritten = !page.settings.setWriteTools(checked);
                checked = page.settings.writeTools();
            }
        }
    }

    Kirigami.Dialog {
        id: consentDialog

        function show() {
            page.sample = ({});
            exampleShown.checked = false;
            leaveOut.text = "";
            open();
        }

        function agree() {
            page.notWritten = !page.settings.giveConsent();
            close();
            page.reload();
        }

        function showSample() {
            page.sample = page.settings.sample(leaveOut.text);
        }

        title: page.i18nContext.i18nc("@title:window", "What KassenSturz sends to this provider")
        padding: Kirigami.Units.largeSpacing
        preferredWidth: Kirigami.Units.gridUnit * 30
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "I agree")
                icon.name: "dialog-ok"
                onTriggered: consentDialog.agree()
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Not now")
                icon.name: "dialog-cancel"
                onTriggered: consentDialog.close()
            }
        ]

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@info %1 is a web address", "The provider at %1 is on another computer, even if that computer is in your home. When you ask the AI something, this leaves your computer and goes to it:", page.form.address ?? "")
            }

            Repeater {
                model: page.settings.statement()

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
                        text: page.statementText(item.modelData)
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@info", "Never sent: your bank logins, PINs and TANs, links to your bank, and the keys of other providers.")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@info", "What the provider does with what it receives is up to the provider; read its privacy policy if you are unsure. Before each request goes, you can see exactly what it holds and leave words out. You can take your agreement back here at any time.")
            }

            Controls.CheckBox {
                id: exampleShown
                text: page.i18nContext.i18nc("@option:check", "Show an example of a request")
                onToggled: {
                    if (checked) {
                        consentDialog.showSample();
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                visible: exampleShown.checked
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: page.i18nContext.i18nc("@info", "This is the first request for a made-up question, exactly as KassenSturz would build it. Nothing is sent now. When the AI asks for information from your book, that is added to the next request.")
                }

                Controls.TextArea {
                    id: exampleText
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 12
                    readOnly: true
                    wrapMode: Text.WrapAnywhere
                    textFormat: TextEdit.PlainText
                    font.family: "monospace"
                    text: page.sample.text ?? ""
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    Controls.TextField {
                        id: leaveOut
                        Layout.fillWidth: true
                        placeholderText: page.i18nContext.i18nc("@info:placeholder", "A word or phrase to leave out")
                        inputMethodHints: Qt.ImhNoPredictiveText
                        onAccepted: consentDialog.showSample()
                    }

                    Controls.Button {
                        text: page.i18nContext.i18nc("@action:button", "Leave it out")
                        enabled: leaveOut.text.trim() !== ""
                        onClicked: consentDialog.showSample()
                    }
                }

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    visible: page.sample.replaced !== undefined && leaveOut.text.trim() !== ""
                    text: page.i18nContext.i18ncp("@info %1 is a number", "Left out in %1 place.", "Left out in %1 places.", parseInt(page.sample.replaced ?? "0"))
                }

                AiErrorMessage {
                    Layout.fillWidth: true
                    ai: page.ai
                    result: page.sample.code !== undefined ? page.sample : ({})
                }
            }
        }
    }
}
