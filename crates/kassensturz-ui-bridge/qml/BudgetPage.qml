// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Sets a budget, or changes one (M6c.5, D-357, D-345, D-361). A new budget
// is for one or more spending categories, may count only what was paid from
// some of the user's accounts (D-639), repeats every month, quarter or year
// or runs for some days, and can carry what is left into the next period. A change
// applies from the start of one of the budget's periods on, so the results
// of earlier periods never change. A budget can have a name, such as
// "Holiday", which the screens show instead of its categories (D-664); a
// change of only the name touches no amount. The page shows which period
// comes first, and every mistake gets a plain message.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property BudgetBook budgets
    // The budget to change, by ID; empty to set a new one.
    property string budgetId: ""
    // For a new budget: the category it starts with, by ID; may be empty.
    property string categoryId: ""

    readonly property bool adding: budgetId === ""
    readonly property var existing: adding ? null : (budgets.view.budgets ?? []).find(budget => budget.id === budgetId) ?? null
    readonly property var categoryChoices: budgets.view.categories ?? []
    // What the user chose.
    readonly property var chosenCategory: adding ? (categoryChoices[categoryBox.currentIndex] ?? null) : null
    // More categories counted with the first, by ID; only ones in its
    // currency are offered, since a budget has one currency.
    property var moreCategories: []
    readonly property var moreChoices: categoryChoices.filter(category => category.id !== chosenCategory?.id && category.currency === chosenCategory?.currency)
    readonly property var chosenCategories: chosenCategory === null ? [] : [chosenCategory.id].concat(severalBox.checked ? moreCategories.filter(id => moreChoices.some(category => category.id === id)) : [])
    // The accounts it counts from, by ID; none for every account.
    property var chosenAccounts: []
    readonly property var accountChoices: budgets.view.accounts ?? []
    readonly property string everyWord: adding ? words.everyWords[everyBox.currentIndex] : (existing?.every ?? "month")
    readonly property bool repeats: everyWord !== "range"
    // What the book is asked for: a salary month with its day (D-651).
    readonly property string everyKey: everyWord === "salary-month" ? "salary-month-" + salaryDayBox.value : everyWord
    // The periods to start with (new) or to change from (existing).
    property var starts: []
    readonly property var changes: existing?.changes ?? []
    readonly property string currency: adding ? (chosenCategory?.currency ?? "") : (existing?.currency ?? "")
    // The period a change applies from when the page opens, to tell whether
    // the user changed the amount part at all.
    property int firstFromIndex: 0

    // The page is done: a budget was set, or the change was saved.
    signal done()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    BudgetWords {
        id: words
    }

    function loadStarts() {
        starts = adding && repeats ? budgets.startChoices(everyKey) : [];
        const current = starts.findIndex(choice => choice.current);
        startBox.currentIndex = current >= 0 ? current : 0;
    }

    onEveryKeyChanged: loadStarts()

    Component.onCompleted: {
        if (adding && categoryId !== "") {
            const at = categoryChoices.findIndex(category => category.id === categoryId);
            categoryBox.currentIndex = at >= 0 ? at : 0;
        }
        if (!adding) {
            const current = changes.findIndex(choice => choice.current);
            firstFromIndex = current >= 0 ? current : 0;
            fromBox.currentIndex = firstFromIndex;
            amountField.text = existing?.amountInput ?? "";
            carryBox.checked = existing?.carryOver ?? false;
            nameField.text = existing?.ownName ?? "";
        }
        loadStarts();
    }

    // The first period the new budget will have, in words.
    readonly property string firstPeriod: {
        if (!adding || !repeats || starts.length === 0) {
            return "";
        }
        const choice = starts[Math.max(0, startBox.currentIndex)];
        return words.days(choice.firstDay, choice.lastDay);
    }

    // `list` with `id` in it when `on`, without it otherwise.
    function toggled(list, id, on) {
        const rest = list.filter(other => other !== id);
        return on ? rest.concat([id]) : rest;
    }

    function save() {
        let result;
        if (adding) {
            const first = repeats ? (starts[Math.max(0, startBox.currentIndex)]?.day ?? "") : firstDayField.text;
            const accounts = someAccountsBox.checked ? chosenAccounts : [];
            // The categories, then the accounts after a semicolon (D-639).
            result = budgets.addBudget(nameField.text, chosenCategories.join(",") + ";" + accounts.join(","), everyKey, amountField.text, repeats && carryBox.checked, first + "\n" + lastDayField.text);
        } else {
            const from = changes[Math.max(0, fromBox.currentIndex)]?.day ?? "";
            // Saving the same amount again would drop what was planned for
            // later periods (D-345), so a change of only the name sends none.
            const changesAmount = amountField.text !== (existing?.amountInput ?? "")
                || (repeats && carryBox.checked) !== (existing?.carryOver ?? false)
                || fromBox.currentIndex !== firstFromIndex;
            result = budgets.changeBudget(budgetId, nameField.text, changesAmount, from, amountField.text, repeats && carryBox.checked);
        }
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    title: Plain.plain(adding
        ? i18nContext.i18nc("@title", "Set a budget")
        : i18nContext.i18nc("@title %1 is the name of a category", "Change the budget for %1", existing?.name ?? ""))
    header: FormError {
        id: error
    }

    Kirigami.FormLayout {
        // A name of the user's own, which the screens show instead of the
        // categories (D-664). Empty: the budget is called by its categories.
        Controls.TextField {
            id: nameField

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox the name of a budget", "Name:")
            Layout.fillWidth: true
            maximumLength: 100
            placeholderText: page.i18nContext.i18nc("@info:placeholder", "Optional, for example Holiday")
            Accessible.name: page.i18nContext.i18nc("@label:textbox the name of a budget", "Name:")
            onAccepted: {
                ActionLog.taken("BudgetPage_save_5")
                page.save()
            }
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font: Kirigami.Theme.smallFont
            text: page.i18nContext.i18nc("@info", "Without a name, the budget is called by its categories.")
        }

        Controls.ComboBox {
            id: categoryBox

            visible: page.adding
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Category:")
            Layout.fillWidth: true
            model: page.categoryChoices
            textRole: "path"
            enabled: page.categoryChoices.length > 0
            Accessible.name: page.i18nContext.i18nc("@label:listbox", "Category:")
        }

        Controls.Label {
            visible: page.adding && page.categoryChoices.length === 0
            text: page.i18nContext.i18nc("@info", "You have no spending categories yet. Add a category for money you spend first.")
            wrapMode: Text.Wrap
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
        }

        Controls.CheckBox {
            id: severalBox

            visible: page.adding && page.moreChoices.length > 0
            text: page.i18nContext.i18nc("@option:check", "Count more categories with it")
        }

        ColumnLayout {
            visible: severalBox.visible && severalBox.checked
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Also count:")
            spacing: 0

            Repeater {
                model: page.moreChoices

                delegate: Controls.CheckBox {
                    required property var modelData

                    text: Plain.plain(modelData.path)
                    checked: page.moreCategories.includes(modelData.id)
                    onToggled: {
                        ActionLog.taken("BudgetPage_pickCategory")
                        page.moreCategories = page.toggled(page.moreCategories, modelData.id, checked)
                    }
                }
            }
        }

        Controls.Label {
            visible: !page.adding
            Kirigami.FormData.label: (page.existing?.categories ?? []).length > 1
                ? page.i18nContext.i18nc("@label", "Categories:")
                : page.i18nContext.i18nc("@label", "Category:")
            text: page.existing?.categoryNames ?? ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
        }

        Controls.CheckBox {
            id: someAccountsBox

            visible: page.adding && page.accountChoices.length > 1
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Accounts:")
            text: page.i18nContext.i18nc("@option:check", "Only count money paid from some accounts")
        }

        ColumnLayout {
            visible: someAccountsBox.visible && someAccountsBox.checked
            spacing: 0

            Repeater {
                model: page.accountChoices

                delegate: Controls.CheckBox {
                    required property var modelData

                    text: Plain.plain(modelData.name)
                    checked: page.chosenAccounts.includes(modelData.id)
                    onToggled: {
                        ActionLog.taken("BudgetPage_pickAccount")
                        page.chosenAccounts = page.toggled(page.chosenAccounts, modelData.id, checked)
                    }
                }
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font: Kirigami.Theme.smallFont
                text: page.i18nContext.i18nc("@info", "Nothing ticked: money paid from every account counts.")
            }
        }

        Controls.Label {
            visible: !page.adding && (page.existing?.accounts ?? "") !== ""
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Accounts:")
            text: page.i18nContext.i18nc("@info %1 is a list of account names", "Only money paid from %1", page.existing?.accounts ?? "")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
        }

        Controls.ComboBox {
            id: everyBox

            visible: page.adding
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "How often:")
            model: words.everyWords.map(word => words.every(word))
            Accessible.name: page.i18nContext.i18nc("@label:listbox", "How often:")
        }

        Controls.Label {
            visible: !page.adding
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "How often:")
            text: words.everyFrom(page.everyWord, page.existing?.salaryDay ?? 0)
            textFormat: Text.PlainText
        }

        // A salary month starts on a day the user chooses, such as payday
        // (D-651).
        Controls.SpinBox {
            id: salaryDayBox

            visible: page.adding && page.everyWord === "salary-month"
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:spinbox the day of the month a salary month starts on", "Starts on day:")
            from: 1
            to: 31
            editable: true
            value: 1
            Accessible.name: page.i18nContext.i18nc("@label:spinbox", "The day of the month the salary month starts on")
        }

        Controls.ComboBox {
            id: startBox

            visible: page.adding && page.repeats
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox the first period of a new budget", "Start with:")
            model: page.starts.map(choice => words.period(choice))
            Accessible.name: page.i18nContext.i18nc("@label:listbox the first period of a new budget", "Start with:")
        }

        Controls.Label {
            visible: page.firstPeriod !== ""
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@info %1 is a stretch of days", "The first period is %1.", page.firstPeriod)
        }

        Controls.TextField {
            id: firstDayField

            visible: page.adding && !page.repeats
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox the first day of a budget for some days", "First day:")
            placeholderText: page.i18nContext.i18nc("@info:placeholder", "For example 1 Nov 2026, in your regional format")
            onAccepted: {
                ActionLog.taken("BudgetPage_save")
                page.save()
            }
        }

        Controls.TextField {
            id: lastDayField

            visible: page.adding && !page.repeats
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox the last day of a budget for some days", "Last day:")
            placeholderText: page.i18nContext.i18nc("@info:placeholder", "For example 30 Nov 2026, in your regional format")
            onAccepted: {
                ActionLog.taken("BudgetPage_save_2")
                page.save()
            }
        }

        RowLayout {
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Amount:")

            Controls.TextField {
                id: amountField

                Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                inputMethodHints: Qt.ImhFormattedNumbersOnly
                placeholderText: page.i18nContext.i18nc("@info:placeholder", "What you plan to spend")
                onAccepted: {
                    ActionLog.taken("BudgetPage_save_3")
                    page.save()
                }
            }

            Controls.Label {
                text: page.currency
                textFormat: Text.PlainText
            }
        }

        Controls.ComboBox {
            id: fromBox

            visible: !page.adding
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox the period a change starts to apply from", "From this period on:")
            model: page.changes.map((choice, at) => at === page.changes.length - 1 && page.changes.length > 1
                ? page.i18nContext.i18nc("@item:inlistbox %1 is a stretch of days", "%1 (the first period, to correct it from the start)", words.days(choice.firstDay, choice.lastDay))
                : words.period(choice))
            Accessible.name: page.i18nContext.i18nc("@label:listbox the period a change starts to apply from", "From this period on:")
        }

        Controls.Label {
            visible: !page.adding
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font: Kirigami.Theme.smallFont
            text: page.i18nContext.i18nc("@info", "Earlier periods keep what they had, so their results don't change.")
        }

        Controls.CheckBox {
            id: carryBox

            visible: page.repeats
            text: page.i18nContext.i18nc("@option:check", "Carry over what is left")
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "At the end of a period:")
        }

        Controls.Label {
            visible: page.repeats
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font: Kirigami.Theme.smallFont
            text: page.i18nContext.i18nc("@info", "What is left at the end of a period, or what was spent beyond it, moves into the next one.")
        }

        Controls.Button {
            text: Plain.plain(page.adding
                ? page.i18nContext.i18nc("@action:button", "Set the budget")
                : page.i18nContext.i18nc("@action:button", "Save the change"))
            icon.name: page.adding ? "list-add" : "document-save"
            enabled: !page.adding || page.categoryChoices.length > 0
            onClicked: {
                ActionLog.taken("BudgetPage_save_4")
                page.save()
            }
        }
    }
}
