// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// My groups and Hidden categories (M6b.5c, D-290, D-309). A group puts
// categories together under a name of the user's own, so a chart can show
// them as one slice or bar; a hidden category stays out of the charts where
// it would be shown on its own. Neither changes a category, a transaction or
// any total: it is only a way of looking at the figures.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property CategoryGroups groups
    readonly property var groupList: groups.view.groups ?? []
    readonly property var categoryList: groups.view.categories ?? []

    signal addGroupClicked()
    signal groupClicked(groupId: string)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The IDs of the hidden categories, with `id` added or taken out.
    function hiddenWith(id: string, hide: bool): string {
        const ids = [];
        for (const category of categoryList) {
            const hidden = category.id === id ? hide : category.hidden;
            if (hidden) {
                ids.push(category.id);
            }
        }
        return ids.join(",");
    }

    // What a group holds, in a few words: the first names and how many more.
    function summary(group: var): string {
        const names = group.members.map(member => member.name);
        if (names.length === 0) {
            return i18nContext.i18nc("@info a group with nothing in it yet", "Nothing in it yet");
        }
        return names.length <= 3
            ? names.join(", ")
            : i18nContext.i18nc("@info %1 is the names of the first three members, %2 how many more there are", "%1 and %2 more", names.slice(0, 3).join(", "), names.length - 3);
    }

    title: i18nContext.i18nc("@title", "My groups and hidden categories")
    header: FormError {
        id: error
    }

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "New group")
            onTriggered: page.addGroupClicked()
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Heading {
            Layout.fillWidth: true
            level: 2
            text: page.i18nContext.i18nc("@title", "My groups")
        }

        Controls.Label {
            Layout.fillWidth: true
            text: page.i18nContext.i18nc("@info", "Put categories together under a name of your own, for example Fixed costs for Rent, Insurance and Phone. A chart can then show the group as one slice or bar. Groups don't change your categories or any total.")
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: page.groupList.length === 0
            text: page.i18nContext.i18nc("@info", "You have no groups yet")
            helpfulAction: Kirigami.Action {
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "New group")
                onTriggered: page.addGroupClicked()
            }
        }

        Repeater {
            model: page.groupList

            delegate: Controls.ItemDelegate {
                id: entry

                required property var modelData
                required property int index

                Layout.fillWidth: true
                // A name could contain anything.
                text: modelData.name
                Accessible.name: page.i18nContext.i18nc("@info %1 is the name of a group, %2 what it holds", "%1: %2", modelData.name, page.summary(modelData))
                onClicked: page.groupClicked(modelData.id)

                contentItem: RowLayout {
                    spacing: Kirigami.Units.smallSpacing

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        Controls.Label {
                            Layout.fillWidth: true
                            text: entry.text
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            font.bold: true
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            text: (entry.modelData.income
                                ? page.i18nContext.i18nc("@info the kind of a group", "Money received")
                                : page.i18nContext.i18nc("@info the kind of a group", "Money spent"))
                                + " · " + page.summary(entry.modelData)
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            font: Kirigami.Theme.smallFont
                            opacity: 0.8
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "arrow-up"
                        display: Controls.AbstractButton.IconOnly
                        text: page.i18nContext.i18nc("@action:button move a group up in the list", "Move up")
                        enabled: entry.index > 0
                        onClicked: error.result = page.groups.moveGroup(entry.modelData.id, entry.index - 1)
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }

                    Controls.ToolButton {
                        icon.name: "arrow-down"
                        display: Controls.AbstractButton.IconOnly
                        text: page.i18nContext.i18nc("@action:button move a group down in the list", "Move down")
                        enabled: entry.index < page.groupList.length - 1
                        onClicked: error.result = page.groups.moveGroup(entry.modelData.id, entry.index + 1)
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }

                    Controls.ToolButton {
                        icon.name: "edit-delete"
                        display: Controls.AbstractButton.IconOnly
                        text: page.i18nContext.i18nc("@action:button remove a group; its categories stay", "Remove the group")
                        onClicked: error.result = page.groups.removeGroup(entry.modelData.id)
                        Controls.ToolTip.text: page.i18nContext.i18nc("@info", "Removes only the group. Its categories stay as they are, and Undo brings the group back.")
                        Controls.ToolTip.visible: hovered
                        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.largeSpacing
        }

        Kirigami.Heading {
            Layout.fillWidth: true
            level: 2
            text: page.i18nContext.i18nc("@title", "Hidden categories")
        }

        Controls.Label {
            Layout.fillWidth: true
            text: page.i18nContext.i18nc("@info", "A hidden category, with everything inside it, stays out of charts where it would be shown on its own, so a large one such as Rent doesn't crowd out the rest. A chart that leaves categories out says so, with the amount. Groups and the key figures always count everything.")
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.categoryList

            delegate: Controls.CheckBox {
                id: box

                required property var modelData

                Layout.fillWidth: true
                Layout.leftMargin: box.modelData.depth * Kirigami.Units.gridUnit * 1.5
                text: modelData.name
                checked: modelData.hidden
                Accessible.name: page.i18nContext.i18nc("@option:check %1 is the path of a category", "Hide %1 in charts", modelData.path)
                onToggled: {
                    error.result = page.groups.setHidden(page.hiddenWith(box.modelData.id, checked));
                    // The list is read again; show what the book says.
                    checked = Qt.binding(() => box.modelData.hidden);
                }
            }
        }
    }
}
