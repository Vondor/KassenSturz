// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One list of the filter bar (M6c.7, D-351): a button that says what is
// chosen and opens a list with a check box for each choice. Nothing chosen
// means everything. A choice the book no longer has (a removed category) is
// listed as "Something that no longer exists" so the user can take it out.
// The list is only shown and passed on: the layout checks it.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Button {
    id: picker

    // What the list is about, already in words ("Accounts").
    property string label: ""
    // The choices, as {id, name}.
    property var options: []
    // The IDs chosen now.
    property var chosen: []
    // What to say when there is nothing to pick yet.
    property string emptyText: ""

    // The list was changed: the IDs, separated by commas.
    signal picked(string ids)

    readonly property var known: options.map(option => String(option.id).toLowerCase())
    readonly property var gone: chosen.filter(id => !known.includes(String(id).toLowerCase()))

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function isChosen(id: string): bool {
        return chosen.some(one => String(one).toLowerCase() === String(id).toLowerCase());
    }

    function toggle(id: string, on: bool) {
        const without = chosen.filter(one => String(one).toLowerCase() !== String(id).toLowerCase());
        picked((on ? without.concat([id]) : without).join(","));
    }

    text: chosen.length === 0
        ? i18nContext.i18nc("@action:button %1 is what the list is about, like Accounts", "%1: all", label)
        : i18nContext.i18ncp("@action:button %2 is what the list is about, like Accounts", "%2: %1 chosen", "%2: %1 chosen", chosen.length, label)
    icon.name: "view-filter"
    checked: popup.opened
    Accessible.name: text
    Accessible.description: i18nContext.i18nc("@info:tooltip", "Choose which ones this page counts")
    onClicked: {
        ActionLog.taken("FilterPicker_clicked")
        popup.opened ? popup.close() : popup.open()
    }

    Controls.Popup {
        id: popup

        y: picker.height
        width: Math.max(picker.width, Kirigami.Units.gridUnit * 18)
        height: Math.min(content.implicitHeight + padding * 2, Kirigami.Units.gridUnit * 22)
        padding: Kirigami.Units.largeSpacing
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

        ColumnLayout {
            id: content

            anchors.fill: parent
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                Layout.fillWidth: true

                Controls.Label {
                    Layout.fillWidth: true
                    text: picker.label
                    font.bold: true
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }

                Controls.Button {
                    text: picker.i18nContext.i18nc("@action:button take away every choice, so all count", "Show all")
                    enabled: picker.chosen.length > 0
                    onClicked: {
                        ActionLog.taken("FilterPicker_showAll")
                        picker.picked("")
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: picker.options.length === 0
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: picker.emptyText
            }

            Controls.ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredHeight: Math.min(optionList.contentHeight, Kirigami.Units.gridUnit * 16)
                visible: picker.options.length > 0 || picker.gone.length > 0
                Accessible.name: picker.label

                ListView {
                    id: optionList

                    clip: true
                    model: picker.gone.map(id => ({id: id, name: "", gone: true})).concat(picker.options)
                    delegate: Controls.CheckBox {
                        required property var modelData

                        width: ListView.view.width
                        text: modelData.gone === true
                            ? picker.i18nContext.i18nc("@option:check a choice in a filter that was removed from the book", "Something that no longer exists")
                            : modelData.name
                        checked: picker.isChosen(modelData.id)
                        onToggled: {
                            ActionLog.taken("FilterPicker_toggle")
                            picker.toggle(modelData.id, checked)
                        }
                    }
                }
            }
        }
    }
}
