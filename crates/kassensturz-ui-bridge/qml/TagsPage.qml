// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Settings › Tags (US4.8, docs/TAGS.md §1): the book's tags with how often
// each is used, and renaming, merging and removing one. A tag in use is
// removed from everything after the page says how much it touches (Q-495);
// each change is one step that Undo takes back (D-086).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookTags tags
    // The tag the dialog is about.
    property var chosen: ({})
    // What the last change returned: empty, or the error.
    property var result: ({})

    // Undo and redo are in the side menu; the page reads the tags again.
    function survivesUndo(): bool {
        tags.refresh();
        return true;
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Tags")

    Component.onCompleted: tags.refresh()

    // How often a tag is used, in words.
    function useText(tag: var): string {
        const transactions = i18nContext.i18ncp("@info %1 is a number", "%1 transaction", "%1 transactions", tag.transactions);
        const parts = i18nContext.i18ncp("@info %1 is a number", "%1 part", "%1 parts", tag.parts);
        if (tag.transactions > 0 && tag.parts > 0) {
            return i18nContext.i18nc("@info %1 is how many transactions, %2 how many parts", "Used on %1 and %2", transactions, parts);
        }
        if (tag.transactions > 0) {
            return i18nContext.i18nc("@info %1 is how many transactions", "Used on %1", transactions);
        }
        if (tag.parts > 0) {
            return i18nContext.i18nc("@info %1 is how many parts", "Used on %1", parts);
        }
        return i18nContext.i18nc("@info", "Not used yet");
    }

    function open(tag: var) {
        chosen = tag;
        result = ({});
        nameField.text = tag.name;
        mergeBox.currentIndex = -1;
        tagDialog.open();
    }

    // Closes the dialog when the change worked; otherwise it says why.
    function finish(outcome: var) {
        result = outcome;
        if (outcome.code === undefined) {
            tagDialog.close();
            removeDialog.close();
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Tags are your own labels, for example Holiday 2026 or Tax. They never change an amount or a sum. Add them on a transaction's page or on one of its parts.")
    }

    FormCard.FormCard {
        visible: page.tags.tags.length === 0

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "No tags yet.")
        }
    }

    FormCard.FormCard {
        visible: page.tags.tags.length > 0

        Repeater {
            model: page.tags.tags

            FormCard.FormButtonDelegate {
                required property var modelData

                icon.name: "tag"
                // A tag's name could come from an imported file.
                text: Plain.plain(modelData.name)
                description: page.useText(modelData)
                descriptionItem.textFormat: Text.PlainText
                onClicked: {
                    ActionLog.taken("TagsPage_openTag")
                    page.open(modelData)
                }
            }
        }
    }

    Kirigami.Dialog {
        id: tagDialog

        title: Plain.plain(page.chosen.name ?? "")
        standardButtons: Kirigami.Dialog.Close
        preferredWidth: Kirigami.Units.gridUnit * 24
        padding: Kirigami.Units.largeSpacing

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            FormError {
                Layout.fillWidth: true
                result: page.result
            }

            Controls.Label {
                Layout.fillWidth: true
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: page.chosen.name === undefined ? "" : page.useText(page.chosen)
            }

            Controls.Label {
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@label:textbox", "Name:")
            }

            RowLayout {
                Controls.TextField {
                    id: nameField

                    Layout.fillWidth: true
                    maximumLength: 60
                    Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name:")
                    onAccepted: {
                        ActionLog.taken("TagsPage_renameTag")
                        page.finish(page.tags.rename(page.chosen.id, text))
                    }
                }

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Rename")
                    icon.name: "edit-rename"
                    enabled: nameField.text.trim() !== "" && nameField.text !== page.chosen.name
                    onClicked: {
                        ActionLog.taken("TagsPage_rename")
                        page.finish(page.tags.rename(page.chosen.id, nameField.text))
                    }
                }
            }

            Controls.Label {
                visible: mergeBox.count > 0
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@label:listbox", "Merge it into:")
            }

            RowLayout {
                visible: mergeBox.count > 0

                Controls.ComboBox {
                    id: mergeBox

                    Layout.fillWidth: true
                    model: page.tags.tags.filter(tag => tag.id !== page.chosen.id)
                    textRole: "name"
                    valueRole: "id"
                    displayText: currentIndex < 0 ? page.i18nContext.i18nc("@item:inlistbox no tag chosen yet", "Choose a tag") : currentText
                    Accessible.name: page.i18nContext.i18nc("@label:listbox", "Merge it into:")
                }

                Controls.Button {
                    text: page.i18nContext.i18nc("@action:button", "Merge")
                    icon.name: "merge"
                    enabled: mergeBox.currentIndex >= 0
                    onClicked: {
                        ActionLog.taken("TagsPage_merge")
                        page.finish(page.tags.merge(page.chosen.id, mergeBox.currentValue))
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: mergeBox.currentIndex >= 0
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
                text: visible
                    ? page.i18nContext.i18nc("@info %1 and %2 are the names of tags", "Everything tagged %1 gets %2 instead, and %1 is removed. You can undo this.", page.chosen.name, mergeBox.currentText)
                    : ""
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Remove this tag…")
                icon.name: "edit-delete"
                onClicked: {
                    ActionLog.taken("TagsPage_removeTag")
                    removeDialog.open()
                }
            }
        }
    }

    // A tag in use is removed from everything only after the user saw how
    // much that touches (Q-495).
    Kirigami.PromptDialog {
        id: removeDialog

        readonly property bool used: (page.chosen.transactions ?? 0) > 0 || (page.chosen.parts ?? 0) > 0

        title: page.i18nContext.i18nc("@title:window", "Remove this tag?")
        subtitle: Plain.plain(used
            ? page.i18nContext.i18nc("@info %1 is the name of a tag, %2 how often it is used", "%1 is taken off everything it is on (%2) and removed from the book. The transactions and their amounts stay as they are. You can undo this.", page.chosen.name ?? "", page.chosen.name === undefined ? "" : page.useText(page.chosen))
            : page.i18nContext.i18nc("@info %1 is the name of a tag", "%1 is not used anywhere. It is removed from the book. You can undo this.", page.chosen.name ?? ""))
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Remove the tag")
                icon.name: "edit-delete"
                onTriggered: {
                    ActionLog.taken("TagsPage_removeTheTag")
                    page.finish(page.tags.remove(page.chosen.id))
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep it")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("TagsPage_keep")
                    removeDialog.close()
                }
            }
        ]
    }
}
