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
// whether it is (M3.7, D-120), and a saved one can be joined with an entry
// of another account the user picks, even one sorted already (US3.3, D-692).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountRegister register
    // The planned payments, for "Plan this again…" (US7.1); none hides it.
    property var plans: null
    // The account the transaction belongs to; empty to let the user choose
    // one, e.g. from the overview.
    property string accountId: ""
    // The transaction to change, by ID; empty to add a new one.
    property string transactionId: ""
    property bool moneyIn: false

    readonly property bool adding: transactionId === ""
    // What the Rust side says about the transaction to change; read again
    // when the book changed while the page is open (M6-B234).
    property var entry: adding ? ({}) : register.details(accountId, transactionId)
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
    // The category shown, by ID: a new list (another direction or account,
    // or the book read again) resets the box to its first entry, "Not
    // sorted yet", so the page puts the category back by its ID, never by
    // its place (M6-B234).
    property string categoryId: ""

    onChosenAccountChanged: reload()
    onMoneyInChanged: reload()

    function reload() {
        const stored = entry.category ?? "";
        const found = register.categoryChoices(chosenAccount, moneyIn, stored);
        const choices = [];
        if (found.code === undefined) {
            for (let i = 0; i < found.length; ++i) {
                choices.push(found[i]);
            }
        }
        // The category the transaction has, even when the list doesn't
        // offer it (e.g. one in another currency): shown as such, and kept
        // when the form is saved, never "Not sorted yet" in its place.
        if (stored !== "" && !choices.some(choice => choice.id === stored)) {
            choices.push({
                id: stored,
                path: i18nContext.i18nc("@item:inlistbox", "The category it has now (not in this list)"),
            });
        }
        categoryChoices = choices;
        categoryBox.currentIndex = categoryIndex(categoryId);
        tagField.choices = register.tagChoices();
        amountExample = register.amountExample(chosenAccount);
    }

    // Fills the form with the stored transaction.
    function fill() {
        moneyIn = entry.moneyIn ?? false;
        amountField.text = entry.amount ?? "";
        otherPartyField.text = entry.otherParty ?? "";
        dateField.text = entry.date ?? "";
        noteField.text = entry.note ?? "";
        loadedNote = noteField.text;
        tagField.tags = entry.tags ?? [];
        loadedTags = tagsText();
        categoryId = entry.category ?? "";
        reload();
        loadedFields = fieldsText();
    }

    // The book changed while the page is open, e.g. the AI changed this
    // transaction from the chat: the form shows what is stored now, unless
    // the user typed something, which stays as typed (M6-B234).
    Connections {
        target: page.register

        function onModelReset() {
            if (page.adding || page.unsaved()) {
                return;
            }
            const found = page.register.details(page.accountId, page.transactionId);
            if (found.code === undefined) {
                page.entry = found;
                page.fill();
            }
        }
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

    title: Plain.plain(adding
            ? i18nContext.i18nc("@title", "Add a transaction")
            : kind === "starting-balance"
                ? i18nContext.i18nc("@title", "Starting balance")
                : kind === "transfer"
                    ? i18nContext.i18nc("@title", "Money moved between your accounts")
                    : i18nContext.i18nc("@title", "Change a transaction"))

    header: FormError {
        id: error
    }

    Component.onCompleted: {
        reload();
        if (!adding) {
            fill();
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

    // The note as it was loaded, so a save sends it only when the user
    // changed it (D-703): an imported note the page can't take as typed
    // text, e.g. a long memo, then stays exactly as it is.
    property string loadedNote: ""
    // The longest note the page takes: 512, or the loaded note's own length
    // if it is longer, so an imported memo can be changed without being cut
    // (D-703, B-226).
    readonly property int noteLimit: register.noteLimit(entry.note ?? "")

    // `typed` is `before` with text put in (and maybe some taken out) at one
    // place. Returns `{text, cursor}`: `before` with as much of the new text
    // as keeps the note within `limit`, and the cursor after it. The length
    // is counted by Rust, as a save counts it.
    function fitNote(before: string, typed: string, limit: int): var {
        let start = 0;
        while (start < before.length && start < typed.length && before[start] === typed[start]) {
            start++;
        }
        let end = 0;
        while (end < before.length - start && end < typed.length - start && before[before.length - 1 - end] === typed[typed.length - 1 - end]) {
            end++;
        }
        const head = typed.slice(0, start);
        const added = typed.slice(start, typed.length - end);
        const tail = typed.slice(typed.length - end);
        // The most of `added` that fits, found by halving.
        let low = 0;
        let high = added.length;
        while (low < high) {
            const middle = Math.ceil((low + high) / 2);
            if (register.noteLength(head + added.slice(0, middle) + tail) <= limit) {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        // Never keep half of a character that takes two UTF-16 units.
        if (low > 0 && low < added.length) {
            const unit = added.charCodeAt(low - 1);
            if (unit >= 0xD800 && unit <= 0xDBFF) {
                low--;
            }
        }
        const kept = head + added.slice(0, low) + tail;
        // Even the new text cut to nothing doesn't fit (the user replaced a
        // short piece with a longer one at the limit): keep the note as it was.
        if (register.noteLength(kept) > limit) {
            return {text: before, cursor: start};
        }
        return {text: kept, cursor: start + low};
    }

    // The other fields as they were loaded or last saved, so a step that
    // leaves the page can tell whether the user typed something (M6-B227).
    property string loadedFields: ""

    function fieldsText(): string {
        return JSON.stringify([moneyIn, amountField.text, otherPartyField.text, chosenCategory(), dateField.text, tagsText()]);
    }

    // The tags as they were loaded or last saved, so a save sends them
    // only when the user changed them (US4.8).
    property string loadedTags: "[]"

    function tagsText(): string {
        return JSON.stringify(tagField.tags.map(tag => [tag.id, tag.name]));
    }

    // Whether the form holds something the user typed and didn't save.
    function unsaved(): bool {
        return !adding && editable && (fieldsText() !== loadedFields || noteField.text !== loadedNote);
    }

    // Saves what the user typed before a step that leaves the page: joining
    // it with the other side of a move, breaking it down or making a Rule
    // from it work on the saved transaction, and the page closes after
    // them, so a note typed but not saved was lost without a word
    // (M6-B227). Saving is its own change, which Undo takes back on its
    // own. False when it couldn't be saved; the form then says why and the
    // step waits.
    function keepTyped(): bool {
        if (!unsaved()) {
            return true;
        }
        const result = register.editTransaction(chosenAccount, transactionId, form());
        error.result = result;
        if (result.code !== undefined) {
            return false;
        }
        loadedNote = noteField.text;
        loadedTags = tagsText();
        loadedFields = fieldsText();
        return true;
    }

    // What the user typed, as the Rust side takes it. Without a `note` key
    // the Rust side leaves the note as it is.
    function form(): var {
        const typed = {
            moneyIn: moneyIn,
            amount: amountField.text,
            otherParty: otherPartyField.text,
            category: chosenCategory(),
            date: dateField.text,
        };
        if (noteField.text !== loadedNote) {
            typed.note = noteField.text;
        }
        // Without a `tags` key the Rust side leaves the tags as they are.
        if (adding || tagsText() !== loadedTags) {
            typed.tags = tagField.tags;
        }
        return typed;
    }

    // Saves the tags of a Transfer (Q-535); nothing else of it changes.
    function saveTransferTags() {
        const result = register.setTransferTags(chosenAccount, transactionId, { tags: tagField.tags });
        error.result = result;
        if (result.code === undefined) {
            loadedTags = tagsText();
        }
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

    // A transaction with what a bank or file delivered is removed only
    // after the plain warning (D-375); one typed in is removed at once.
    function remove() {
        const notice = register.removalNotice(chosenAccount, transactionId);
        if (notice.code !== undefined) {
            error.result = notice;
        } else if (notice.holdsOriginal) {
            removeOriginal.ask(notice);
        } else {
            removeNow();
        }
    }

    // The bills linked to the transaction (D-739), read again after a link
    // is taken off, so the fields typed meanwhile stay.
    property var bills: entry.bills ?? []

    function removeBill(link: string) {
        const result = register.removeBill(accountId, transactionId, link);
        error.result = result;
        if (result.code === undefined) {
            bills = register.details(accountId, transactionId).bills ?? [];
        }
    }

    function removeNow() {
        const result = register.removeTransaction(chosenAccount, transactionId);
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    RemoveOriginalDialog {
        id: removeOriginal

        onRemoveConfirmed: page.removeNow()
    }

    TransferPrompt {
        id: transferPrompt

        register: page.register
        onFinished: page.done()
    }

    TransferPartnerDialog {
        id: partnerDialog

        register: page.register
        onPicked: partner => transferPrompt.askPicked([partner])
    }

    Kirigami.FormLayout {
        // What can't be changed here, and where it can.
        Controls.Label {
            textFormat: Text.PlainText
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: !page.editable
            text: page.kind === "starting-balance"
                ? page.i18nContext.i18nc("@info", "This is the starting balance: what the account held when you began. To change it, check the account against your bank, and KassenSturz offers to correct it.")
                : page.kind === "transfer"
                    ? page.i18nContext.i18nc("@info %1 is the name of an account", "This money was moved between this account and %1. Apart from its tags, a Transfer can't be changed yet. You can remove it and add each side again in its own account; KassenSturz then offers to join them.", page.entry.otherAccount ?? "")
                    : page.i18nContext.i18nc("@info", "This transaction is broken down into several parts, which have their own page.")
            wrapMode: Text.Wrap
        }

        Controls.Button {
            visible: page.kind === "starting-balance"
            text: page.i18nContext.i18nc("@action:button", "Check against my bank…")
            icon.name: "view-financial-account-checking"
            onClicked: {
                ActionLog.taken("TransactionPage_checkAgainstBank")
                page.checkAgainstBankClicked(page.chosenAccount)
            }
        }

        Controls.Button {
            visible: page.kind === "parts"
            text: page.i18nContext.i18nc("@action:button", "Change the parts…")
            icon.name: "view-list-details"
            onClicked: {
                ActionLog.taken("TransactionPage_changeParts")
                page.breakDownClicked(page.chosenAccount, page.transactionId)
            }
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
            onToggled: {
                ActionLog.taken("TransactionPage_iPaidMoney")
                page.moneyIn = false
            }
        }

        Controls.RadioButton {
            visible: page.editable
            text: page.i18nContext.i18nc("@option:radio", "I received money")
            checked: page.moneyIn
            Controls.ButtonGroup.group: directionGroup
            onToggled: {
                ActionLog.taken("TransactionPage_iReceivedMoney")
                page.moneyIn = true
            }
        }

        Controls.TextField {
            id: amountField
            visible: page.editable
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "How much?")
            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", page.amountExample)
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            readOnly: page.entry.imported ?? false
            onAccepted: {
                ActionLog.taken("TransactionPage_save")
                page.save()
            }
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
            onAccepted: {
                ActionLog.taken("TransactionPage_save_2")
                page.save()
            }
        }

        // The name as the bank or file delivered it, when the transaction
        // shows another one: the original stays next to the user's own
        // (D-375).
        Controls.Label {
            visible: !page.adding && (page.entry.deliveredName ?? "") !== ""
            Kirigami.FormData.label: page.i18nContext.i18nc("@label the other party's name as the bank or file delivered it", "As delivered:")
            text: page.entry.deliveredName ?? ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            Accessible.name: page.i18nContext.i18nc("@info %1 is a name as the bank delivered it", "As delivered by your bank or file: %1", text)
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
            onActivated: {
                ActionLog.taken("TransactionPage_chooseCategory")
                page.categoryId = page.chosenCategory()
            }
        }

        Controls.TextField {
            id: dateField
            visible: page.editable
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Date:")
            text: page.register.today
            onAccepted: {
                ActionLog.taken("TransactionPage_save_3")
                page.save()
            }
        }

        // A few words about the transaction for the user's own use, e.g.
        // what a payment was for. Optional, so it comes after the basics.
        // It wraps and takes line breaks, so an imported memo shows whole
        // (D-703); Tab moves on as in the other fields, Ctrl+Enter saves.
        // It grows up to a few lines and then scrolls, so the buttons below
        // stay in view, and it stops taking text at the limit a save checks,
        // with a counter below that shows how much is left (B-226).
        Controls.ScrollView {
            id: noteScroll
            visible: page.editable
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            Layout.maximumHeight: Kirigami.Units.gridUnit * 8
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Note:")

            Controls.TextArea {
                id: noteField
                wrapMode: TextEdit.Wrap
                placeholderText: page.i18nContext.i18nc("@info:placeholder", "Optional, for example what it was for")
                Accessible.name: page.i18nContext.i18nc("@label:textbox", "Note:")
                Accessible.description: noteCounter.text
                Keys.onTabPressed: event => nextItemInFocusChain(true).forceActiveFocus(Qt.TabFocusReason)
                Keys.onBacktabPressed: event => nextItemInFocusChain(false).forceActiveFocus(Qt.BacktabFocusReason)

                // The text as it last fitted, to go back to when typing or
                // pasting would make it longer than the limit; text that
                // makes it shorter is always taken, and nothing already in
                // the field is cut.
                property string fitting: ""
                property int counted: 0
                property bool refused: false
                property bool restoring: false

                onTextChanged: {
                    if (restoring) {
                        return;
                    }
                    const typedLength = page.register.noteLength(text);
                    if (typedLength > page.noteLimit && typedLength > counted) {
                        // Keep as much of the new text as fits and refuse
                        // only the rest, so a long paste is cut at the
                        // limit, not dropped (#707, US4 P3 R1).
                        const kept = page.fitNote(fitting, text, page.noteLimit);
                        restoring = true;
                        text = kept.text;
                        cursorPosition = Math.max(0, Math.min(kept.cursor, text.length));
                        restoring = false;
                        fitting = text;
                        counted = page.register.noteLength(text);
                        refused = true;
                        return;
                    }
                    fitting = text;
                    counted = typedLength;
                    refused = false;
                }

                // Ctrl+Enter saves; Enter alone starts a new line.
                Shortcut {
                    sequences: ["Ctrl+Return", "Ctrl+Enter"]
                    enabled: noteField.activeFocus
                    onActivated: {
                        ActionLog.taken("TransactionPage_save_5")
                        page.save()
                    }
                }
            }
        }

        // How long the note is and how long it may be; when typing or
        // pasting was stopped at the limit it says why, in the field's
        // column so it is seen where the text goes (B-226).
        Controls.Label {
            id: noteCounter
            visible: page.editable
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font: Kirigami.Theme.smallFont
            color: noteField.refused
                ? Kirigami.Theme.negativeTextColor
                : noteField.counted >= page.noteLimit
                    ? Kirigami.Theme.neutralTextColor
                    : Kirigami.Theme.disabledTextColor
            text: noteField.refused
                ? page.i18nContext.i18nc("@info %1 is the longest a note may be", "A note may be up to %1 characters long, so the rest did not fit. Please shorten the note to add more.", page.noteLimit)
                : page.i18nContext.i18nc("@info %1 characters typed so far, %2 the longest a note may be", "%1 of %2 characters", noteField.counted, page.noteLimit)
        }

        // The tags of the whole transaction (US4.8, docs/TAGS.md §1). A
        // Transfer, which can't be changed here otherwise, takes tags too
        // (Q-535), saved with their own button.
        TagField {
            id: tagField
            visible: page.editable || page.kind === "transfer"
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Tags:")
        }

        Controls.Button {
            visible: !page.editable && page.kind === "transfer"
            enabled: page.tagsText() !== page.loadedTags
            text: page.i18nContext.i18nc("@action:button", "Save tags")
            icon.name: "document-save"
            onClicked: {
                ActionLog.taken("TransactionPage_saveTransferTags")
                page.saveTransferTags()
            }
        }

        // Tags its parts have of their own, which the Parts page changes; a
        // tag never moves between the whole and a part by itself (D-713).
        Controls.Label {
            visible: page.editable && (page.entry.partTags ?? []).length > 0
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.disabledTextColor
            text: page.i18nContext.i18nc("@info %1 is a list of tags, e.g. Tax, Gift", "Some parts also have the tags: %1", (page.entry.partTags ?? []).map(tag => tag.name).join(", "))
        }

        // The bills and other documents linked to it (D-132, D-739): shown
        // with AI off too, as the links are the user's. Opening goes
        // through the system (in the Flatpak the OpenURI portal), so
        // KassenSturz never runs a file; removing takes off the link only.
        ColumnLayout {
            visible: !page.adding && page.bills.length > 0
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Bills:")

            Repeater {
                model: page.bills

                ColumnLayout {
                    id: billRow

                    required property var modelData

                    Layout.fillWidth: true

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: billRow.modelData.name
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: billRow.modelData.missing
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        color: Kirigami.Theme.negativeTextColor
                        text: page.i18nContext.i18nc("@info %1 is where the file was", "This bill isn't where it was: %1. Put it back there, or remove the link.", billRow.modelData.link)
                    }

                    RowLayout {
                        Controls.Button {
                            visible: billRow.modelData.url !== "" && !billRow.modelData.missing
                            text: page.i18nContext.i18nc("@action:button", "Open bill")
                            icon.name: "document-open"
                            onClicked: {
                                ActionLog.taken("TransactionPage_openBill")
                                Qt.openUrlExternally(billRow.modelData.url)
                            }
                        }

                        Controls.Button {
                            text: page.i18nContext.i18nc("@action:button", "Remove link")
                            icon.name: "edit-delete-remove"
                            onClicked: {
                                ActionLog.taken("TransactionPage_removeBill")
                                page.removeBill(billRow.modelData.link)
                            }
                        }
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font: Kirigami.Theme.smallFont
                text: page.i18nContext.i18nc("@info", "Removing a link leaves the file where it is. KassenSturz never moves, changes or deletes a bill.")
            }
        }

        RowLayout {
            Controls.Button {
                visible: page.editable
                text: Plain.plain(page.adding
                    ? page.i18nContext.i18nc("@action:button", "Add transaction")
                    : page.i18nContext.i18nc("@action:button", "Save"))
                icon.name: page.adding ? "list-add" : "document-save"
                onClicked: {
                    ActionLog.taken("TransactionPage_save_4")
                    page.save()
                }
            }

            Controls.Button {
                visible: !page.adding && page.kind !== "starting-balance"
                text: page.i18nContext.i18nc("@action:button", "Remove")
                icon.name: "edit-delete"
                onClicked: {
                    ActionLog.taken("TransactionPage_remove")
                    page.remove()
                }
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
            onClicked: {
                ActionLog.taken("TransactionPage_breakDownTransaction")
                if (page.keepTyped()) {
                    page.breakDownClicked(page.chosenAccount, page.transactionId)
                }
            }
        }

        // Money moved to or from another of the user's accounts, also when
        // it is sorted already (D-692): the user picks the other side.
        Controls.Button {
            visible: !page.adding && page.kind === "simple"
            text: page.i18nContext.i18nc("@action:button", "Join with the other side of a move…")
            icon.name: "edit-link"
            flat: true
            onClicked: {
                ActionLog.taken("TransactionPage_joinOtherSide")
                if (page.keepTyped()) {
                    partnerDialog.show(page.transactionId)
                }
            }
        }

        // Likewise a Rule: sorting the next transactions like this one
        // automatically (M3.11).
        Controls.Button {
            visible: !page.adding && page.kind === "simple"
            text: page.i18nContext.i18nc("@action:button", "Create a Rule from this transaction…")
            icon.name: "view-filter"
            flat: true
            onClicked: {
                ActionLog.taken("TransactionPage_createRuleTransaction")
                if (page.keepTyped()) {
                    page.createRuleClicked(page.chosenAccount, page.transactionId)
                }
            }
        }

        // A payment that comes again and again: a planned payment filled
        // from this entry, which becomes its first payment (US7.1, D-769).
        Controls.Button {
            visible: page.plans !== null && !page.adding && page.kind === "simple"
            text: page.i18nContext.i18nc("@action:button", "Plan this again…")
            icon.name: "view-calendar-upcoming-events"
            flat: true
            onClicked: {
                ActionLog.taken("TransactionPage_planAgain")
                if (!page.keepTyped()) {
                    return;
                }
                const form = page.plans.formFromEntry(page.transactionId, page.chosenAccount, page.i18nContext.i18nc("@info the name of a new planned payment when the entry names nobody", "Planned payment"));
                if (form.code !== undefined) {
                    error.result = form;
                } else if (form.name === undefined) {
                    applicationWindow().showPassiveNotification(page.i18nContext.i18nc("@info", "This entry can't be planned again: a planned payment is money going out of or into one of your accounts, not a move between two of them."), "long");
                } else {
                    planAgain.fromEntryForm(form);
                }
            }
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: !page.adding && page.kind !== "starting-balance"
            text: page.i18nContext.i18nc("@info", "Removing takes the transaction out of this account's list and balance. Undo brings it back.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }

    // "Plan this again…" (US7.1). Without planned payments the button is
    // hidden, so the form is never opened.
    PlanFormDialog {
        id: planAgain

        plans: page.plans
        register: page.register
        onSaved: applicationWindow().showPassiveNotification(page.i18nContext.i18nc("@info", "Planned. KassenSturz now expects this payment again; see Accounts › Planned payments…"), "long")
    }
}
