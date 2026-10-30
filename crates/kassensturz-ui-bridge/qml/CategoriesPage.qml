// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The categories as a tree (D-067, D-082): what money came from and what it
// was spent on, with categories inside categories, e.g. Groceries inside
// Food. Income comes before spending; each category is followed by what is
// inside it. Like KMyMoney's category list (round 1), the tree opens and
// closes, with thin guide lines, small icons and compact, alternately shaded
// lines.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property CategoryTree categories
    readonly property var rows: categories.view.rows ?? []
    // What the user closed, by the category's ID or the heading's key.
    property var collapsed: ({})
    // What the tree shows now: the two headings with their categories, minus
    // what sits inside a closed one.
    readonly property var shown: visibleItems(rows, collapsed)

    function toggle(key: string) {
        const next = Object.assign({}, collapsed);
        if (next[key]) {
            delete next[key];
        } else {
            next[key] = true;
        }
        collapsed = next;
    }

    // The rows are in tree order, income first, each followed by what is
    // inside it, so a closed one hides the rows after it that are deeper.
    function visibleItems(all: var, closed: var): var {
        const out = [];
        let groupHidden = false;
        let hidingDepth = -1;
        for (let i = 0; i < all.length; i++) {
            const row = all[i];
            if (i === 0 || all[i - 1].income !== row.income) {
                const key = row.income ? "income" : "spending";
                groupHidden = closed[key] === true;
                hidingDepth = -1;
                out.push({
                    group: true,
                    key: key,
                    id: "",
                    path: "",
                    depth: 0,
                    hasChildren: true,
                    expanded: !groupHidden,
                    name: row.income
                        ? i18nContext.i18nc("@title", "Where money comes from")
                        : i18nContext.i18nc("@title", "What money is spent on"),
                });
            }
            if (groupHidden || (hidingDepth >= 0 && row.depth > hidingDepth)) {
                continue;
            }
            hidingDepth = -1;
            const hasChildren = i + 1 < all.length && all[i + 1].depth > row.depth;
            const expanded = closed[row.id] !== true;
            if (hasChildren && !expanded) {
                hidingDepth = row.depth;
            }
            out.push({
                group: false,
                key: row.id,
                id: row.id,
                path: row.path,
                depth: row.depth + 1,
                hasChildren: hasChildren,
                expanded: expanded,
                name: row.name,
            });
        }
        return out;
    }

    signal addCategoryClicked()
    signal groupsClicked()
    signal categoryClicked(categoryId: string)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Categories")

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add category")
            onTriggered: page.addCategoryClicked()
        },
        Kirigami.Action {
            icon.name: "folder-symbolic"
            text: page.i18nContext.i18nc("@action:button opens the page for groups of categories and hidden ones", "My groups and hidden categories")
            onTriggered: page.groupsClicked()
        }
    ]

    ColumnLayout {
        spacing: 0

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: page.rows.length === 0
            text: page.i18nContext.i18nc("@info", "You have no categories yet")
            explanation: page.i18nContext.i18nc("@info", "A category says what money was spent on or where it came from, for example Groceries or Salary.")
            helpfulAction: Kirigami.Action {
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "Add category")
                onTriggered: page.addCategoryClicked()
            }
        }

        Repeater {
            model: page.shown

            delegate: Controls.ItemDelegate {
                id: entry

                required property var modelData
                required property int index

                readonly property bool isGroup: modelData.group
                readonly property real step: Kirigami.Units.gridUnit * 1.5

                Layout.fillWidth: true
                // Compact lines, every second one shaded, to follow across.
                topPadding: Kirigami.Units.smallSpacing / 2
                bottomPadding: Kirigami.Units.smallSpacing / 2
                leftPadding: Kirigami.Units.smallSpacing
                // A name could contain anything.
                text: modelData.name
                Accessible.name: isGroup ? modelData.name : modelData.path
                // A heading opens and closes; a category opens to be changed.
                onClicked: isGroup ? page.toggle(modelData.key) : page.categoryClicked(modelData.id)
                Keys.onLeftPressed: if (modelData.hasChildren && modelData.expanded) page.toggle(modelData.key)
                Keys.onRightPressed: if (modelData.hasChildren && !modelData.expanded) page.toggle(modelData.key)

                background: Rectangle {
                    readonly property bool lit: entry.down || entry.hovered || entry.visualFocus

                    color: lit
                        ? Kirigami.Theme.highlightColor
                        : entry.index % 2 === 1 ? Kirigami.Theme.alternateBackgroundColor : "transparent"
                    opacity: lit ? 0.3 : 1
                }

                contentItem: RowLayout {
                    spacing: 0

                    // One thin line per level, joining a category to the
                    // one it sits inside.
                    Repeater {
                        model: entry.modelData.depth

                        delegate: Item {
                            Layout.preferredWidth: entry.step
                            Layout.fillHeight: true

                            Kirigami.Separator {
                                anchors.horizontalCenter: parent.horizontalCenter
                                anchors.top: parent.top
                                anchors.bottom: parent.bottom
                                anchors.topMargin: -entry.topPadding
                                anchors.bottomMargin: -entry.bottomPadding
                                width: 1
                            }
                        }
                    }

                    // The arrow of what has something inside; the others
                    // keep its room, so the names line up.
                    Controls.ToolButton {
                        Layout.preferredWidth: entry.step
                        Layout.preferredHeight: entry.step
                        visible: entry.modelData.hasChildren
                        icon.name: entry.modelData.expanded ? "arrow-down" : "arrow-right"
                        display: Controls.AbstractButton.IconOnly
                        flat: true
                        focusPolicy: Qt.NoFocus
                        text: entry.modelData.expanded
                            ? page.i18nContext.i18nc("@action:button hide what is inside a category", "Collapse")
                            : page.i18nContext.i18nc("@action:button show what is inside a category", "Expand")
                        onClicked: page.toggle(entry.modelData.key)

                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }

                    Item {
                        Layout.preferredWidth: entry.step
                        visible: !entry.modelData.hasChildren
                    }

                    Kirigami.Icon {
                        Layout.preferredWidth: Kirigami.Units.iconSizes.small
                        Layout.preferredHeight: Kirigami.Units.iconSizes.small
                        Layout.rightMargin: Kirigami.Units.smallSpacing
                        source: entry.modelData.hasChildren ? "folder" : "tag"
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        text: entry.text
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        font.bold: entry.isGroup
                    }
                }
            }
        }
    }
}
