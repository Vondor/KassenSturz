// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Adds or changes one rule of the warnings tile (US4.9, D-722, design §6):
// what to watch, which budget or account, and the value. A typed amount is
// read by Rust in the account's currency and kept in the tile's plain form
// (D-065); QML never compares an amount. *Save* stays off until every
// needed field is right.
//
//     dialog.edit(index, row);   // index −1 and row {} for a new rule
//
// `saved(index, rule)` follows *Save*, with the rule as the document writes
// it.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    objectName: "alarmRuleDialog"

    // The book's own accounts and budgets, `{id, name}` each.
    property var accounts: []
    property var budgets: []
    // `BookAlarms`, to read a typed amount.
    property var feed: null

    // The rule's place in the list, −1 for a new one.
    property int index: -1
    property string reason: "budget_over"
    property string account: ""
    property string budget: ""
    property int share: 100
    property int times: 1
    property bool lowest: false

    // The amount as the user types it, and what Rust made of it.
    property string typed: ""
    readonly property string plain: feed && account !== "" && typed.trim() !== "" ? feed.readAmount(typed, account) : ""

    readonly property bool onAccount: reason === "account_below" || reason === "account_reserve"
    readonly property bool onBudget: reason !== "account_below"
    readonly property bool ready: (!onAccount || account !== "") && (!onBudget || budget !== "") && (reason !== "account_below" || plain !== "")

    signal saved(int index, var rule)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function edit(at: int, row: var) {
        index = at;
        reason = row.reason ?? "budget_over";
        account = row.account ?? "";
        budget = row.budget ?? "";
        share = row.share > 0 ? row.share : (reason === "budget_under" ? 80 : 100);
        times = row.times > 0 ? row.times : 1;
        lowest = row.lowest ?? (reason === "account_reserve");
        typed = row.amountInput ?? "";
        open();
    }

    // The defaults of a reason, when the user picks another one (design §2).
    function pick(word: string) {
        reason = word;
        share = word === "budget_under" ? 80 : 100;
        lowest = word === "account_reserve";
    }

    function rule(): var {
        switch (reason) {
        case "budget_over":
        case "budget_under":
            return {reason: reason, budget: budget, share: share};
        case "account_below":
            return {reason: reason, account: account, amount: plain, lowest: lowest};
        default:
            return {reason: reason, account: account, budget: budget, times: times, lowest: lowest};
        }
    }

    title: Plain.plain(index < 0
        ? i18nContext.i18nc("@title:window", "Add a warning")
        : i18nContext.i18nc("@title:window", "Change a warning"))
    preferredWidth: Kirigami.Units.gridUnit * 26
    standardButtons: Kirigami.Dialog.NoButton
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Save")
            icon.name: "document-save"
            enabled: dialog.ready
            onTriggered: {
                ActionLog.taken("AlarmRuleDialog_save")
                dialog.close();
                dialog.saved(dialog.index, dialog.rule());
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Cancel")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("AlarmRuleDialog_cancel")
                dialog.close()
            }
        }
    ]

    AlarmWords {
        id: words
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Heading {
            level: 4
            text: dialog.i18nContext.i18nc("@title:group", "What should be watched?")
        }

        Repeater {
            model: words.reasons

            delegate: ColumnLayout {
                id: choice

                required property string modelData

                Layout.fillWidth: true
                spacing: 0

                Controls.RadioButton {
                    text: Plain.plain(words.reasonName(choice.modelData))
                    checked: dialog.reason === choice.modelData
                    onToggled: {
                        ActionLog.taken("AlarmRuleDialog_pickReason")
                        if (checked) {
                            dialog.pick(choice.modelData);
                        }
                    }
                }

                Controls.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.gridUnit * 1.5
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    font: Kirigami.Theme.smallFont
                    opacity: 0.75
                    text: words.reasonHelp(choice.modelData)
                }
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.ComboBox {
                id: accountBox

                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:listbox", "Account:")
                Accessible.name: Kirigami.FormData.label
                visible: dialog.onAccount && dialog.accounts.length > 0
                model: dialog.accounts
                textRole: "name"
                valueRole: "id"
                currentIndex: dialog.accounts.findIndex(a => a.id === dialog.account)
                onActivated: index => {
                    ActionLog.taken("AlarmRuleDialog_pickAccount")
                    dialog.account = dialog.accounts[index].id;
                }
            }

            Controls.Label {
                visible: dialog.onAccount && dialog.accounts.length === 0
                wrapMode: Text.Wrap
                text: dialog.i18nContext.i18nc("@info", "There are no accounts to pick yet.")
            }

            Controls.ComboBox {
                id: budgetBox

                Kirigami.FormData.label: dialog.reason === "account_reserve"
                    ? dialog.i18nContext.i18nc("@label:listbox the budget a reserve is counted in", "Of which budget:")
                    : dialog.i18nContext.i18nc("@label:listbox", "Budget:")
                Accessible.name: Kirigami.FormData.label
                visible: dialog.onBudget && dialog.budgets.length > 0
                model: dialog.budgets
                textRole: "name"
                valueRole: "id"
                currentIndex: dialog.budgets.findIndex(b => b.id === dialog.budget)
                onActivated: index => {
                    ActionLog.taken("AlarmRuleDialog_pickBudget")
                    dialog.budget = dialog.budgets[index].id;
                }
            }

            Controls.Label {
                visible: dialog.onBudget && dialog.budgets.length === 0
                wrapMode: Text.Wrap
                text: dialog.i18nContext.i18nc("@info", "There are no budgets to pick yet. Set one on the page Budgets first.")
            }

            Controls.SpinBox {
                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:spinbox a share of a budget's amount", "Share (%):")
                Accessible.name: Kirigami.FormData.label
                visible: dialog.reason === "budget_over" || dialog.reason === "budget_under"
                from: 1
                to: 1000
                value: dialog.share
                editable: true
                onValueModified: dialog.share = value
            }

            Controls.TextField {
                id: amountField

                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:textbox", "Amount:")
                Accessible.name: Kirigami.FormData.label
                visible: dialog.reason === "account_below"
                text: dialog.typed
                onTextEdited: dialog.typed = text
            }

            Controls.Label {
                visible: dialog.reason === "account_below" && dialog.typed.trim() !== "" && dialog.plain === "" && dialog.account !== ""
                wrapMode: Text.Wrap
                color: Kirigami.Theme.negativeTextColor
                text: dialog.i18nContext.i18nc("@info", "Type an amount such as 500 or 500,00, with no more decimal places than the account's currency has.")
            }

            Controls.SpinBox {
                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:spinbox how many periods of a budget a reserve is", "How many periods:")
                Accessible.name: Kirigami.FormData.label
                visible: dialog.reason === "account_reserve"
                from: 1
                to: 24
                value: dialog.times
                editable: true
                onValueModified: dialog.times = value
            }

            Controls.CheckBox {
                visible: dialog.onAccount
                text: dialog.i18nContext.i18nc("@option:check", "Check the lowest point of this month, not just today")
                checked: dialog.lowest
                onToggled: {
                    ActionLog.taken("AlarmRuleDialog_lowest")
                    dialog.lowest = checked;
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: dialog.onAccount
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font: Kirigami.Theme.smallFont
            opacity: 0.75
            text: dialog.i18nContext.i18nc("@info", "The lowest point is the smallest balance at the end of a day, from the start of this month to today. For an account you owe on, such as a credit card, a debt is a negative balance: under −1,000 means more than 1,000 owed.")
        }
    }
}
