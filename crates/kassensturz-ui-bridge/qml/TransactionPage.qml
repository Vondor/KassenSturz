// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Adds a transaction, or changes or removes one (M3.5, D-118): money paid
// or received, how much, to or from whom, what for and when. The other
// party reads "To" or "From" (D-034). Only a simple transaction, one account
// and one category, can be changed here: a starting balance is corrected
// through "Check against my bank" (D-116), a purchase broken down into Parts
// has its own page (M3.6, D-119), which "Break down this transaction" opens
// for a simple one too; a Transfer can only be removed from here. After a
// transaction is saved that looks like one side of a Transfer, the page asks
// whether it is (M3.7, D-120).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountRegister register
    // The account the transaction belongs to; empty to let the user choose
    // one, e.g. from the overview.
    property string accountId: ""
    // The transaction to change, by ID; empty to add a new one.
    property string transactionId: ""
    // Part of the first start rather than added from the overview.
    property bool firstRun: false
    property bool moneyIn: false

    readonly property bool adding: transactionId === ""
    // What the Rust side says about the transaction to change.
    readonly property var entry: adding ? ({}) : register.details(accountId, transactionId)
    readonly property string kind: entry.kind ?? "simple"
    readonly property bool editable: adding || kind === "simple"
    // The accounts to choose from, when the page wasn't opened for one.
    readonly property var accountChoices: register.accountChoices()
    // The account chosen: the page's, or the list's.
    readonly property string chosenAccount: accountId !== ""
        ? accountId
        : (accountChoices[accountBox.currentIndex] ?? ({})).id ?? ""
    // The categories to choose from, "Not sorted yet" first, and an amount
    // written the way the user types it. Both depend on the account and the
    // direction, and are read again when those change (a binding on the
    // invokables would report a loop).
    property var categoryChoices: []
    property string amountExample: ""

    onChosenAccountChanged: reload()
    onMoneyInChanged: reload()

    function reload() {
        const found = register.categoryChoices(chosenAccount, moneyIn, entry.category ?? "");
        categoryChoices = found.code === undefined ? found : [];
        amountExample = register.amountExample(chosenAccount);
    }

    // Added, changed, removed, or skipped during the first start.
    signal done()
    // The user wants "Check against my bank" for the account.
    signal checkAgainstBankClicked(string accountId)
    // The user wants to break the transaction down into Parts (D-017).
    signal breakDownClicked(string accountId, string transactionId)
    // The user wants a Rule made from this transaction (D-088).
    signal createRuleClicked(string accountId, string transactionId)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: firstRun
        ? i18nContext.i18nc("@title", "Your first transaction")
        : adding
            ? i18nContext.i18nc("@title", "Add a transaction")
            : kind === "starting-balance"
                ? i18nContext.i18nc("@title", "Starting balance")
                : kind === "transfer"
                    ? i18nContext.i18nc("@title", "Money moved between your accounts")
                    : i18nContext.i18nc("@title", "Change a transaction")

    header: FormError {
        id: error
    }

    Component.onCompleted: {
        reload();
        if (!adding) {
            moneyIn = entry.moneyIn ?? false;
            amountField.text = entry.amount ?? "";
            otherPartyField.text = entry.otherParty ?? "";
            dateField.text = entry.date ?? "";
            categoryBox.currentIndex = page.categoryIndex(entry.category ?? "");
        }
    }

    // The place of the category with `id` in the list; 0 is "Not sorted yet".
    function categoryIndex(id: string): int {
        return categoryChoices.findIndex(choice => choice.id === id) + 1;
    }

    // The ID of the category chosen, empty for none yet.
    function chosenCategory(): string {
        return categoryBox.currentIndex <= 0 ? "" : categoryChoices[categoryBox.currentIndex - 1].id;
    }

    // What the user typed, as the Rust side takes it.
    function form(): var {
        return {
            moneyIn: moneyIn,
            amount: amountField.text,
            otherParty: otherPartyField.text,
            category: chosenCategory(),
            date: dateField.text,
        };
    }

    function save() {
        const result = adding
            ? register.addTransaction(chosenAccount, form())
            : register.editTransaction(chosenAccount, transactionId, form());
        error.result = result;
        if (result.code === undefined) {
            // If it looks like one side of a Transfer, ask before leaving
            // (M3.7, D-120); the page is done when the questions are.
            transferPrompt.ask(register.transferSuggestions(adding ? result.id : transactionId));
        }
    }

    function remove() {
        const result = register.removeTransaction(chosenAccount, transactionId);
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    TransferPrompt {
        id: transferPrompt

        register: page.register
        onFinished: page.done()
    }

    Kirigami.FormLayout {
        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: page.firstRun
            text: page.i18nContext.i18nc("@info", "Now add something you paid or received, for example your last shopping trip.")
            wrapMode: Text.Wrap
        }

        // What can't be changed here, and where it can.
        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: !page.editable
            text: page.kind === "starting-balance"
                ? page.i18nContext.i18nc("@info", "This is the starting balance: what the account held when you began. To change it, check the account against your bank, and KassenSturz offers to correct it.")
                : page.kind === "transfer"
                    ? page.i18nContext.i18nc("@info %1 is the name of an account", "This money was moved between this account and %1. Changing a Transfer isn't possible yet. You can remove it and add each side again in its own account; KassenSturz then offers to join them.", page.entry.otherAccount ?? "")
                    : page.i18nContext.i18nc("@info", "This transaction is broken down into several parts, which have their own page.")
            wrapMode: Text.Wrap
        }

        Controls.Button {
            visible: page.kind === "starting-balance"
            text: page.i18nContext.i18nc("@action:button", "Check against my bank…")
            icon.name: "view-financial-account-checking"
            onClicked: page.checkAgainstBankClicked(page.chosenAccount)
        }

        Controls.Button {
            visible: page.kind === "parts"
            text: page.i18nContext.i18nc("@action:button", "Change the parts…")
            icon.name: "view-list-details"
            onClicked: page.breakDownClicked(page.chosenAccount, page.transactionId)
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: page.editable && (page.entry.imported ?? false)
            text: page.i18nContext.i18nc("@info", "This transaction came from your bank, so its amount stays as the bank sent it. You can change the rest.")
            wrapMode: Text.Wrap
        }

        Controls.ComboBox {
            id: accountBox
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Account:")
            model: page.accountChoices.map(choice => choice.name)
            // The account added last comes last; with one account there's
            // nothing to choose.
            currentIndex: count - 1
            visible: page.accountId === "" && count > 1
        }

        Controls.ButtonGroup {
            id: directionGroup
        }

        Controls.RadioButton {
            visible: page.editable
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "What happened?")
            text: page.i18nContext.i18nc("@option:radio", "I paid money")
            checked: !page.moneyIn
            Controls.ButtonGroup.group: directionGroup
            onToggled: page.moneyIn = false
        }

        Controls.RadioButton {
            visible: page.editable
            text: page.i18nContext.i18nc("@option:radio", "I received money")
            checked: page.moneyIn
            Controls.ButtonGroup.group: directionGroup
            onToggled: page.moneyIn = true
        }

        Controls.TextField {
            id: amountField
            visible: page.editable
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "How much?")
            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", page.amountExample)
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            readOnly: page.entry.imported ?? false
            onAccepted: page.save()
        }

        Controls.TextField {
            id: otherPartyField
            visible: page.editable
            Kirigami.FormData.label: page.moneyIn
                ? page.i18nContext.i18nc("@label:textbox who the money came from", "From:")
                : page.i18nContext.i18nc("@label:textbox who the money went to", "To:")
            placeholderText: page.moneyIn
                ? page.i18nContext.i18nc("@info:placeholder", "For example your employer")
                : page.i18nContext.i18nc("@info:placeholder", "For example the shop")
            onAccepted: page.save()
        }

        Controls.ComboBox {
            id: categoryBox
            visible: page.editable
            Kirigami.FormData.label: page.moneyIn
                ? page.i18nContext.i18nc("@label:listbox", "What kind of income?")
                : page.i18nContext.i18nc("@label:listbox", "What was it for?")
            model: [page.i18nContext.i18nc("@item:inlistbox the transaction gets no category yet", "Not sorted yet")]
                .concat(page.categoryChoices.map(choice => choice.path))
            Accessible.name: page.i18nContext.i18nc("@label:listbox", "The category")
        }

        Controls.TextField {
            id: dateField
            visible: page.editable
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Date:")
            text: page.register.today
            onAccepted: page.save()
        }

        RowLayout {
            Controls.Button {
                visible: page.editable
                text: page.adding
                    ? page.i18nContext.i18nc("@action:button", "Add transaction")
                    : page.i18nContext.i18nc("@action:button", "Save")
                icon.name: page.adding ? "list-add" : "document-save"
                onClicked: page.save()
            }

            Controls.Button {
                visible: !page.adding && page.kind !== "starting-balance"
                text: page.i18nContext.i18nc("@action:button", "Remove")
                icon.name: "edit-delete"
                onClicked: page.remove()
            }

            Controls.Button {
                visible: page.firstRun
                text: page.i18nContext.i18nc("@action:button", "Skip for now")
                flat: true
                onClicked: page.done()
            }
        }

        // One step away from the simple form (progressive disclosure,
        // AGENTS.md §6): a saved transaction can be broken down after the
        // fact (D-017).
        Controls.Button {
            visible: !page.adding && page.kind === "simple"
            text: page.i18nContext.i18nc("@action:button", "Break down this transaction…")
            icon.name: "view-list-details"
            flat: true
            onClicked: page.breakDownClicked(page.chosenAccount, page.transactionId)
        }

        // Likewise a Rule: sorting the next transactions like this one
        // automatically (M3.11).
        Controls.Button {
            visible: !page.adding && page.kind === "simple"
            text: page.i18nContext.i18nc("@action:button", "Create a Rule from this transaction…")
            icon.name: "view-filter"
            flat: true
            onClicked: page.createRuleClicked(page.chosenAccount, page.transactionId)
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: !page.adding && page.kind !== "starting-balance"
            text: page.i18nContext.i18nc("@info", "Removing takes the transaction out of this account's list and balance. Undo brings it back.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
