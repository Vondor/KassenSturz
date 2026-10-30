// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The review of the changes the AI prepared (M5.8, D-047, D-200, threat
// T-27): every change in words, the new categories, the new Rules and the
// transactions that get a category or have other fields changed (D-257), with the amount and day as the register
// shows them, and a chart the AI proposes (M6a.9, D-282) with a preview of real figures. Nothing has changed yet. The confirm button below is the only
// way to apply the set (`AiChat.confirm`): all of it is made or none, as one
// change that Undo takes back, which this page offers once it is done. The
// texts come from the model or the book and are shown as plain text (T-08).
// Before confirming, the user may correct a change, leave it out or take the
// AI's version back (M6b.8, D-293, `AiChangeCorrection`); the set stays the
// AI's. Hidden with the rest of AI while the switch is off (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AiSwitch ai
    required property AiChat chat
    // The set to read, by the ID the chat gave it.
    required property string setId

    // The set was applied or thrown away.
    signal applied(string setId)
    signal discarded(string setId)
    // The user wants the applied set taken back.
    signal undoClicked()
    signal doneClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The changes, or an error object when the set can't be shown.
    property var changes: []
    // The categories a change may be given (D-293).
    property var choices: []
    // What confirm answered, once it was pressed.
    property var result: ({})
    // What the last correction answered when it wasn't kept.
    property var correctionResult: ({})
    property bool done: false

    // The changes the set holds, and those the user left out.
    readonly property var kept: changes.filter ? changes.filter(change => !change.leftOut) : []
    readonly property var leftOut: changes.filter ? changes.filter(change => change.leftOut) : []
    readonly property var newCategories: kept.filter(change => change.kind === "newCategory")
    readonly property var editedCategories: kept.filter(change => change.kind === "editCategory")
    readonly property var editedRules: kept.filter(change => change.kind === "editRule")
    readonly property var movedRules: kept.filter(change => change.kind === "moveRule")
    readonly property var deletedRules: kept.filter(change => change.kind === "deleteRule")
    readonly property var newRules: kept.filter(change => change.kind === "newRule")
    readonly property var transactions: kept.filter(change => change.kind === "categorize")
    readonly property var newTransactions: kept.filter(change => change.kind === "newTransaction")
    readonly property var removedTransactions: kept.filter(change => change.kind === "deleteTransaction")
    readonly property var editedTransactions: kept.filter(change => change.kind === "editTransaction")
    readonly property var deletedCategories: kept.filter(change => change.kind === "deleteCategory")
    readonly property var newAccounts: kept.filter(change => change.kind === "newAccount")
    readonly property var newViews: kept.filter(change => change.kind === "newView")
    readonly property var editedAccounts: kept.filter(change => change.kind === "editAccount")
    readonly property bool shows: changes.code === undefined && !done

    // The name of a field of a transaction the AI changes.
    function transactionFieldName(field: string): string {
        if (field === "amount") {
            return i18nContext.i18nc("@label a field of a transaction", "Amount");
        }
        if (field === "date") {
            return i18nContext.i18nc("@label a field of a transaction", "Date");
        }
        if (field === "payee") {
            return i18nContext.i18nc("@label a field of a transaction", "Who paid or was paid");
        }
        if (field === "account") {
            return i18nContext.i18nc("@label a field of a transaction", "Account");
        }
        return i18nContext.i18nc("@label a field of a transaction", "Note");
    }

    AccountTypes {
        id: accountTypes
    }

    // The name of a period of a chart.
    function periodName(id: string): string {
        switch (id) {
        case "this-month":
            return i18nContext.i18nc("@item:inlistbox a period", "This month");
        case "last-month":
            return i18nContext.i18nc("@item:inlistbox a period", "Last month");
        case "last-12-months":
            return i18nContext.i18nc("@item:inlistbox a period", "The last 12 months");
        case "this-year":
            return i18nContext.i18nc("@item:inlistbox a period", "This year");
        case "last-year":
            return i18nContext.i18nc("@item:inlistbox a period", "Last year");
        }
        return "";
    }

    // A Rule's condition in words.
    function conditionText(condition: var): string {
        if (condition.kind === "text") {
            return i18nContext.i18nc("@info %1 is what a Rule looks at, %2 how it compares, %3 the text", "%1 %2 “%3”", fieldName(condition.field), operatorName(condition.operator), condition.value);
        }
        if (condition.kind === "amount") {
            const way = condition.direction === "in" ? i18nContext.i18nc("@info", "Money received") : i18nContext.i18nc("@info", "Money paid");
            if (condition.amountTest === "exactly") {
                return i18nContext.i18nc("@info %1 is money paid or received, %2 an amount, %3 a currency", "%1: exactly %2 %3", way, condition.amount, condition.currency);
            }
            if (condition.amountTest === "between") {
                return i18nContext.i18nc("@info %1 is money paid or received, %2 and %3 amounts, %4 a currency", "%1: between %2 and %3 %4", way, condition.amount, condition.amountTo, condition.currency);
            }
            return way;
        }
        return i18nContext.i18nc("@info %1 is the kind of a condition", "Another condition (%1)", condition.kind);
    }

    function fieldName(field: string): string {
        switch (field) {
        case "other-party-name":
            return i18nContext.i18nc("@item:inlistbox what a Rule looks at: the name the bank gave for who paid or was paid", "Other party");
        case "other-party-iban":
            return i18nContext.i18nc("@item:inlistbox what a Rule looks at: the other party's account number", "Other party's IBAN");
        case "purpose":
            return i18nContext.i18nc("@item:inlistbox what a Rule looks at: the text a payment came with", "Purpose");
        default:
            return i18nContext.i18nc("@item:inlistbox what a Rule looks at: the name shown as From or To", "Name shown");
        }
    }

    function operatorName(operator: string): string {
        switch (operator) {
        case "is":
            return i18nContext.i18nc("@item:inlistbox a text has to", "is exactly");
        case "starts-with":
            return i18nContext.i18nc("@item:inlistbox a text has to", "starts with");
        case "ends-with":
            return i18nContext.i18nc("@item:inlistbox a text has to", "ends with");
        case "pattern":
            return i18nContext.i18nc("@item:inlistbox a text has to", "matches the pattern");
        default:
            return i18nContext.i18nc("@item:inlistbox a text has to", "contains");
        }
    }

    // Shows what a correction answered: the set's changes, or why it
    // wasn't kept. True when it was kept.
    function corrected(answer: var): bool {
        if (answer.code !== undefined) {
            correctionResult = answer;
            return false;
        }
        correctionResult = {};
        changes = answer;
        choices = chat.changeChoices(setId);
        return true;
    }

    function editChange(slot: string, fields: var): bool {
        return corrected(chat.editChange(setId, slot, JSON.stringify(fields)));
    }

    function leaveOut(slot: string): bool {
        return corrected(chat.leaveOutChange(setId, slot));
    }

    function backToAi(slot: string): bool {
        return corrected(chat.backToAi(setId, slot));
    }

    // What a change the user left out is about, in a few words.
    function shortText(change: var): string {
        switch (change.kind) {
        case "newCategory":
        case "deleteCategory":
            return change.path;
        case "editCategory":
            return change.before;
        case "newRule":
        case "deleteRule":
        case "moveRule":
            return change.name;
        case "editRule":
            return change.beforeName;
        case "newAccount":
            return change.name;
        case "editAccount":
            return change.before;
        case "newView":
            return change.title;
        }
        return i18nContext.i18nc("@info %1 is a date, %2 who was paid or paid, %3 an amount", "%1 · %2 · %3", change.date, change.payee, change.amount);
    }

    function kindName(kind: string): string {
        switch (kind) {
        case "newCategory":
            return i18nContext.i18nc("@info a kind of prepared change", "New category");
        case "editCategory":
            return i18nContext.i18nc("@info a kind of prepared change", "Changed category");
        case "deleteCategory":
            return i18nContext.i18nc("@info a kind of prepared change", "Category to delete");
        case "newRule":
            return i18nContext.i18nc("@info a kind of prepared change", "New Rule");
        case "editRule":
            return i18nContext.i18nc("@info a kind of prepared change", "Changed Rule");
        case "moveRule":
            return i18nContext.i18nc("@info a kind of prepared change", "Rule in a new place");
        case "deleteRule":
            return i18nContext.i18nc("@info a kind of prepared change", "Rule to delete");
        case "categorize":
            return i18nContext.i18nc("@info a kind of prepared change", "Transaction that gets a category");
        case "editTransaction":
            return i18nContext.i18nc("@info a kind of prepared change", "Transaction that changes");
        case "newTransaction":
            return i18nContext.i18nc("@info a kind of prepared change", "Payment in cash that is added");
        case "deleteTransaction":
            return i18nContext.i18nc("@info a kind of prepared change", "Payment in cash that is removed");
        case "newAccount":
            return i18nContext.i18nc("@info a kind of prepared change", "New account");
        case "editAccount":
            return i18nContext.i18nc("@info a kind of prepared change", "Changed account");
        }
        return i18nContext.i18nc("@info a kind of prepared change", "New chart");
    }

    function typeName(id: string): string {
        switch (id) {
        case "chart.bar":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Bars");
        case "chart.pie":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Pie");
        case "chart.donut":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Ring");
        case "chart.line":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Line");
        case "chart.area":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Filled line");
        case "chart.stacked_bar":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Stacked bars");
        case "kpi.card":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "One figure");
        case "progress.bar":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Progress bar");
        case "table.rows":
            return i18nContext.i18nc("@item:inlistbox a way to show figures", "Table");
        }
        return id;
    }

    // The user's yes: the only call that applies the set.
    function confirm() {
        result = chat.confirm(setId);
        if (result.ok === true) {
            done = true;
            applied(setId);
        }
    }

    function throwAway() {
        chat.discard(setId);
        discarded(setId);
    }

    title: i18nContext.i18nc("@title", "Changes the AI prepared")
    visible: ai.enabled
    Component.onCompleted: {
        changes = chat.changes(setId);
        choices = chat.changeChoices(setId);
    }

    // Real buttons below the changes, not page actions: Kirigami moves
    // actions into a menu when the page is narrow, as it is beside the chat,
    // and the one button that applies the set has to be in plain sight.
    footer: Controls.ToolBar {
        visible: page.shows

        contentItem: RowLayout {
            spacing: Kirigami.Units.smallSpacing

            Item {
                Layout.fillWidth: true
            }

            Controls.Button {
                icon.name: "edit-delete"
                text: page.i18nContext.i18nc("@action:button throw the prepared changes away", "Throw away")
                onClicked: page.throwAway()
            }

            Controls.Button {
                highlighted: true
                enabled: page.kept.length > 0
                icon.name: "dialog-ok-apply"
                text: page.i18nContext.i18nc("@action:button make all the changes shown", "Confirm and make these changes")
                onClicked: page.confirm()
            }
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Information
            visible: page.shows
            text: page.i18nContext.i18nc("@info", "The AI prepared these changes. Nothing in your book has changed yet. Read them: they are made together when you confirm, and you can take them back with Undo.")
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Information
            visible: page.shows
            text: page.i18nContext.i18nc("@info", "You can change a new category, a new Rule, the category a transaction gets and a new chart before you confirm, or leave any change out. What you change stays between you and KassenSturz: it isn't sent to the AI.")
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            visible: page.shows && page.kept.length === 0
            text: page.i18nContext.i18nc("@info", "You left every change out, so there is nothing to confirm. Put a change back below, or throw the changes away.")
        }

        AiErrorMessage {
            Layout.fillWidth: true
            ai: page.ai
            result: page.changes.code !== undefined ? page.changes : (page.correctionResult.code !== undefined ? page.correctionResult : page.result)
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Positive
            visible: page.done
            text: page.i18nContext.i18nc("@info", "The changes were made. You can take them back with Undo.")
            actions: [
                Kirigami.Action {
                    icon.name: "edit-undo"
                    text: page.i18nContext.i18nc("@action:button take the last change back", "Undo")
                    onTriggered: page.undoClicked()
                },
                Kirigami.Action {
                    icon.name: "dialog-close"
                    text: page.i18nContext.i18nc("@action:button leave this page", "Close")
                    onTriggered: page.doneClicked()
                }
            ]
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.newCategories.length > 0
            text: page.i18nContext.i18nc("@title:group", "New categories")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.newCategories : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.path
                    }

                    Controls.Label {
                        text: modelData.income ? page.i18nContext.i18nc("@info", "For money you receive") : page.i18nContext.i18nc("@info", "For money you spend")
                        textFormat: Text.PlainText
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.editedCategories.length > 0
            text: page.i18nContext.i18nc("@title:group", "Changed categories")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.editedCategories : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 and %2 are paths of categories", "%1 → %2", modelData.before, modelData.after)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info", "Its subcategories and its transactions stay with it.")
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.deletedCategories.length > 0
            text: page.i18nContext.i18nc("@title:group", "Categories to delete")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.deletedCategories : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.path
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info", "Nothing uses it: no transactions, no subcategories and no Rule. Undo brings it back.")
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.newAccounts.length > 0
            text: page.i18nContext.i18nc("@title:group", "New accounts")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.newAccounts : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.name
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is a kind of account such as Cash, %2 a currency code", "%1 in %2, with no balance yet.", accountTypes.nameOf(modelData.accountType), modelData.currency)
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.editedAccounts.length > 0
            text: page.i18nContext.i18nc("@title:group", "Changed accounts")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.editedAccounts : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.before !== modelData.after
                            ? page.i18nContext.i18nc("@info %1 is the old name of an account, %2 the new one", "%1 → %2", modelData.before, modelData.after)
                            : modelData.before
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        visible: modelData.closed !== modelData.wasClosed
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: modelData.closed
                            ? page.i18nContext.i18nc("@info", "It is closed. Its history stays, and you can open it again.")
                            : page.i18nContext.i18nc("@info", "It is opened again.")
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.newRules.length > 0
            text: page.i18nContext.i18nc("@title:group", "New Rules")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.newRules : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.name
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is the path of a category", "Books matching transactions to %1.", modelData.bookTo)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.matchAll ? page.i18nContext.i18nc("@info", "When all of these hold:") : page.i18nContext.i18nc("@info", "When one of these holds:")
                    }

                    Repeater {
                        model: modelData.conditions

                        Controls.Label {
                            required property var modelData

                            Layout.fillWidth: true
                            Layout.leftMargin: Kirigami.Units.largeSpacing
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: "• " + page.conditionText(modelData)
                        }
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.editedRules.length > 0
            text: page.i18nContext.i18nc("@title:group", "Changed Rules")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.editedRules : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.beforeName !== modelData.name
                            ? page.i18nContext.i18nc("@info %1 is the old name of a Rule, %2 the new one", "%1 → %2", modelData.beforeName, modelData.name)
                            : modelData.name
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: modelData.bookTo !== ""
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.beforeBookTo !== modelData.bookTo
                            ? page.i18nContext.i18nc("@info %1 and %2 are paths of categories", "Books matching transactions to %2 (before: %1).", modelData.beforeBookTo, modelData.bookTo)
                            : page.i18nContext.i18nc("@info %1 is the path of a category", "Books matching transactions to %1.", modelData.bookTo)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.matchAll ? page.i18nContext.i18nc("@info", "When all of these hold:") : page.i18nContext.i18nc("@info", "When one of these holds:")
                    }

                    Repeater {
                        model: modelData.conditions

                        Controls.Label {
                            required property var modelData

                            Layout.fillWidth: true
                            Layout.leftMargin: Kirigami.Units.largeSpacing
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: "• " + page.conditionText(modelData)
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info", "Transactions it sorted earlier stay as they are.")
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.movedRules.length > 0
            text: page.i18nContext.i18nc("@title:group", "Rules in a new order")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.movedRules : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.name
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 and %2 are places in the list of Rules, counted from 1", "Moves from place %1 to place %2. The first Rule that fits a transaction wins.", modelData.from, modelData.to)
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.deletedRules.length > 0
            text: page.i18nContext.i18nc("@title:group", "Rules that are deleted")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.deletedRules : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.name
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: modelData.bookTo !== ""
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is the path of a category", "Booked matching transactions to %1.", modelData.bookTo)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.matchAll ? page.i18nContext.i18nc("@info", "When all of these hold:") : page.i18nContext.i18nc("@info", "When one of these holds:")
                    }

                    Repeater {
                        model: modelData.conditions

                        Controls.Label {
                            required property var modelData

                            Layout.fillWidth: true
                            Layout.leftMargin: Kirigami.Units.largeSpacing
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: "• " + page.conditionText(modelData)
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info", "Transactions it sorted earlier stay as they are.")
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.editedTransactions.length > 0
            text: page.i18nContext.i18nc("@title:group", "Transactions that change")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.editedTransactions : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    RowLayout {
                        Layout.fillWidth: true

                        Controls.Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            font.bold: true
                            textFormat: Text.PlainText
                            text: modelData.payee !== "" ? modelData.payee : page.i18nContext.i18nc("@info a transaction without a name", "(no name)")
                        }

                        Controls.Label {
                            textFormat: Text.PlainText
                            text: modelData.amount
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is a date, %2 the name of an account", "%1 · %2", modelData.date, modelData.account)
                    }

                    Repeater {
                        model: modelData.lines

                        Controls.Label {
                            required property var modelData

                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: modelData.before !== ""
                                ? page.i18nContext.i18nc("@info %1 is the name of a field, %2 its value now, %3 the value it becomes", "%1: %2 → %3", page.transactionFieldName(modelData.field), modelData.before, modelData.after !== "" ? modelData.after : page.i18nContext.i18nc("@info the value is removed", "(none)"))
                                : page.i18nContext.i18nc("@info %1 is the name of a field, %2 the value it becomes", "%1: %2 (none before)", page.transactionFieldName(modelData.field), modelData.after)
                        }
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.newTransactions.length > 0
            text: page.i18nContext.i18nc("@title:group", "Payments in cash that are added")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.newTransactions : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    RowLayout {
                        Layout.fillWidth: true

                        Controls.Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            font.bold: true
                            textFormat: Text.PlainText
                            text: modelData.payee !== "" ? modelData.payee : page.i18nContext.i18nc("@info a transaction without a name", "(no name)")
                        }

                        Controls.Label {
                            textFormat: Text.PlainText
                            text: modelData.amount
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is a date, %2 the name of an account", "%1 · %2", modelData.date, modelData.account)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: modelData.category !== ""
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is the path of a category", "Category: %1", modelData.category)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: modelData.note !== ""
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is a note the user can read", "Note: %1", modelData.note)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info", "Added to your cash account. Undo removes it again.")
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.removedTransactions.length > 0
            text: page.i18nContext.i18nc("@title:group", "Payments in cash that are removed")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.removedTransactions : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    RowLayout {
                        Layout.fillWidth: true

                        Controls.Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            font.bold: true
                            textFormat: Text.PlainText
                            text: modelData.payee !== "" ? modelData.payee : page.i18nContext.i18nc("@info a transaction without a name", "(no name)")
                        }

                        Controls.Label {
                            textFormat: Text.PlainText
                            text: modelData.amount
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is a date, %2 the name of an account", "%1 · %2", modelData.date, modelData.account)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: modelData.category !== ""
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is the path of a category", "Category: %1", modelData.category)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info", "The balance of the cash account changes. Undo brings it back.")
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.transactions.length > 0
            text: page.i18nContext.i18nc("@title:group", "Transactions that get a category")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.transactions : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    RowLayout {
                        Layout.fillWidth: true

                        Controls.Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            font.bold: true
                            textFormat: Text.PlainText
                            text: modelData.payee !== "" ? modelData.payee : page.i18nContext.i18nc("@info a transaction without a name", "(no name)")
                        }

                        Controls.Label {
                            textFormat: Text.PlainText
                            text: modelData.amount
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info %1 is a date, %2 the name of an account", "%1 · %2", modelData.date, modelData.account)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: modelData.before !== ""
                            ? page.i18nContext.i18nc("@info %1 and %2 are paths of categories", "Category: %1 → %2", modelData.before, modelData.after)
                            : page.i18nContext.i18nc("@info %1 is the path of a category", "Category: %1 (none before)", modelData.after)
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.newViews.length > 0
            text: page.i18nContext.i18nc("@title:group", "New charts")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.newViews : []

            Kirigami.AbstractCard {
                id: chartCard

                required property var modelData

                Layout.fillWidth: true

                // The chart is read by the same code as one made by hand, so
                // the preview shows exactly what would be saved.
                ComponentProperties {
                    id: chartProperties
                }

                Component.onCompleted: chartProperties.load(modelData.component)

                contentItem: ColumnLayout {
                    Controls.Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: chartCard.modelData.title
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.periodName(chartCard.modelData.period)
                    }

                    ComponentPreview {
                        Layout.fillWidth: true
                        subject: chartProperties.view
                        period: chartCard.modelData.period
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        textFormat: Text.PlainText
                        text: page.i18nContext.i18nc("@info", "The AI chose what this chart shows. The figures are worked out by KassenSturz from your book, not by the AI. It is saved in the Statistic Builder only when you confirm, and Undo removes it again.")
                    }

                    AiChangeCorrection {
                        Layout.fillWidth: true
                        ai: page.ai
                        change: chartCard.modelData
                        choices: page.choices
                        review: page
                    }
                }
            }
        }

        Kirigami.Heading {
            level: 3
            visible: page.shows && page.leftOut.length > 0
            text: page.i18nContext.i18nc("@title:group prepared changes the user left out", "Left out")
            textFormat: Text.PlainText
        }

        Repeater {
            model: page.shows ? page.leftOut : []

            Kirigami.AbstractCard {
                required property var modelData

                Layout.fillWidth: true

                contentItem: RowLayout {
                    ColumnLayout {
                        Layout.fillWidth: true

                        Controls.Label {
                            Layout.fillWidth: true
                            opacity: 0.8
                            textFormat: Text.PlainText
                            text: page.kindName(modelData.kind)
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: page.shortText(modelData)
                        }
                    }

                    Controls.Button {
                        icon.name: "edit-undo"
                        text: page.i18nContext.i18nc("@action:button bring a prepared change the user left out back into the set", "Put it back")
                        onClicked: page.backToAi(modelData.slot)
                    }
                }
            }
        }
    }
}
