// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Per-computer configuration for built-in and custom quick-prompt buttons in
// the AI chat panel (US11.3, #1452, D-1013). Kept in `kassensturzrc` (`[AI]`),
// never in the book (D-892), and hidden while AI is off (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: root

    required property AiSwitch ai
    required property AiSettings settings

    // Whether the last setting write succeeded, for AiSettingsPage's banner.
    signal saved(bool ok)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Current states of the four built-in chat buttons, loaded from settings.
    property var builtinStates: ({
        categories: true,
        month: true,
        rules: true,
        describe_chart: true
    })
    // User-defined custom quick-prompt buttons in display order.
    property var customButtons: []
    // Validation limits from Rust.
    property var limits: ({ maxCount: 20, maxLabelChars: 60, maxPromptChars: 2000 })
    // Whether both receipt inbox and archive folders are configured.
    property bool inboxConfigured: false

    // Dialog editor state.
    property string editingId: ""
    property bool editingEnabled: true
    property string editorError: ""

    readonly property var builtinButtons: [
        {
            word: "categories",
            label: root.i18nContext.i18nc("@option:check built-in chat button", "Suggest categories")
        },
        {
            word: "month",
            label: root.i18nContext.i18nc("@option:check built-in chat button", "Explain this month")
        },
        {
            word: "rules",
            label: root.i18nContext.i18nc("@option:check built-in chat button", "Prepare Rules")
        },
        {
            word: "describe_chart",
            label: root.i18nContext.i18nc("@option:check built-in chat button", "Describe a chart")
        }
    ]

    function refreshInboxConfigured() {
        if (!ai.enabled) {
            inboxConfigured = false;
            return;
        }
        inboxConfigured = settings.receiptFolder("inbox") !== "" && settings.receiptFolder("archive") !== "";
    }

    function reload() {
        if (!ai.enabled) {
            builtinStates = ({
                categories: false,
                month: false,
                rules: false,
                describe_chart: false
            });
            customButtons = [];
            inboxConfigured = false;
            return;
        }
        const rawLimits = settings.chatButtonLimits();
        if (rawLimits !== undefined && rawLimits.maxCount !== undefined && rawLimits.maxCount > 0) {
            limits = rawLimits;
        }
        builtinStates = ({
            categories: settings.builtinChatButton("categories"),
            month: settings.builtinChatButton("month"),
            rules: settings.builtinChatButton("rules"),
            describe_chart: settings.builtinChatButton("describe_chart")
        });
        customButtons = settings.customChatButtons();
        refreshInboxConfigured();
    }

    function openAddDialog() {
        editingId = "";
        editingEnabled = true;
        editorError = "";
        labelField.text = "";
        promptField.text = "";
        insertOnlyBox.checked = false;
        editorDialog.open();
        labelField.forceActiveFocus();
    }

    function openEditDialog(button: var) {
        editingId = button.id ?? "";
        editingEnabled = button.enabled === true;
        editorError = "";
        labelField.text = button.label ?? "";
        promptField.text = button.prompt ?? "";
        insertOnlyBox.checked = button.insertOnly === true;
        editorDialog.open();
        labelField.forceActiveFocus();
    }

    function reasonText(reason: string): string {
        switch (reason) {
        case "invalid-label":
            return root.i18nContext.i18nc("@info %1 is the maximum label length", "Please enter a single-line button label (up to %1 characters).", root.limits.maxLabelChars);
        case "invalid-prompt":
            return root.i18nContext.i18nc("@info %1 is the maximum prompt length", "Please enter a prompt (up to %1 characters).", root.limits.maxPromptChars);
        case "limit":
            return root.i18nContext.i18nc("@info %1 is the maximum number of custom buttons", "You have reached the limit of %1 custom buttons. Delete one before adding another.", root.limits.maxCount);
        default:
            return root.i18nContext.i18nc("@info", "The button couldn't be saved. Check that your settings folder can be written to, then try again.");
        }
    }

    function saveEditor() {
        const result = editingId === ""
            ? settings.addCustomChatButton(labelField.text, promptField.text, insertOnlyBox.checked)
            : settings.updateCustomChatButton(editingId, labelField.text, promptField.text, editingEnabled, insertOnlyBox.checked);
        if (result.ok === true) {
            editorError = "";
            editorDialog.close();
            saved(true);
            reload();
        } else {
            editorError = reasonText(result.reason ?? "");
            saved(result.reason !== "not-written");
        }
    }

    function addInboxPreset() {
        const label = root.i18nContext.i18nc("@action:button default label for the inbox filing quick button", "File 5 inbox bills");
        const prompt = root.i18nContext.i18nc("@info default prompt for the inbox filing quick button", "Check my receipt inbox and prepare filing proposals for up to 5 bills. Match each bill to its transaction, suggest splitting if a bill covers more than one category, and keep any existing notes.");
        const result = settings.addCustomChatButton(label, prompt, false);
        saved(result.ok === true);
        reload();
    }

    visible: ai.enabled
    spacing: Kirigami.Units.smallSpacing
    Component.onCompleted: reload()

    Connections {
        target: root.settings

        function onChatButtonsChanged() {
            root.reload();
        }
    }

    Connections {
        target: root.ai

        function onEnabledChanged() {
            root.reload();
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: root.i18nContext.i18nc("@info", "Choose which built-in buttons appear in the chat, and add your own quick-prompt buttons. These buttons are kept on this computer only, never in your book.")
    }

    Kirigami.Heading {
        level: 3
        textFormat: Text.PlainText
        text: root.i18nContext.i18nc("@title:group", "Built-in buttons")
    }

    Repeater {
        model: root.builtinButtons

        delegate: Controls.Switch {
            required property var modelData

            objectName: "builtinButtonSwitch_" + modelData.word
            text: Plain.plain(modelData.label)
            checked: root.builtinStates[modelData.word] === true
            onToggled: {
                ActionLog.taken("AiChatButtonsSettings_toggleBuiltin")
                const ok = root.settings.setBuiltinChatButton(modelData.word, checked);
                root.saved(ok);
                root.reload();
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        text: root.i18nContext.i18nc("@info", "Built-in buttons only appear in the chat when the permissions above also allow them.")
    }

    Kirigami.Heading {
        Layout.topMargin: Kirigami.Units.smallSpacing
        level: 3
        textFormat: Text.PlainText
        text: root.i18nContext.i18nc("@title:group", "Your own buttons")
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font.italic: true
        visible: root.customButtons.length === 0
        text: root.i18nContext.i18nc("@info", "No custom buttons yet. Add one for a question you ask often, or for a template you edit before sending.")
    }

    Repeater {
        model: root.customButtons

        delegate: RowLayout {
            id: buttonRow

            required property var modelData
            required property int index

            objectName: "customButtonRow_" + modelData.id
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Controls.Switch {
                objectName: "customButtonSwitch_" + buttonRow.modelData.id
                checked: buttonRow.modelData.enabled === true
                Accessible.name: Plain.plain(root.i18nContext.i18nc("@option:check %1 is a custom button label", "Show button \"%1\" in the chat", buttonRow.modelData.label))
                onToggled: {
                    ActionLog.taken("AiChatButtonsSettings_toggleCustom")
                    const ok = root.settings.setCustomChatButtonEnabled(buttonRow.modelData.id, checked);
                    root.saved(ok);
                    root.reload();
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                Controls.Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.bold: true
                    textFormat: Text.PlainText
                    text: buttonRow.modelData.label
                }

                Controls.Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font: Kirigami.Theme.smallFont
                    opacity: 0.8
                    textFormat: Text.PlainText
                    text: buttonRow.modelData.insertOnly === true
                        ? root.i18nContext.i18nc("@info %1 is the prompt text", "Inserts into input box — %1", buttonRow.modelData.prompt.replace(/\s+/g, " "))
                        : root.i18nContext.i18nc("@info %1 is the prompt text", "Sends immediately — %1", buttonRow.modelData.prompt.replace(/\s+/g, " "))
                }
            }

            Controls.ToolButton {
                objectName: "moveUpButton_" + buttonRow.modelData.id
                icon.name: "go-up"
                enabled: buttonRow.index > 0
                display: Controls.AbstractButton.IconOnly
                text: root.i18nContext.i18nc("@action:button move custom chat button up", "Move up")
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: text
                Accessible.name: text
                onClicked: {
                    ActionLog.taken("AiChatButtonsSettings_moveUp")
                    const ok = root.settings.moveCustomChatButton(buttonRow.modelData.id, -1);
                    root.saved(ok);
                    root.reload();
                }
            }

            Controls.ToolButton {
                objectName: "moveDownButton_" + buttonRow.modelData.id
                icon.name: "go-down"
                enabled: buttonRow.index < root.customButtons.length - 1
                display: Controls.AbstractButton.IconOnly
                text: root.i18nContext.i18nc("@action:button move custom chat button down", "Move down")
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: text
                Accessible.name: text
                onClicked: {
                    ActionLog.taken("AiChatButtonsSettings_moveDown")
                    const ok = root.settings.moveCustomChatButton(buttonRow.modelData.id, 1);
                    root.saved(ok);
                    root.reload();
                }
            }

            Controls.Button {
                objectName: "editButton_" + buttonRow.modelData.id
                flat: true
                icon.name: "document-edit"
                text: root.i18nContext.i18nc("@action:button edit a custom chat button", "Edit")
                onClicked: {
                    ActionLog.taken("AiChatButtonsSettings_editCustom")
                    root.openEditDialog(buttonRow.modelData)
                }
            }

            Controls.Button {
                objectName: "deleteButton_" + buttonRow.modelData.id
                flat: true
                icon.name: "edit-delete"
                text: root.i18nContext.i18nc("@action:button delete a custom chat button", "Delete")
                onClicked: {
                    ActionLog.taken("AiChatButtonsSettings_deleteCustom")
                    const ok = root.settings.removeCustomChatButton(buttonRow.modelData.id);
                    root.saved(ok);
                    root.reload();
                }
            }
        }
    }

    Flow {
        Layout.fillWidth: true
        Layout.topMargin: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        Controls.Button {
            objectName: "addCustomButton"
            icon.name: "list-add"
            enabled: root.customButtons.length < root.limits.maxCount
            text: root.i18nContext.i18nc("@action:button add a custom chat quick-prompt button", "Add button…")
            onClicked: {
                ActionLog.taken("AiChatButtonsSettings_addCustom")
                root.openAddDialog()
            }
        }

        Controls.Button {
            objectName: "addInboxPresetButton"
            flat: true
            icon.name: "mail-folder-inbox"
            visible: root.inboxConfigured
            enabled: root.customButtons.length < root.limits.maxCount
            text: root.i18nContext.i18nc("@action:button add a ready-made button for filing bills from the receipt inbox", "Add inbox filing button")
            onClicked: {
                ActionLog.taken("AiChatButtonsSettings_addInboxPreset")
                root.addInboxPreset()
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        visible: root.customButtons.length >= root.limits.maxCount
        text: root.i18nContext.i18nc("@info %1 is the maximum number of custom buttons", "You have reached the limit of %1 custom buttons. Delete one before adding another.", root.limits.maxCount)
    }

    Kirigami.Dialog {
        id: editorDialog
        objectName: "customButtonDialog"

        title: Plain.plain(root.editingId === ""
            ? root.i18nContext.i18nc("@title:window", "Add chat button")
            : root.i18nContext.i18nc("@title:window", "Edit chat button"))
        padding: Kirigami.Units.largeSpacing
        preferredWidth: Kirigami.Units.gridUnit * 28
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: root.i18nContext.i18nc("@action:button save custom chat button", "Save")
                icon.name: "dialog-ok"
                enabled: labelField.text.trim() !== "" && promptField.text.trim() !== ""
                onTriggered: {
                    ActionLog.taken("AiChatButtonsSettings_saveCustom")
                    root.saveEditor()
                }
            },
            Kirigami.Action {
                text: root.i18nContext.i18nc("@action:button cancel custom chat button edit", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("AiChatButtonsSettings_cancelCustom")
                    editorDialog.close()
                }
            }
        ]

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                textFormat: Text.PlainText
                text: root.i18nContext.i18nc("@label:textbox", "Button label:")
            }

            Controls.TextField {
                id: labelField
                objectName: "customButtonLabelField"

                Layout.fillWidth: true
                placeholderText: root.i18nContext.i18nc("@info:placeholder", "For example: Weekly spending summary")
                Accessible.name: root.i18nContext.i18nc("@label:textbox", "Button label")
                onTextChanged: if (length > root.limits.maxLabelChars) remove(root.limits.maxLabelChars, length)
            }

            Controls.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                font: Kirigami.Theme.smallFont
                text: root.i18nContext.i18nc("@info %1 characters written, %2 the most allowed", "%1 of %2 characters", labelField.length, root.limits.maxLabelChars)
            }

            Controls.Label {
                Layout.fillWidth: true
                textFormat: Text.PlainText
                text: root.i18nContext.i18nc("@label:textbox", "Prompt:")
            }

            Controls.TextArea {
                id: promptField
                objectName: "customButtonPromptField"

                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 7
                wrapMode: TextEdit.Wrap
                textFormat: TextEdit.PlainText
                placeholderText: root.i18nContext.i18nc("@info:placeholder", "The question or instruction for the AI")
                Accessible.name: root.i18nContext.i18nc("@label:textbox", "Prompt")
                onTextChanged: if (length > root.limits.maxPromptChars) remove(root.limits.maxPromptChars, length)
            }

            Controls.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                font: Kirigami.Theme.smallFont
                text: root.i18nContext.i18nc("@info %1 characters written, %2 the most allowed", "%1 of %2 characters", promptField.length, root.limits.maxPromptChars)
            }

            Controls.CheckBox {
                id: insertOnlyBox
                objectName: "customButtonInsertOnlyBox"

                Layout.fillWidth: true
                text: root.i18nContext.i18nc("@option:check", "Insert into input box instead of sending")
                onToggled: {
                    ActionLog.taken("AiChatButtonsSettings_toggleInsertOnly")
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font: Kirigami.Theme.smallFont
                text: root.i18nContext.i18nc("@info", "Useful for templates where you want to fill in a date, shop, or category before sending. Buttons stay on this computer only; write no passwords or PINs.")
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                type: Kirigami.MessageType.Error
                visible: root.editorError !== ""
                text: root.editorError !== "" ? Plain.plain(root.editorError) : ""
            }
        }
    }
}
