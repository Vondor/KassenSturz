// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What the user can do with one change of a set before confirming it (M6b.8,
// D-293, D-297, US11.2, D-1016): leave it out, change a few of its fields, or
// take the AI's version back. The fields are those of the AI's own call: a new
// category's name and parent, a new Rule's name, category and conditions, the
// category a transaction gets, an edited transaction's unlocked payee and
// note, a split transaction's part categories and (when Note is unlocked) part
// names, and a new chart's title, kind and period. The edit is checked in Rust
// like the AI's call (`AiChat.editChange`) and is never sent to the AI. Texts
// from the AI or the book are shown as plain text (T-08). Part of the review
// page, so hidden with the rest of AI (D-013).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: correction

    required property AiSwitch ai
    // The change as `AiChat.changes` gives it.
    required property var change
    // The categories it may be given, as `AiChat.changeChoices` gives them.
    required property var choices
    // The review page: it runs the corrections and names periods and kinds
    // of chart.
    required property var review

    property bool editing: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property string kind: change.kind
    // A sort's one field is the switch on its own card (D-682).
    readonly property bool editable: kind !== "sortWithRules" && change.fields !== undefined && change.fields.length > 0
    readonly property bool canEditPayee: kind === "editTransaction" && hasField("payee")
    readonly property bool canEditNote: kind === "editTransaction" && hasField("note")
    readonly property var form: change.form !== undefined && change.form !== "" ? JSON.parse(change.form) : ({})
    // The fixed periods, and the change's own when it is another one: a
    // period that moves with today or a range of days (D-650), so opening
    // the form never changes it.
    readonly property var periods: {
        const fixed = ["this-month", "last-month", "last-12-months", "this-year", "last-year"];
        const own = form.period ?? "";
        return own !== "" && !fixed.includes(own) ? fixed.concat([own]) : fixed;
    }
    // A new category goes inside one of the book's categories of its own
    // kind and currency, or at the top.
    readonly property var parents: choices.filter(choice => choice.id !== "" && choice.income === change.income && choice.currency === change.currency)
    // A transaction (or its split parts) gets a category in its own currency; a Rule any.
    readonly property var categories: (kind === "categorize" || kind === "splitTransaction")
        ? choices.filter(choice => choice.currency === change.currency)
        : choices
    readonly property var subjects: [
        { key: "payee", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at: the name shown as From or To", "Name shown") },
        { key: "other_party_name", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at: the name the bank gave for who paid or was paid", "Other party") },
        { key: "bank_text", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at: the text a payment came with", "Purpose") },
        { key: "money_in", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at", "Money received") },
        { key: "money_out", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at", "Money paid") }
    ]
    readonly property var tests: [
        { key: "contains", label: i18nContext.i18nc("@item:inlistbox a text has to", "contains") },
        { key: "equals", label: i18nContext.i18nc("@item:inlistbox a text has to", "is exactly") },
        { key: "starts_with", label: i18nContext.i18nc("@item:inlistbox a text has to", "starts with") },
        { key: "ends_with", label: i18nContext.i18nc("@item:inlistbox a text has to", "ends with") },
        { key: "matches_pattern", label: i18nContext.i18nc("@item:inlistbox a text has to", "matches the pattern") }
    ]

    function hasField(name: string): bool {
        return change.fields !== undefined && Array.from(change.fields).indexOf(name) >= 0;
    }

    function choiceIndex(list: var, item: var): int {
        return list.findIndex(choice => (item.category_id !== undefined && item.category_id !== "" && choice.id === item.category_id) || (item.new_category !== undefined && item.new_category !== "" && choice.key === item.new_category));
    }

    // Index into the split-part category ComboBox: 0 is "Not sorted yet",
    // 1..categories.length are the compatible open and proposed categories.
    function splitChoiceIndex(item: var): int {
        if ((item.category_id === undefined || item.category_id === "") && (item.new_category === undefined || item.new_category === "")) {
            return 0;
        }
        const at = choiceIndex(categories, item);
        return at >= 0 ? at + 1 : 0;
    }

    function chosen(choice: var): var {
        return choice.id !== "" ? { category_id: choice.id } : { new_category: choice.key };
    }

    // Fills the form from the change as it stands.
    function start() {
        nameField.text = form.name ?? form.title ?? "";
        payeeField.text = form.payee !== undefined ? form.payee : (change.payee ?? "");
        noteField.text = form.note !== undefined ? form.note : (change.note ?? "");
        parentBox.currentIndex = form.parent_id === undefined ? 0 : parents.findIndex(choice => choice.id === form.parent_id) + 1;
        categoryBox.currentIndex = choiceIndex(categories, form);
        matchBox.currentIndex = form.match === "any" ? 1 : 0;
        conditionModel.clear();
        for (const condition of form.conditions ?? []) {
            conditionModel.append({
                subject: condition.money_in === true ? "money_in" : (condition.money_out === true ? "money_out" : condition.field),
                test: condition.test ?? "contains",
                text: condition.text ?? "",
                caseSensitive: condition.case_sensitive === true
            });
        }
        splitPartModel.clear();
        const rawParts = form.parts ?? [];
        const viewParts = change.parts ?? [];
        for (let i = 0; i < rawParts.length; ++i) {
            const part = rawParts[i];
            const shownPart = i < viewParts.length ? viewParts[i] : null;
            // Carry each part's exact decimal amount string untouched (D-065).
            splitPartModel.append({
                amountText: String(part.amount ?? ""),
                displayAmount: shownPart !== null ? String(shownPart.amount ?? "") : String(part.amount ?? ""),
                categoryId: part.category_id ?? "",
                newCategory: part.new_category ?? "",
                name: part.name ?? ""
            });
        }
        const shown = form.chart === "auto" && change.component !== undefined ? JSON.parse(change.component).type : form.chart;
        chartBox.currentIndex = Math.max(0, (change.charts ?? []).indexOf(shown));
        periodBox.currentIndex = Math.max(0, periods.indexOf(form.period ?? "last-12-months"));
        editing = true;
        if (kind === "editTransaction") {
            if (canEditPayee) {
                payeeField.forceActiveFocus();
            } else if (canEditNote) {
                noteField.forceActiveFocus();
            }
        } else if (kind === "newCategory" || kind === "newRule" || kind === "newView") {
            nameField.forceActiveFocus();
        }
    }

    // The fields that changed, in the words of the AI's call.
    function fields(): var {
        if (kind === "newCategory") {
            return { name: nameField.text, parent_id: parentBox.currentIndex > 0 ? parents[parentBox.currentIndex - 1].id : null };
        }
        if (kind === "categorize") {
            return categoryBox.currentIndex < 0 ? {} : chosen(categories[categoryBox.currentIndex]);
        }
        if (kind === "editTransaction") {
            const out = {};
            if (canEditPayee) {
                const storedPayee = change.payee ?? "";
                if (payeeField.text !== storedPayee) {
                    out.payee = payeeField.text;
                } else if (form.payee !== undefined) {
                    out.payee = null;
                }
            }
            if (canEditNote) {
                const storedNote = change.note ?? "";
                if (noteField.text !== storedNote) {
                    out.note = noteField.text;
                } else if (form.note !== undefined) {
                    out.note = null;
                }
            }
            return out;
        }
        if (kind === "splitTransaction") {
            const parts = [];
            for (let row = 0; row < splitPartModel.count; ++row) {
                const part = splitPartModel.get(row);
                // Keep exact decimal text from Rust; never convert to float (D-065).
                const written = { amount: part.amountText };
                if (part.categoryId !== "") {
                    written.category_id = part.categoryId;
                } else if (part.newCategory !== "") {
                    written.new_category = part.newCategory;
                }
                if (change.noteUnlocked === true && part.name !== "") {
                    written.name = part.name;
                }
                parts.push(written);
            }
            return { parts: parts };
        }
        if (kind === "newView") {
            return { title: nameField.text, chart: change.charts[chartBox.currentIndex], period: periods[periodBox.currentIndex] };
        }
        const conditions = [];
        for (let row = 0; row < conditionModel.count; ++row) {
            const condition = conditionModel.get(row);
            if (condition.subject === "money_in" || condition.subject === "money_out") {
                conditions.push({ [condition.subject]: true });
            } else {
                const written = { field: condition.subject, test: condition.test, text: condition.text };
                if (condition.caseSensitive) {
                    written.case_sensitive = true;
                }
                conditions.push(written);
            }
        }
        const rule = { name: nameField.text, match: matchBox.currentIndex === 1 ? "any" : "all", conditions: conditions };
        return categoryBox.currentIndex < 0 ? rule : Object.assign(rule, chosen(categories[categoryBox.currentIndex]));
    }

    function save() {
        const editedFields = fields();
        if (kind === "editTransaction" && Object.keys(editedFields).length === 0) {
            editing = false;
            return;
        }
        if (review.editChange(change.slot, editedFields)) {
            editing = false;
        }
    }

    visible: ai.enabled
    spacing: Kirigami.Units.smallSpacing

    Kirigami.Separator {
        Layout.fillWidth: true
    }

    RowLayout {
        Layout.fillWidth: true

        Controls.Label {
            Layout.fillWidth: true
            visible: correction.change.edited
            font.italic: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: correction.i18nContext.i18nc("@info the user edited a change the AI prepared", "Changed by you")
        }

        Item {
            Layout.fillWidth: true
            visible: !correction.change.edited
        }

        Controls.Button {
            visible: correction.change.edited
            icon.name: "edit-undo"
            text: correction.i18nContext.i18nc("@action:button undo the user's edit of a prepared change", "Back to the AI's version")
            onClicked: {
                ActionLog.taken("AiChangeCorrection_backAiS")
                correction.editing = false
                correction.review.backToAi(correction.change.slot)
            }
        }

        Controls.Button {
            visible: correction.editable && !correction.editing
            icon.name: "document-edit"
            text: correction.i18nContext.i18nc("@action:button edit a prepared change", "Change")
            onClicked: {
                ActionLog.taken("AiChangeCorrection_change")
                correction.start()
            }
        }

        Controls.Button {
            icon.name: "list-remove"
            text: correction.i18nContext.i18nc("@action:button leave a prepared change out of the set", "Leave out")
            onClicked: {
                ActionLog.taken("AiChangeCorrection_leaveOut")
                correction.review.leaveOut(correction.change.slot)
            }
        }
    }

    Kirigami.FormLayout {
        Layout.fillWidth: true
        visible: correction.editing && correction.kind !== "splitTransaction"

        Controls.TextField {
            id: nameField

            Kirigami.FormData.label: correction.kind === "newView"
                ? correction.i18nContext.i18nc("@label:textbox", "Title:")
                : correction.i18nContext.i18nc("@label:textbox", "Name:")
            visible: correction.kind === "newCategory" || correction.kind === "newRule" || correction.kind === "newView"
            Accessible.name: correction.kind === "newView"
                ? correction.i18nContext.i18nc("@label:textbox", "The chart's title")
                : correction.i18nContext.i18nc("@label:textbox", "The name")
            onAccepted: {
                ActionLog.taken("AiChangeCorrection_newview")
                correction.save()
            }
        }

        Controls.TextField {
            id: payeeField

            Layout.fillWidth: true
            Kirigami.FormData.label: correction.i18nContext.i18nc("@label:textbox", "Who paid or was paid:")
            visible: correction.canEditPayee
            placeholderText: correction.i18nContext.i18nc("@info:placeholder", "Leave empty to remove")
            Accessible.name: correction.i18nContext.i18nc("@label:textbox", "Who paid or was paid")
            onAccepted: {
                ActionLog.taken("AiChangeCorrection_editPayee")
                correction.save()
            }
        }

        Controls.TextField {
            id: noteField

            Layout.fillWidth: true
            Kirigami.FormData.label: correction.i18nContext.i18nc("@label:textbox", "Note:")
            visible: correction.canEditNote
            placeholderText: correction.i18nContext.i18nc("@info:placeholder", "One line; leave empty to remove")
            Accessible.name: correction.i18nContext.i18nc("@label:textbox", "The note")
            onAccepted: {
                ActionLog.taken("AiChangeCorrection_editNote")
                correction.save()
            }
        }

        Controls.ComboBox {
            id: parentBox

            Kirigami.FormData.label: correction.i18nContext.i18nc("@label:listbox the category a new one goes inside", "Inside:")
            visible: correction.kind === "newCategory"
            model: [correction.i18nContext.i18nc("@item:inlistbox a category at the top, inside no other", "At the top")].concat(correction.parents.map(choice => choice.path))
            Accessible.name: correction.i18nContext.i18nc("@label:listbox", "The category it goes inside")
        }

        Controls.ComboBox {
            id: categoryBox

            Kirigami.FormData.label: correction.kind === "categorize"
                ? correction.i18nContext.i18nc("@label:listbox", "Category:")
                : correction.i18nContext.i18nc("@label:listbox the category a Rule books to", "Books to:")
            visible: correction.kind === "categorize" || correction.kind === "newRule"
            model: correction.categories.map(choice => choice.path)
            Accessible.name: correction.i18nContext.i18nc("@label:listbox", "The category")
        }

        Controls.ComboBox {
            id: matchBox

            Kirigami.FormData.label: correction.i18nContext.i18nc("@label:listbox", "When:")
            visible: correction.kind === "newRule"
            model: [correction.i18nContext.i18nc("@item:inlistbox when a Rule matches", "All of these hold"), correction.i18nContext.i18nc("@item:inlistbox when a Rule matches", "One of these holds")]
            Accessible.name: correction.i18nContext.i18nc("@label:listbox", "Whether all conditions or one must hold")
        }

        Controls.ComboBox {
            id: chartBox

            Kirigami.FormData.label: correction.i18nContext.i18nc("@label:listbox the way a chart shows its figures", "Shown as:")
            visible: correction.kind === "newView"
            model: (correction.change.charts ?? []).map(id => correction.review.typeName(id))
            Accessible.name: correction.i18nContext.i18nc("@label:listbox", "The kind of chart")
        }

        Controls.ComboBox {
            id: periodBox

            Kirigami.FormData.label: correction.i18nContext.i18nc("@label:listbox", "Period:")
            visible: correction.kind === "newView"
            model: correction.periods.map(id => correction.review.periodName(id))
            Accessible.name: correction.i18nContext.i18nc("@label:listbox", "The period the chart shows")
        }
    }

    ListModel {
        id: conditionModel
    }

    Repeater {
        model: correction.editing && correction.kind === "newRule" ? conditionModel : null

        RowLayout {
            id: conditionRow

            required property int index
            required property string subject
            required property string test
            required property string text

            readonly property bool isText: subject !== "money_in" && subject !== "money_out"

            Layout.fillWidth: true

            Controls.ComboBox {
                model: correction.subjects.map(item => item.label)
                currentIndex: correction.subjects.findIndex(item => item.key === conditionRow.subject)
                Accessible.name: correction.i18nContext.i18nc("@label:listbox", "What the condition looks at")
                onActivated: index => {
                    ActionLog.taken("AiChangeCorrection_whatConditionLooks")
                    conditionModel.setProperty(conditionRow.index, "subject", correction.subjects[index].key)
                }
            }

            Controls.ComboBox {
                visible: conditionRow.isText
                model: correction.tests.map(item => item.label)
                currentIndex: Math.max(0, correction.tests.findIndex(item => item.key === conditionRow.test))
                Accessible.name: correction.i18nContext.i18nc("@label:listbox", "How the text is compared")
                onActivated: index => {
                    ActionLog.taken("AiChangeCorrection_howTextCompared")
                    conditionModel.setProperty(conditionRow.index, "test", correction.tests[index].key)
                }
            }

            Controls.TextField {
                Layout.fillWidth: true
                visible: conditionRow.isText
                text: conditionRow.text
                Accessible.name: correction.i18nContext.i18nc("@label:textbox", "The text to look for")
                onTextEdited: conditionModel.setProperty(conditionRow.index, "text", text)
            }

            Item {
                Layout.fillWidth: true
                visible: !conditionRow.isText
            }

            Controls.ToolButton {
                icon.name: "list-remove"
                display: Controls.AbstractButton.IconOnly
                text: correction.i18nContext.i18nc("@action:button", "Remove this condition")
                Accessible.name: text
                Controls.ToolTip.text: text
                Controls.ToolTip.visible: hovered
                onClicked: {
                    ActionLog.taken("AiChangeCorrection_removeCondition")
                    conditionModel.remove(conditionRow.index)
                }
            }
        }
    }

    ListModel {
        id: splitPartModel
    }

    Repeater {
        model: correction.editing && correction.kind === "splitTransaction" ? splitPartModel : null

        ColumnLayout {
            id: splitPartRow

            required property int index
            required property string displayAmount
            required property string categoryId
            required property string newCategory
            required property string name

            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Controls.ComboBox {
                    Layout.fillWidth: true
                    model: [correction.i18nContext.i18nc("@item:inlistbox the part gets no category yet", "Not sorted yet")].concat(correction.categories.map(choice => choice.path))
                    currentIndex: correction.splitChoiceIndex({ category_id: splitPartRow.categoryId, new_category: splitPartRow.newCategory })
                    Accessible.name: correction.i18nContext.i18nc("@label:listbox", "The part's category")
                    onActivated: index => {
                        ActionLog.taken("AiChangeCorrection_splitPartCategory")
                        if (index <= 0) {
                            splitPartModel.setProperty(splitPartRow.index, "categoryId", "")
                            splitPartModel.setProperty(splitPartRow.index, "newCategory", "")
                        } else {
                            const choice = correction.categories[index - 1]
                            splitPartModel.setProperty(splitPartRow.index, "categoryId", choice.id)
                            splitPartModel.setProperty(splitPartRow.index, "newCategory", choice.key)
                        }
                    }
                }

                Controls.Label {
                    textFormat: Text.PlainText
                    text: splitPartRow.displayAmount
                }
            }

            Controls.TextField {
                Layout.fillWidth: true
                visible: correction.change.noteUnlocked === true
                text: splitPartRow.name
                placeholderText: correction.i18nContext.i18nc("@info:placeholder", "What this part was for (optional)")
                Accessible.name: correction.i18nContext.i18nc("@label:textbox", "What this part was for")
                onTextEdited: splitPartModel.setProperty(splitPartRow.index, "name", text)
                onAccepted: {
                    ActionLog.taken("AiChangeCorrection_splitPartName")
                    correction.save()
                }
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        visible: correction.editing

        Controls.Button {
            visible: correction.kind === "newRule"
            icon.name: "list-add"
            text: correction.i18nContext.i18nc("@action:button", "Add a condition")
            onClicked: {
                ActionLog.taken("AiChangeCorrection_addCondition")
                conditionModel.append({ subject: "payee", test: "contains", text: "", caseSensitive: false })
            }
        }

        Item {
            Layout.fillWidth: true
        }

        Controls.Button {
            icon.name: "dialog-cancel"
            text: correction.i18nContext.i18nc("@action:button", "Cancel")
            onClicked: {
                ActionLog.taken("AiChangeCorrection_cancel")
                correction.editing = false
            }
        }

        Controls.Button {
            highlighted: true
            icon.name: "dialog-ok"
            text: correction.i18nContext.i18nc("@action:button keep the user's edit of a prepared change", "Use my change")
            onClicked: {
                ActionLog.taken("AiChangeCorrection_useChange")
                correction.save()
            }
        }
    }
}
