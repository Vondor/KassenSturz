// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Makes a group, or changes one's name and members (M6b.5c, D-290, D-309). A
// group of money spent takes spending categories and other groups of money
// spent; a group of money received takes income ones. A category counts with
// everything inside it, and once, however often it is reached.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property CategoryGroups groups
    // The group to change, by ID; empty to make a new one.
    property string groupId: ""
    property string groupName: ""
    property bool income: false
    // The IDs of what the group holds now.
    property var memberIds: []

    readonly property bool adding: groupId === ""
    // What can be put in: the categories of its kind and the other groups
    // of its kind, a group never into itself.
    readonly property var categoryChoices: (groups.view.categories ?? []).filter(category => category.income === income)
    readonly property var groupChoices: (groups.view.groups ?? []).filter(group => group.income === income && group.id !== groupId)
    // What the user ticked, by ID.
    property var ticked: ({})

    // The page is done: a group was made, or the changes were saved.
    signal done()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: adding
        ? i18nContext.i18nc("@title", "New group")
        : i18nContext.i18nc("@title %1 is the name of a group", "Change %1", groupName)
    header: FormError {
        id: error
    }

    Component.onCompleted: {
        const next = {};
        for (const id of memberIds) {
            next[id] = true;
        }
        ticked = next;
    }

    function tick(id: string, on: bool) {
        const next = Object.assign({}, ticked);
        if (on) {
            next[id] = true;
        } else {
            delete next[id];
        }
        ticked = next;
    }

    // The chosen IDs in the order the user put them: what the group held
    // keeps its place, what was added follows.
    function chosen(): string {
        const ids = memberIds.filter(id => ticked[id] === true);
        for (const id of Object.keys(ticked)) {
            if (!ids.includes(id)) {
                ids.push(id);
            }
        }
        return ids.join(",");
    }

    function save() {
        const result = adding
            ? groups.addGroup(nameField.text, income, chosen())
            : groups.editGroup(groupId, nameField.text, chosen());
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    Kirigami.FormLayout {
        Controls.ButtonGroup {
            id: kindGroup
        }

        Controls.RadioButton {
            visible: page.adding
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "What does it add up?")
            text: page.i18nContext.i18nc("@option:radio", "Money spent")
            checked: !page.income
            Controls.ButtonGroup.group: kindGroup
            onToggled: {
                ActionLog.taken("CategoryGroupPage_moneySpent")
                page.income = false;
                page.ticked = ({});
            }
        }

        Controls.RadioButton {
            visible: page.adding
            text: page.i18nContext.i18nc("@option:radio", "Money received")
            checked: page.income
            Controls.ButtonGroup.group: kindGroup
            onToggled: {
                ActionLog.taken("CategoryGroupPage_moneyReceived")
                page.income = true;
                page.ticked = ({});
            }
        }

        Controls.TextField {
            id: nameField

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Name:")
            text: page.groupName
            placeholderText: page.i18nContext.i18nc("@info:placeholder", "For example Fixed costs")
            onAccepted: {
                ActionLog.taken("CategoryGroupPage_save")
                page.save()
            }
        }

        Controls.Label {
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "What is in it:")
            visible: page.categoryChoices.length === 0 && page.groupChoices.length === 0
            text: page.i18nContext.i18nc("@info", "You have no categories of this kind yet.")
            wrapMode: Text.Wrap
        }

        Repeater {
            model: page.categoryChoices

            delegate: Controls.CheckBox {
                id: box

                required property var modelData
                required property int index

                Kirigami.FormData.label: index === 0 ? page.i18nContext.i18nc("@label", "What is in it:") : ""
                Layout.leftMargin: modelData.depth * Kirigami.Units.gridUnit * 1.5
                text: modelData.name
                checked: page.ticked[modelData.id] === true
                Accessible.name: modelData.path
                onToggled: {
                    ActionLog.taken("CategoryGroupPage_tick")
                    page.tick(modelData.id, checked)
                }
            }
        }

        Repeater {
            model: page.groupChoices

            delegate: Controls.CheckBox {
                id: groupBox

                required property var modelData

                text: page.i18nContext.i18nc("@option:check %1 is the name of a group that goes inside this one", "The group %1", modelData.name)
                checked: page.ticked[modelData.id] === true
                onToggled: {
                    ActionLog.taken("CategoryGroupPage_group")
                    page.tick(modelData.id, checked)
                }
            }
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            text: page.i18nContext.i18nc("@info", "A category counts with everything inside it, and only once, even if it is in the group twice. Hidden categories in a group still count.")
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font: Kirigami.Theme.smallFont
        }

        Controls.Button {
            text: page.adding
                ? page.i18nContext.i18nc("@action:button", "Make the group")
                : page.i18nContext.i18nc("@action:button", "Save")
            icon.name: page.adding ? "list-add" : "document-save"
            onClicked: {
                ActionLog.taken("CategoryGroupPage_save_2")
                page.save()
            }
        }
    }
}
