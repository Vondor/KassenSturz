// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Settings › Plugins (#791 step 3, AGENTS.md §7, D-851): the plugins the
// user added, adding a downloaded plugin file after seeing what it asks
// for, and removing one. There is no plugin store (D-007). Adding copies the
// file into KassenSturz's own folder; removing deletes only that copy.
// Everything a plugin file says about itself is untrusted text and shown as
// plain text (AGENTS.md §8); the words for extension points and permissions
// are written here (D-034). Plugins don't do anything yet: what each kind
// of plugin does comes with the next steps of #791.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property AiSwitch ai

    // What the last add or check returned: empty, or a code.
    property var result: ({})
    // Whether the last add worked.
    property bool added: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Plugins")

    Plugins {
        id: plugins
    }

    Component.onCompleted: plugins.refresh(page.ai.enabled)

    // What a kind of plugin adds, in plain words.
    function pointText(point: string): string {
        switch (point) {
        case "importer":
            return i18nContext.i18nc("@item what a plugin adds", "Reads another kind of bank file");
        case "exporter":
            return i18nContext.i18nc("@item what a plugin adds", "Saves your data in another kind of file");
        case "report":
            return i18nContext.i18nc("@item what a plugin adds", "Adds a report");
        case "automation":
            return i18nContext.i18nc("@item what a plugin adds", "Reacts to new transactions, for example after an import");
        case "ai_provider":
            // Listed only while AI is on (D-851); the audit of D-013 keeps
            // the word itself to the Ai* files.
            return i18nContext.i18nc("@item what a plugin adds", "Connects another language model, for the chat and suggestions");
        case "theme":
            return i18nContext.i18nc("@item what a plugin adds", "Changes the colors and fonts");
        case "ui_density":
            return i18nContext.i18nc("@item what a plugin adds", "Changes how much the screens explain");
        }
        return point;
    }

    // What a permission lets a plugin do, in plain words.
    function permissionText(permission: string): string {
        switch (permission) {
        case "read_book":
            return i18nContext.i18nc("@item what a plugin may do", "Read your book: accounts, categories, transactions, plans, budgets and tags");
        case "propose_changes":
            return i18nContext.i18nc("@item what a plugin may do", "Suggest changes to your book, which you check and confirm first");
        case "network":
            return i18nContext.i18nc("@item what a plugin may do", "Connect to other computers over the internet");
        }
        return permission;
    }

    function listText(words: var, wordText: var): string {
        return words.map(word => "• " + wordText(word)).join("\n");
    }

    function chooseFile() {
        page.result = {};
        page.added = false;
        fileDialog.open();
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "What plugins are")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A plugin adds something to KassenSturz, for example a way to read another bank's files. You add a plugin file you downloaded; there is no shop. Before you add it, you see what it asks to be allowed. A plugin runs closed off from the rest of your computer: it can't open your files, and it can only use what it asked for here.")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Only add plugins from people you trust. Plugins can't do anything yet in this version: you can add and look at them already, and what each kind does comes with the next updates.")
    }

    FormError {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.largeSpacing
        result: page.result
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.largeSpacing
        type: Kirigami.MessageType.Positive
        visible: page.added
        text: page.i18nContext.i18nc("@info", "The plugin was added. The file you downloaded is unchanged; you can delete it.")
    }

    FormCard.FormCard {
        Layout.topMargin: Kirigami.Units.gridUnit
        visible: plugins.pending === undefined || plugins.pending.ok !== true

        FormCard.FormButtonDelegate {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add a plugin file…")
            description: page.i18nContext.i18nc("@info", "You see what it asks for before anything is added.")
            onClicked: {
                ActionLog.taken("SettingsPluginsPage_addFile")
                page.chooseFile()
            }
        }
    }

    // The picked file, before it is added.
    FormCard.FormHeader {
        visible: plugins.pending !== undefined && plugins.pending.ok === true
        title: page.i18nContext.i18nc("@title:group", "Add this plugin?")
    }

    FormCard.FormCard {
        visible: plugins.pending !== undefined && plugins.pending.ok === true

        PluginDetails {
            plugin: plugins.pending ?? ({})
            pointsText: page.listText(plugin.points ?? [], page.pointText)
            permissionsText: page.listText(plugin.permissions ?? [], page.permissionText)
        }

        FormCard.FormTextDelegate {
            visible: (plugins.pending?.permissions ?? []).indexOf("network") >= 0
            text: page.i18nContext.i18nc("@info", "This plugin may send data over the internet. KassenSturz still asks you before anything about your money leaves your computer.")
            textItem.wrapMode: Text.Wrap
        }

        FormCard.FormTextDelegate {
            visible: plugins.pending?.replaces === true
            text: (plugins.pending?.installedVersion ?? "") === ""
                ? page.i18nContext.i18nc("@info", "You already added a plugin with this name. Adding this one replaces it.")
                : page.i18nContext.i18nc("@info %1 is a version number", "You already added this plugin in version %1. Adding this one replaces it.", plugins.pending?.installedVersion ?? "")
            textItem.wrapMode: Text.Wrap
            textItem.textFormat: Text.PlainText
        }

        FormCard.FormButtonDelegate {
            icon.name: "dialog-ok-apply"
            text: Plain.plain(plugins.pending?.replaces === true
                ? page.i18nContext.i18nc("@action:button", "Replace it")
                : page.i18nContext.i18nc("@action:button", "Add it"))
            onClicked: {
                ActionLog.taken("SettingsPluginsPage_add")
                const answer = plugins.add(plugins.pending?.replaces === true, page.ai.enabled);
                page.result = answer;
                page.added = answer.code === undefined;
            }
        }

        FormCard.FormButtonDelegate {
            icon.name: "dialog-cancel"
            text: page.i18nContext.i18nc("@action:button", "Don't add it")
            onClicked: {
                ActionLog.taken("SettingsPluginsPage_cancel")
                plugins.cancel()
            }
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Your plugins")
    }

    FormCard.FormCard {
        visible: plugins.items.length === 0

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "You haven't added any plugins.")
        }
    }

    FormCard.FormCard {
        visible: plugins.items.length > 0

        Repeater {
            model: plugins.items

            delegate: ColumnLayout {
                id: row

                required property var modelData
                required property int index

                Layout.fillWidth: true
                spacing: 0

                PluginDetails {
                    visible: row.modelData.ok
                    plugin: row.modelData
                    pointsText: page.listText(row.modelData.points ?? [], page.pointText)
                    permissionsText: page.listText(row.modelData.permissions ?? [], page.permissionText)
                }

                FormCard.FormTextDelegate {
                    visible: !row.modelData.ok
                    // The copy's file name comes from the folder: plain text.
                    text: row.modelData.file
                    textItem.textFormat: Text.PlainText
                    description: page.i18nContext.i18nc("@info", "This plugin can't be used any more, for example because it was made for another version of KassenSturz. Remove it, and look for a newer version of it.")
                }

                FormError {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    visible: !row.modelData.ok
                    result: row.modelData.error ?? ({})
                }

                FormCard.FormButtonDelegate {
                    icon.name: "edit-delete-remove"
                    text: page.i18nContext.i18nc("@action:button", "Remove this plugin…")
                    onClicked: {
                        ActionLog.taken("SettingsPluginsPage_remove")
                        removeDialog.ask(row.modelData)
                    }
                }

                FormCard.FormDelegateSeparator {
                    visible: row.index < plugins.items.length - 1
                }
            }
        }
    }

    Dialogs.FileDialog {
        id: fileDialog

        title: page.i18nContext.i18nc("@title:window", "Add a plugin file")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [
            page.i18nContext.i18nc("@item:inlistbox a file type", "Plugin files (*.wasm)"),
            page.i18nContext.i18nc("@item:inlistbox a file type", "All files (*)")
        ]
        onAccepted: {
            ActionLog.taken("SettingsPluginsPage_fileChosen")
            page.result = plugins.inspect(selectedFile, page.ai.enabled)
        }
    }

    Kirigami.PromptDialog {
        id: removeDialog

        property var plugin: ({})

        function ask(chosen: var) {
            plugin = chosen;
            open();
        }

        title: page.i18nContext.i18nc("@title:window", "Remove this plugin?")
        subtitle: page.i18nContext.i18nc("@info", "KassenSturz deletes its copy of the plugin. Your book isn't changed. To use the plugin again, you add its file again.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Remove it")
                icon.name: "edit-delete-remove"
                onTriggered: {
                    ActionLog.taken("SettingsPluginsPage_removeConfirmed")
                    removeDialog.close();
                    page.added = false;
                    page.result = plugins.remove(removeDialog.plugin.file ?? "", page.ai.enabled)
                        ? {}
                        : { code: "input.plugin-not-removed" };
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button keep a plugin", "Keep it")
                icon.name: "go-previous"
                onTriggered: {
                    ActionLog.taken("SettingsPluginsPage_keep")
                    removeDialog.close()
                }
            }
        ]
    }
}
