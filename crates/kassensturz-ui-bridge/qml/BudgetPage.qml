// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Sets a budget, or changes one (M6c.5, D-357, D-345, D-361). A new budget
// is for one spending category, repeats every month, quarter or year or runs
// for some days, and can carry what is left into the next period. A change
// applies from the start of one of the budget's periods on, so the results
// of earlier periods never change. The page shows which period comes first,
// and every mistake gets a plain message.

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
    readonly property string everyWord: adding ? words.everyWords[everyBox.currentIndex] : (existing?.every ?? "month")
    readonly property bool repeats: everyWord !== "range"
    // The periods to start with (new) or to change from (existing).
    property var starts: []
    readonly property var changes: existing?.changes ?? []
    readonly property string currency: adding ? (chosenCategory?.currency ?? "") : (existing?.currency ?? "")

    // The page is done: a budget was set, or the change was saved.
    signal done()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    BudgetWords {
        id: words
    }

    function loadStarts() {
        starts = adding && repeats ? budgets.startChoices(everyWord) : [];
        const current = starts.findIndex(choice => choice.current);
        startBox.currentIndex = current >= 0 ? current : 0;
    }

    onEveryWordChanged: loadStarts()

    Component.onCompleted: {
        if (adding && categoryId !== "") {
            const at = categoryChoices.findIndex(category => category.id === categoryId);
            categoryBox.currentIndex = at >= 0 ? at : 0;
        }
        if (!adding) {
            const current = changes.findIndex(choice => choice.current);
            fromBox.currentIndex = current >= 0 ? current : 0;
            amountField.text = existing?.amountInput ?? "";
            carryBox.checked = existing?.carryOver ?? false;
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

    function save() {
        let result;
        if (adding) {
            const first = repeats ? (starts[Math.max(0, startBox.currentIndex)]?.day ?? "") : firstDayField.text;
            result = budgets.addBudget(chosenCategory?.id ?? "", everyWord, amountField.text, repeats && carryBox.checked, first, lastDayField.text);
        } else {
            const from = changes[Math.max(0, fromBox.currentIndex)]?.day ?? "";
            result = budgets.changeBudget(budgetId, from, amountField.text, repeats && carryBox.checked);
        }
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    title: adding
        ? i18nContext.i18nc("@title", "Set a budget")
        : i18nContext.i18nc("@title %1 is the name of a category", "Change the budget for %1", existing?.name ?? "")
    header: FormError {
        id: error
    }

    Kirigami.FormLayout {
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

        Controls.Label {
            visible: !page.adding
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Category:")
            text: page.existing?.name ?? ""
            textFormat: Text.PlainText
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
            text: words.every(page.everyWord)
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
            onAccepted: page.save()
        }

        Controls.TextField {
            id: lastDayField

            visible: page.adding && !page.repeats
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox the last day of a budget for some days", "Last day:")
            placeholderText: page.i18nContext.i18nc("@info:placeholder", "For example 30 Nov 2026, in your regional format")
            onAccepted: page.save()
        }

        RowLayout {
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Amount:")

            Controls.TextField {
                id: amountField

                Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                inputMethodHints: Qt.ImhFormattedNumbersOnly
                placeholderText: page.i18nContext.i18nc("@info:placeholder", "What you plan to spend")
                onAccepted: page.save()
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
            text: page.adding
                ? page.i18nContext.i18nc("@action:button", "Set the budget")
                : page.i18nContext.i18nc("@action:button", "Save the change")
            icon.name: page.adding ? "list-add" : "document-save"
            enabled: !page.adding || page.categoryChoices.length > 0
            onClicked: page.save()
        }
    }
}
