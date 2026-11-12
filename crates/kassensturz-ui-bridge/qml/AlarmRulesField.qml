// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The *Warnings* field of the warnings tile's settings (US4.9, D-722,
// design §6): each rule as a short sentence with *Change* and *Remove*, and
// *Add a warning…*. The names and amounts come from `BookAlarms`; a change
// is sent as the whole list, as the document writes it.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: field

    // The rules, as the field's JSON text.
    property string rulesText: "[]"
    // The book's own accounts and budgets, `{id, name}` each.
    property var accounts: []
    property var budgets: []

    // The new list, as JSON text.
    signal rulesEdited(string json)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property var rules: {
        try {
            return JSON.parse(rulesText);
        } catch (error) {
            return [];
        }
    }
    readonly property var rows: feed.view?.rows ?? []
    // The editor, for the smoke test.
    property alias ruleDialog: dialog

    function send(list: var) {
        rulesEdited(JSON.stringify(list));
    }

    spacing: Kirigami.Units.smallSpacing

    onRulesTextChanged: feed.show(rulesText)
    Component.onCompleted: feed.show(rulesText)

    BookAlarms {
        id: feed
    }

    AlarmWords {
        id: words
    }

    AlarmRuleDialog {
        id: dialog

        accounts: field.accounts
        budgets: field.budgets
        feed: feed
        onSaved: (index, rule) => {
            const list = field.rules.slice();
            if (index < 0) {
                list.push(rule);
            } else {
                list[index] = rule;
            }
            field.send(list);
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        Layout.maximumWidth: Kirigami.Units.gridUnit * 22
        visible: field.rules.length === 0
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: field.i18nContext.i18nc("@info", "No warnings yet. The tile shows a line when one of them applies today, and nothing ever leaves the app.")
    }

    Repeater {
        model: field.rows

        delegate: RowLayout {
            id: line

            required property var modelData

            Layout.fillWidth: true

            Controls.Label {
                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 16
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: words.summary(line.modelData)
            }

            Controls.ToolButton {
                icon.name: "document-edit"
                display: Controls.AbstractButton.IconOnly
                text: field.i18nContext.i18nc("@action:button change a warning", "Change")
                Accessible.name: text + ": " + words.summary(line.modelData)
                Controls.ToolTip.text: text
                Controls.ToolTip.visible: hovered
                onClicked: {
                    ActionLog.taken("AlarmRulesField_change")
                    dialog.edit(line.modelData.index, line.modelData);
                }
            }

            Controls.ToolButton {
                icon.name: "list-remove"
                display: Controls.AbstractButton.IconOnly
                text: field.i18nContext.i18nc("@action:button remove a warning", "Remove")
                Accessible.name: text + ": " + words.summary(line.modelData)
                Controls.ToolTip.text: text
                Controls.ToolTip.visible: hovered
                onClicked: {
                    ActionLog.taken("AlarmRulesField_remove")
                    field.send(field.rules.filter((_, at) => at !== line.modelData.index));
                }
            }
        }
    }

    Controls.Button {
        text: field.i18nContext.i18nc("@action:button", "Add a warning…")
        icon.name: "list-add"
        // At most 20 per tile (D-722).
        enabled: field.rules.length < 20
        onClicked: {
            ActionLog.taken("AlarmRulesField_add")
            dialog.edit(-1, {});
        }
    }
}
