// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Tags field of a form (US4.8, docs/TAGS.md §1): the chosen tags as
// small chips that can be taken away, and a box to add one. Typing filters
// the book's tags; a name the book doesn't have yet is offered as
// "Create …", and made when the form is saved, in the same step, so one
// Undo takes back both (D-086).
//
// Keyboard: Tab into the box, type, Up and Down to choose, Enter to take,
// Escape to close the list, Backspace in the empty box takes away the last
// chip; a chip with the focus is taken away with Delete or Backspace.
//
// `tags` holds `{id, name}` objects as the Rust side gives and takes them:
// an empty `id` is a new tag.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: field

    // The chosen tags, in their order.
    property var tags: []
    // The book's tags to choose from (`tagChoices()` of the register).
    property var choices: []
    // The most tags it takes (MAX_TAGS_ON_TRANSACTION or ..._ON_PART);
    // a save checks this again.
    property int limit: 20
    // The longest name a tag can have (MAX_TAG_NAME_CHARS).
    readonly property int nameLimit: 60
    // What the box is called for a screen reader.
    property string accessibleName: i18nContext.i18nc("@label:textbox", "Tags:")

    // The user added or took away a tag.
    signal edited()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    spacing: Kirigami.Units.smallSpacing

    // Whether a tag of this name is chosen already, ignoring capitals and
    // spaces around it, as the book compares names.
    function chosen(name: string): bool {
        const wanted = name.trim().toLowerCase();
        return tags.some(tag => tag.name.trim().toLowerCase() === wanted);
    }

    // What the list offers for the typed text: the book's tags not chosen
    // yet whose name holds it, and "Create …" when no tag has this name.
    readonly property var offered: {
        const typed = input.text.trim();
        const lower = typed.toLowerCase();
        const found = choices
            .filter(choice => !chosen(choice.name) && choice.name.toLowerCase().indexOf(lower) >= 0)
            .map(choice => ({ id: choice.id, name: choice.name, create: false }));
        const known = choices.some(choice => choice.name.trim().toLowerCase() === lower);
        if (typed !== "" && !known && !chosen(typed)) {
            found.push({ id: "", name: typed, create: true });
        }
        return found;
    }

    function add(tag: var) {
        if (chosen(tag.name) || tags.length >= limit) {
            return;
        }
        tags = tags.concat([{ id: tag.id, name: tag.name }]);
        input.text = "";
        edited();
    }

    // Takes the tag chosen in the list for what was typed (Enter); with
    // nothing typed it does nothing, so Enter never saves the form by
    // mistake.
    function takeChosen() {
        const offer = offered[list.currentIndex];
        if (input.text.trim() !== "" && offer !== undefined) {
            add(offer);
            popup.close();
        }
    }

    // Moves the choice in the list up (-1) or down (1).
    function moveChoice(step: int) {
        if (offered.length > 0) {
            popup.open();
            list.currentIndex = Math.max(0, Math.min(list.currentIndex + step, offered.length - 1));
        }
    }

    function removeAt(index: int) {
        const kept = tags.slice();
        kept.splice(index, 1);
        tags = kept;
        edited();
    }

    Flow {
        Layout.fillWidth: true
        visible: field.tags.length > 0
        spacing: Kirigami.Units.smallSpacing

        Repeater {
            model: field.tags

            Kirigami.Chip {
                required property var modelData
                required property int index

                text: Plain.plain(modelData.name)
                checkable: false
                closable: true
                Accessible.name: field.i18nContext.i18nc("@action:button %1 is a tag", "Take away the tag %1", modelData.name)
                // The chip itself takes the tag away too, so Space on a
                // chip with the focus does what its close button does.
                onClicked: {
                    ActionLog.taken("TagField_removeTag")
                    field.removeAt(index)
                }
                onRemoved: {
                    ActionLog.taken("TagField_removeTag")
                    field.removeAt(index)
                }
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) {
                        ActionLog.taken("TagField_removeTag")
                        field.removeAt(index);
                        event.accepted = true;
                    }
                }
            }
        }
    }

    Controls.TextField {
        id: input

        Layout.fillWidth: true
        enabled: field.tags.length < field.limit
        maximumLength: field.nameLimit
        placeholderText: enabled
            ? field.i18nContext.i18nc("@info:placeholder", "Add a tag, for example Holiday")
            : field.i18nContext.i18nc("@info:placeholder %1 is a number", "At most %1 tags", field.limit)
        Accessible.name: field.accessibleName
        Accessible.description: field.i18nContext.i18nc("@info", "Type to find a tag or to make a new one, then press Enter.")

        onTextChanged: {
            list.currentIndex = 0;
            if (text.trim() !== "" && activeFocus) {
                popup.open();
            } else if (text.trim() === "") {
                popup.close();
            }
        }
        onActiveFocusChanged: {
            if (!activeFocus) {
                popup.close();
            }
        }
        // Enter takes the tag chosen in the list.
        onAccepted: {
            ActionLog.taken("TagField_addTag")
            field.takeChosen()
        }
        Keys.onDownPressed: event => field.moveChoice(1)
        Keys.onUpPressed: event => field.moveChoice(-1)
        Keys.onEscapePressed: event => {
            if (popup.opened) {
                popup.close();
            } else {
                event.accepted = false;
            }
        }
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Backspace && text === "" && field.tags.length > 0) {
                ActionLog.taken("TagField_removeTag")
                field.removeAt(field.tags.length - 1);
                event.accepted = true;
            }
        }

        Controls.Popup {
            id: popup

            y: input.height
            width: input.width
            padding: 0
            visible: false
            closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutsideParent

            contentItem: ListView {
                id: list

                implicitHeight: Math.min(contentHeight, Kirigami.Units.gridUnit * 10)
                clip: true
                model: field.offered
                keyNavigationEnabled: false

                delegate: Controls.ItemDelegate {
                    required property var modelData
                    required property int index

                    width: ListView.view.width
                    highlighted: ListView.isCurrentItem
                    text: Plain.plain(modelData.create
                        ? field.i18nContext.i18nc("@item:inlistbox %1 is the name of a new tag", "Create \"%1\"", modelData.name)
                        : modelData.name)
                    icon.name: modelData.create ? "list-add" : "tag"
                    onClicked: {
                        ActionLog.taken("TagField_chooseTag")
                        field.add(modelData);
                        popup.close();
                        input.forceActiveFocus();
                    }
                }
            }
        }
    }
}
