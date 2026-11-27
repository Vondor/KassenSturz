// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Breaks a transaction down into Parts (M3.6, D-017, D-119, D-1015): what one
// purchase was for, piece by piece. One Part's amount can be left empty, and
// KassenSturz works it out from the rest, so nobody has to do the
// subtraction; saving is refused unless the Parts add up to the total. A
// Part booked to a category in another currency also gets its amount in
// that currency (D-043). Money moved to another of the user's accounts in
// the same transaction is shown and kept as it is. Whole-transaction details
// (bank-delivered name and purpose, note, tags, linked bills, and "Plan this
// again…") stay visible and editable here alongside each Part's own note and
// tags (D-1015). Every amount is text from the Rust side (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountRegister register
    // The planned payments, for "Plan this again…" (US7.1, D-1015); none hides it.
    property var plans: null
    required property string accountId
    required property string transactionId

    // The "Plan this again…" form, for the smoke test (a dialog is no child
    // item anyone can find).
    readonly property alias planAgainForm: planAgain

    // What the Rust side says about the transaction.
    property var entry: ({})
    property bool moneyIn: false
    // The categories to choose from, read again when the direction changes.
    property var categoryChoices: []
    // What the Rust side worked out for the form as it stands.
    property var preview: ({ parts: [] })
    property string amountExample: ""
    // The Parts as the form holds them.
    readonly property ListModel partsModel: ListModel {}
    // The book's tags, for the whole transaction and each Part (US4.8, D-1015).
    property var tagChoices: []
    // The note as loaded, so a save sends it only when changed (D-703, D-1015):
    // a long imported memo stays untouched if not edited.
    property string loadedNote: ""
    readonly property int noteLimit: register.noteLimit(entry.note ?? "")
    // The whole-transaction tags as loaded, so a save sends them only when changed.
    property string loadedTags: "[]"
    // Snapshot of the loaded/saved form state, so "Plan this again…" saves
    // unsaved edits first (M6-B227, D-1015).
    property string loadedFields: ""
    // The bills linked to the transaction (D-132, D-739, D-1015), read again
    // after a link is removed without discarding typed edits.
    property var bills: entry.bills ?? []
    // How wide a form field may be: 24 units, or the room the page has when
    // narrow (B-318).
    readonly property real fieldWidth: Math.min(Kirigami.Units.gridUnit * 24, page.availableWidth - Kirigami.Units.gridUnit * 2)

    // Saved or removed.
    signal done()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Break down this transaction")

    header: FormError {
        id: error
    }

    Component.onCompleted: {
        entry = register.partsDetails(accountId, transactionId);
        if (entry.code !== undefined) {
            error.result = entry;
            return;
        }
        moneyIn = entry.moneyIn;
        totalField.text = entry.total;
        otherPartyField.text = entry.otherParty;
        dateField.text = entry.date;
        noteField.text = entry.note ?? "";
        loadedNote = noteField.text;
        tagField.tags = entry.tags ?? [];
        loadedTags = tagsText();
        bills = entry.bills ?? [];
        amountExample = register.amountExample(accountId);
        for (const part of entry.parts) {
            // A list in a ListModel role turns into a model of its own, so
            // the tags are kept as JSON text.
            const partNote = part.note ?? "";
            page.partsModel.append({
                splitId: part.id,
                category: part.category,
                amount: part.amount,
                foreignAmount: part.foreignAmount,
                note: partNote,
                loadedNote: partNote,
                tags: JSON.stringify(part.tags ?? []),
            });
        }
        tagChoices = register.tagChoices();
        reloadChoices();
        refresh();
        loadedFields = stateText();
    }

    onMoneyInChanged: {
        if (entry.id !== undefined) {
            reloadChoices();
            refresh();
        }
    }

    function reloadChoices() {
        const found = register.partCategoryChoices(accountId, transactionId, moneyIn);
        categoryChoices = found.code === undefined ? found : [];
    }

    // `typed` is `before` with text put in (and maybe some taken out) at one
    // place. Returns `{text, cursor}`: `before` with as much of the new text
    // as keeps the note within `limit`, and the cursor after it. The length
    // is counted by Rust, as a save counts it (D-703, B-226).
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
        if (low > 0 && low < added.length) {
            const unit = added.charCodeAt(low - 1);
            if (unit >= 0xD800 && unit <= 0xDBFF) {
                low--;
            }
        }
        const kept = head + added.slice(0, low) + tail;
        if (register.noteLength(kept) > limit) {
            return { text: before, cursor: start };
        }
        return { text: kept, cursor: start + low };
    }

    function tagsText(): string {
        return JSON.stringify(tagField.tags.map(tag => [tag.id, tag.name]));
    }

    function stateText(): string {
        const parts = [];
        for (let i = 0; i < page.partsModel.count; i++) {
            const part = page.partsModel.get(i);
            parts.push([part.splitId, part.category, part.amount, part.foreignAmount, part.note, part.tags]);
        }
        return JSON.stringify([moneyIn, totalField.text, otherPartyField.text, dateField.text, noteField.text, tagsText(), parts]);
    }

    function unsaved(): bool {
        return entry.id !== undefined && stateText() !== loadedFields;
    }

    // Saves unsaved edits before "Plan this again…" reads the transaction.
    function keepTyped(): bool {
        if (!unsaved()) {
            return true;
        }
        const result = register.saveParts(accountId, transactionId, form());
        error.result = result;
        if (result.code !== undefined) {
            return false;
        }
        loadedNote = noteField.text;
        loadedTags = tagsText();
        for (let i = 0; i < page.partsModel.count; i++) {
            page.partsModel.setProperty(i, "loadedNote", page.partsModel.get(i).note);
        }
        loadedFields = stateText();
        return true;
    }

    // What the user typed, as the Rust side takes it. Omitting `note` or
    // `tags` leaves stored values untouched.
    function form(): var {
        const parts = [];
        for (let i = 0; i < page.partsModel.count; i++) {
            const part = page.partsModel.get(i);
            const item = {
                id: part.splitId,
                category: part.category,
                amount: part.amount,
                foreignAmount: part.foreignAmount,
                tags: JSON.parse(part.tags),
            };
            if (part.note !== part.loadedNote || (part.splitId === "" && part.note !== "")) {
                item.note = part.note;
            }
            parts.push(item);
        }
        const typed = {
            moneyIn: moneyIn,
            total: totalField.text,
            otherParty: otherPartyField.text,
            date: dateField.text,
            parts: parts,
        };
        if (noteField.text !== loadedNote) {
            typed.note = noteField.text;
        }
        if (tagsText() !== loadedTags) {
            typed.tags = tagField.tags;
        }
        return typed;
    }

    // Asks the Rust side what the form adds up to. A form it can't read yet
    // shows nothing worked out; saving says what is wrong.
    function refresh() {
        const result = register.partsPreview(accountId, transactionId, form());
        preview = result.code === undefined ? result : ({ parts: [] });
    }

    function previewOf(index: int): var {
        return (preview.parts ?? [])[index] ?? ({});
    }

    function removePart(index: int) {
        page.partsModel.remove(index);
        refresh();
    }

    function addPart() {
        page.partsModel.append({ splitId: "", category: "", amount: "", foreignAmount: "", note: "", loadedNote: "", tags: "[]" });
        refresh();
    }

    function save() {
        const result = register.saveParts(accountId, transactionId, form());
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    function removeBill(link: string) {
        const result = register.removeBill(accountId, transactionId, link);
        error.result = result;
        if (result.code === undefined) {
            bills = register.partsDetails(accountId, transactionId).bills ?? [];
        }
    }

    // A transaction with what a bank or file delivered is removed only
    // after the plain warning (D-375); one typed in is removed at once.
    function remove() {
        const notice = register.removalNotice(accountId, transactionId);
        if (notice.code !== undefined) {
            error.result = notice;
        } else if (notice.holdsOriginal) {
            removeOriginal.ask(notice);
        } else {
            removeNow();
        }
    }

    function removeNow() {
        const result = register.removeTransaction(accountId, transactionId);
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    RemoveOriginalDialog {
        id: removeOriginal

        onRemoveConfirmed: page.removeNow()
    }

    Kirigami.FormLayout {
        id: layout

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            text: page.i18nContext.i18nc("@info", "Say what each part of this transaction was for. Leave one part's amount empty, and KassenSturz works it out from the rest.")
            wrapMode: Text.Wrap
        }

        Controls.ButtonGroup {
            id: directionGroup
        }

        Controls.RadioButton {
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "What happened?")
            text: page.i18nContext.i18nc("@option:radio", "I paid money")
            checked: !page.moneyIn
            Controls.ButtonGroup.group: directionGroup
            onToggled: {
                ActionLog.taken("PartsPage_iPaidMoney")
                page.moneyIn = false
            }
        }

        Controls.RadioButton {
            text: page.i18nContext.i18nc("@option:radio", "I received money")
            checked: page.moneyIn
            Controls.ButtonGroup.group: directionGroup
            onToggled: {
                ActionLog.taken("PartsPage_iReceivedMoney")
                page.moneyIn = true
            }
        }

        Controls.TextField {
            id: totalField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "How much in all?")
            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", page.amountExample)
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            readOnly: page.entry.totalFixed ?? false
            onTextEdited: page.refresh()
            onAccepted: {
                ActionLog.taken("PartsPage_save")
                page.save()
            }
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            visible: page.entry.totalFixed ?? false
            text: page.i18nContext.i18nc("@info", "This transaction came from your bank, so the total stays as the bank sent it. You can change the parts.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Controls.TextField {
            id: otherPartyField
            Kirigami.FormData.label: page.moneyIn
                ? page.i18nContext.i18nc("@label:textbox who the money came from", "From:")
                : page.i18nContext.i18nc("@label:textbox who the money went to", "To:")
            onAccepted: {
                ActionLog.taken("PartsPage_save_2")
                page.save()
            }
        }

        // The name as the bank or file delivered it, when the transaction
        // shows another one (D-375, D-1015).
        Controls.Label {
            visible: (page.entry.deliveredName ?? "") !== ""
            Kirigami.FormData.label: page.i18nContext.i18nc("@label the other party's name as the bank or file delivered it", "As delivered:")
            text: page.entry.deliveredName ?? ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            Layout.maximumWidth: page.fieldWidth
            Accessible.name: page.i18nContext.i18nc("@info %1 is a name as the bank delivered it", "As delivered by your bank or file: %1", text)
        }

        Controls.TextField {
            id: dateField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Date:")
            onTextEdited: page.refresh()
            onAccepted: {
                ActionLog.taken("PartsPage_save_3")
                page.save()
            }
        }

        // Read-only bank payment purpose (`remittance_information`) when
        // delivered by the bank or file (D-1015).
        Controls.Label {
            visible: (page.entry.purpose ?? "") !== ""
            Kirigami.FormData.label: page.i18nContext.i18nc("@label the payment purpose as the bank or file delivered it", "Purpose:")
            text: page.entry.purpose ?? ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            Layout.maximumWidth: page.fieldWidth
            Accessible.name: page.i18nContext.i18nc("@info %1 is a payment purpose as the bank delivered it", "Purpose as delivered by your bank or file: %1", text)
        }

        // Whole-transaction note with character counter and untouched-memo
        // preservation, matching TransactionPage.qml (D-703, B-226, B-318, D-1015).
        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Layout.fillWidth: true
            Layout.maximumWidth: page.fieldWidth
            Layout.minimumWidth: 0
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Note:")

            Controls.ScrollView {
                id: noteScroll
                Layout.fillWidth: true
                Layout.maximumHeight: Kirigami.Units.gridUnit * 8

                Controls.TextArea {
                    id: noteField
                    wrapMode: TextEdit.Wrap
                    placeholderText: page.i18nContext.i18nc("@info:placeholder", "Optional, for example what it was for")
                    Accessible.name: page.i18nContext.i18nc("@label:textbox", "Note:")
                    Accessible.description: noteCounter.text
                    Keys.onTabPressed: event => nextItemInFocusChain(true).forceActiveFocus(Qt.TabFocusReason)
                    Keys.onBacktabPressed: event => nextItemInFocusChain(false).forceActiveFocus(Qt.BacktabFocusReason)

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

                    Shortcut {
                        sequences: ["Ctrl+Return", "Ctrl+Enter"]
                        enabled: noteField.activeFocus
                        onActivated: {
                            ActionLog.taken("PartsPage_save_6")
                            page.save()
                        }
                    }
                }
            }

            Controls.Label {
                id: noteCounter
                Layout.fillWidth: true
                Layout.preferredWidth: 0
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
        }

        // Tags on the whole transaction, distinct from per-part tags below (D-713, D-1015).
        TagField {
            id: tagField
            Layout.fillWidth: true
            Layout.maximumWidth: page.fieldWidth
            choices: page.tagChoices
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox tags on the whole transaction, as opposed to one part", "Tags for the whole transaction:")
            accessibleName: page.i18nContext.i18nc("@label:textbox tags on the whole transaction, as opposed to one part", "Tags for the whole transaction:")
        }

        // Bills and other documents linked to the transaction (D-132, D-739,
        // D-1015): shown with AI off too, as the links are the user's.
        ColumnLayout {
            visible: page.bills.length > 0
            Layout.fillWidth: true
            Layout.maximumWidth: page.fieldWidth
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
                        visible: !billRow.modelData.missing
                            && billRow.modelData.shownLink !== ""
                            && billRow.modelData.shownLink !== billRow.modelData.name
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        font: Kirigami.Theme.smallFont
                        opacity: 0.7
                        text: billRow.modelData.shownLink
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: billRow.modelData.missing
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        color: Kirigami.Theme.negativeTextColor
                        text: page.i18nContext.i18nc("@info %1 is where the file was", "This bill isn't where it was: %1. Put it back there, or remove the link.", billRow.modelData.shownLink)
                    }

                    RowLayout {
                        Controls.Button {
                            visible: billRow.modelData.url !== "" && !billRow.modelData.missing
                            text: page.i18nContext.i18nc("@action:button", "Open bill")
                            icon.name: "document-open"
                            onClicked: {
                                ActionLog.taken("PartsPage_openBill")
                                Qt.openUrlExternally(billRow.modelData.url)
                            }
                        }

                        Controls.Button {
                            visible: billRow.modelData.folderUrl !== "" && !billRow.modelData.missing
                            text: page.i18nContext.i18nc("@action:button", "Show in folder")
                            icon.name: "folder-open"
                            onClicked: {
                                ActionLog.taken("PartsPage_showBillFolder")
                                Qt.openUrlExternally(billRow.modelData.folderUrl)
                            }
                        }

                        Controls.Button {
                            text: page.i18nContext.i18nc("@action:button", "Remove link")
                            icon.name: "edit-delete-remove"
                            onClicked: {
                                ActionLog.taken("PartsPage_removeBill")
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
                text: page.i18nContext.i18nc("@info", "Removing a link leaves the file where it is. KassenSturz never changes or deletes a bill; it moves one only when you confirm filing it from your inbox.")
            }
        }

        Kirigami.Separator {
            Kirigami.FormData.isSection: true
            Kirigami.FormData.label: page.i18nContext.i18nc("@title:group", "The parts")
        }

        // The rows sit in a column of their own, so a Part added later
        // joins the others and not the end of the form.
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            // Money moved to or from another of the user's accounts in the
            // same transaction stays as it is (M3.7).
            Repeater {
                model: page.entry.fixed ?? []

                Controls.Label {
                    textFormat: Text.PlainText
                    required property var modelData

                    text: modelData.incoming
                        ? page.i18nContext.i18nc("@label %1 is the name of an account, %2 an amount of money", "Moved from %1: %2", modelData.name, modelData.amount)
                        : page.i18nContext.i18nc("@label %1 is the name of an account, %2 an amount of money", "Moved to %1: %2", modelData.name, modelData.amount)
                }
            }

            Repeater {
                model: page.partsModel

                PartRow {
                    required property int index
                    required property var model

                    Layout.fillWidth: true
                    number: index + 1
                    choices: page.categoryChoices
                    category: model.category
                    amount: model.amount
                    foreignAmount: model.foreignAmount
                    note: model.note
                    loadedNote: model.loadedNote
                    noteLimit: page.register.noteLimit(model.loadedNote)
                    tags: JSON.parse(model.tags)
                    tagChoices: page.tagChoices
                    preview: page.previewOf(index)
                    amountExample: page.amountExample
                    moneyIn: page.moneyIn
                    onCategoryChosen: categoryId => {
                        model.category = categoryId;
                        page.refresh();
                    }
                    onNewCategoryMade: categoryId => {
                        page.reloadChoices();
                        model.category = categoryId;
                        page.refresh();
                    }
                    onAmountEdited: text => {
                        model.amount = text;
                        page.refresh();
                    }
                    onForeignAmountEdited: text => {
                        model.foreignAmount = text;
                        page.refresh();
                    }
                    onNoteEdited: text => {
                        model.note = text;
                    }
                    onTagsEdited: tags => {
                        model.tags = JSON.stringify(tags);
                    }
                    // The page removes the row: removing it here destroys
                    // this delegate, and the rest of the handler would then
                    // run without `page` (M6-B235).
                    onRemoveClicked: page.removePart(index)
                    onAccepted: {
                        ActionLog.taken("PartsPage_save_4")
                        page.save()
                    }
                }
            }
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button", "Add a part")
            icon.name: "list-add"
            onClicked: {
                ActionLog.taken("PartsPage_addPart")
                page.addPart()
            }
        }

        // Whether it all adds up, from the exact amounts (D-065).
        Controls.Label {
            textFormat: Text.PlainText
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            visible: (page.preview.sum ?? "") !== ""
            // Worked out only while there is a sum: before the first
            // preview, or when the form can't be read, there is none, and
            // KI18n warns about a message given no amount (M6-B235).
            text: (page.preview.sum ?? "") === ""
                ? ""
                : (page.preview.missing ?? "") !== ""
                    ? page.i18nContext.i18nc("@info %1 and %2 are amounts of money", "The parts add up to %1. %2 is still to be sorted into a part.", page.preview.sum, page.preview.missing)
                    : (page.preview.over ?? "") !== ""
                        ? page.i18nContext.i18nc("@info %1 and %2 are amounts of money", "The parts add up to %1, which is %2 more than the total.", page.preview.sum, page.preview.over)
                        : page.i18nContext.i18nc("@info %1 is an amount of money", "The parts add up to %1, the whole transaction.", page.preview.sum)
            wrapMode: Text.Wrap
        }

        RowLayout {
            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Save")
                icon.name: "document-save"
                enabled: page.entry.id !== undefined
                onClicked: {
                    ActionLog.taken("PartsPage_save_5")
                    page.save()
                }
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Remove")
                icon.name: "edit-delete"
                enabled: page.entry.id !== undefined
                onClicked: {
                    ActionLog.taken("PartsPage_remove")
                    page.remove()
                }
            }
        }

        // A recurring split payment can still be planned again (US7.1, D-1015).
        Controls.Button {
            visible: page.plans !== null && page.entry.id !== undefined && (page.entry.fixed ?? []).length === 0
            text: page.i18nContext.i18nc("@action:button", "Plan this again…")
            icon.name: "view-calendar-upcoming-events"
            flat: true
            onClicked: {
                ActionLog.taken("PartsPage_planAgain")
                if (!page.keepTyped()) {
                    return;
                }
                const form = page.plans.formFromEntry(page.transactionId, page.accountId, page.i18nContext.i18nc("@info the name of a new planned payment when the entry names nobody", "Planned payment"));
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
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            text: page.i18nContext.i18nc("@info", "Removing takes the transaction out of this account's list and balance. Undo brings it back.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }

    PlanFormDialog {
        id: planAgain

        plans: page.plans
        register: page.register
        onSaved: applicationWindow().showPassiveNotification(page.i18nContext.i18nc("@info", "Planned. KassenSturz now expects this payment again; see Accounts › Planned payments…"), "long")
    }
}
