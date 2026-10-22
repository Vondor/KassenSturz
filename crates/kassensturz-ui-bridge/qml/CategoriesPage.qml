// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The categories as a tree (D-067, D-082): what money came from and what it
// was spent on, with categories inside categories, e.g. Groceries inside
// Food. Income comes before spending; each category is followed by what is
// inside it.

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

    signal addCategoryClicked()
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
            model: page.rows

            delegate: ColumnLayout {
                id: entry

                required property var modelData
                required property int index

                // Income and spending each start with a heading.
                readonly property bool startsKind: index === 0 || page.rows[index - 1].income !== modelData.income

                Layout.fillWidth: true
                spacing: 0

                Kirigami.Heading {
                    Layout.topMargin: entry.index === 0 ? 0 : Kirigami.Units.largeSpacing
                    Layout.bottomMargin: Kirigami.Units.smallSpacing
                    visible: entry.startsKind
                    text: entry.modelData.income
                        ? page.i18nContext.i18nc("@title", "Where money comes from")
                        : page.i18nContext.i18nc("@title", "What money is spent on")
                    level: 2
                }

                Controls.ItemDelegate {
                    Layout.fillWidth: true
                    // Inside another category: indented one step per level.
                    leftPadding: Kirigami.Units.largeSpacing + entry.modelData.depth * Kirigami.Units.gridUnit * 1.5
                    text: entry.modelData.name
                    // A name could contain anything.
                    contentItem: Controls.Label {
                        text: parent.text
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                    }
                    Accessible.name: entry.modelData.path
                    onClicked: page.categoryClicked(entry.modelData.id)
                }
            }
        }
    }
}
