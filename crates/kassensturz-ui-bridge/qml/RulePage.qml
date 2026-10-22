// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Adds a Rule, or changes or removes one (M3.11, D-088): when a transaction
// fits its conditions, the Rule sorts it into a category, moves it to one of
// your accounts, or sets the payee. A Rule only sorts transactions that have
// no category yet; what you sorted yourself stays as you left it. Changing a
// Rule can also book again what it sorted before.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property RuleBook rules
    // The Rule to change, by ID; empty to add one.
    property string ruleId: ""
    // The Rule a new one goes inside, by ID; empty for the top level.
    property string parentId: ""
    // A form to start from, e.g. made from a transaction; empty to start
    // with a blank one.
    property var initialForm: ({})

    readonly property bool adding: ruleId === ""
    readonly property var choices: rules.choices()
    // How a date and an amount are typed, as examples in the fields.
    property string dateExample: ""
    property string amountExample: ""
    // What the last "Show what would change" found.
    property var rebookPreview: ({})
    // The transactions the conditions fit as typed now, newest first, and
    // how many of them are listed (D-133).
    property int previewCount: 0
    property var previewRows: []
    readonly property int previewLimit: 15

    // A Rule was added, changed or removed.
    signal done()
    // The user wants a Rule inside this one.
    signal addSubRuleClicked(string ruleId)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: adding
        ? i18nContext.i18nc("@title", "Add a Rule")
        : i18nContext.i18nc("@title %1 is the name of a Rule", "Change %1", nameField.text)
    header: FormError {
        id: error
    }

    ListModel {
        id: conditionModel
    }

    // The list of what fits follows what is typed, a moment after the last
    // change.
    Timer {
        id: previewTimer

        interval: 300
        onTriggered: page.refreshPreview()
    }

    Connections {
        target: conditionModel

        function onDataChanged() {
            previewTimer.restart();
        }
        function onRowsInserted() {
            previewTimer.restart();
        }
        function onRowsRemoved() {
            previewTimer.restart();
        }
        function onModelReset() {
            previewTimer.restart();
        }
    }

    onMatchAllChanged: previewTimer.restart()

    function refreshPreview() {
        const found = rules.previewMatches(form());
        const shown = [];
        for (let at = 0; at < found.length && at < previewLimit; at++) {
            shown.push(found[at]);
        }
        previewCount = found.length;
        previewRows = shown;
    }

    // The Rule may have been removed by an undo (M3.8).
    function survivesUndo(): bool {
        return adding || rules.formOf(ruleId).code === undefined;
    }

    function loadForm(form: var) {
        nameField.text = form.name ?? "";
        matchAll = form.matchAll ?? true;
        conditionModel.clear();
        for (const condition of form.conditions ?? []) {
            conditionModel.append(condition);
        }
        bookToKind = form.bookToKind ?? "";
        bookTo = form.bookTo ?? "";
        payeeField.text = form.payee ?? "";
    }

    property bool matchAll: true
    property string bookToKind: ""
    property string bookTo: ""

    Component.onCompleted: {
        let form = initialForm;
        if (!adding) {
            form = rules.formOf(ruleId);
        } else if (form.conditions === undefined) {
            form = rules.newForm(parentId);
        }
        if (form.code !== undefined) {
            error.result = form;
            form = rules.newForm(parentId);
        }
        loadForm(form);
        refreshPreview();
        dateExample = rules.dateExample();
        amountExample = rules.amountExample(choices.currency);
    }

    // What the user entered, as the Rust side takes it.
    function form(): var {
        const conditions = [];
        for (let at = 0; at < conditionModel.count; at++) {
            conditions.push(Object.assign({}, conditionModel.get(at)));
        }
        return {
            id: ruleId,
            parent: parentId,
            name: nameField.text,
            matchAll: matchAll,
            conditions: conditions,
            bookToKind: bookToKind,
            bookTo: bookTo,
            payee: payeeField.text,
        };
    }

    function addCondition() {
        conditionModel.append(rules.newForm("").conditions[0]);
    }

    function save() {
        const result = adding
            ? rules.addRule(form())
            : rules.changeRule(form(), rebookBox.checked);
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    function remove() {
        const result = rules.removeRule(ruleId);
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    function showRebook() {
        const result = rules.previewRebook(form());
        error.result = result;
        rebookPreview = result.code === undefined ? result : ({});
    }

    // The categories to choose from, in tree order, with their kind.
    readonly property var targets: bookToKind === "transfer" ? choices.accounts : choices.categories

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.TextField {
                id: nameField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Name:")
                placeholderText: page.i18nContext.i18nc("@info:placeholder", "For example Supermarket")
                Accessible.name: page.i18nContext.i18nc("@label:textbox", "The name of the Rule")
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                text: page.i18nContext.i18nc("@info", "A Rule sorts transactions for you. The first Rule that fits a transaction decides; put the most specific ones first in the list. Transactions you sorted yourself are never changed.")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }
        }

        Kirigami.Heading {
            level: 2
            text: page.i18nContext.i18nc("@title", "When a transaction fits")
        }

        Controls.ComboBox {
            visible: conditionModel.count > 1
            model: [
                page.i18nContext.i18nc("@item:inlistbox", "All of these conditions"),
                page.i18nContext.i18nc("@item:inlistbox", "At least one of these conditions")
            ]
            currentIndex: page.matchAll ? 0 : 1
            onActivated: index => page.matchAll = index === 0
            Accessible.name: page.i18nContext.i18nc("@label:listbox", "How many conditions have to fit")
        }

        Repeater {
            model: conditionModel

            delegate: RuleCondition {
                id: row

                required property int index
                required property var model

                Layout.fillWidth: true
                condition: model
                choices: page.choices
                dateExample: page.dateExample
                amountExample: page.amountExample
                onEdited: values => {
                    conditionModel.set(row.index, values);
                }
                onRemoveClicked: conditionModel.remove(row.index)
            }
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button", "Add a condition")
            icon.name: "list-add"
            onClicked: page.addCondition()
        }

        // What the conditions fit now, so a Rule can be checked before it is
        // saved.
        Kirigami.Heading {
            level: 3
            text: page.i18nContext.i18ncp("@title %1 is a number", "These conditions fit %1 transaction", "These conditions fit %1 transactions", page.previewCount)
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 28
            text: page.i18nContext.i18nc("@info", "Checked on their own: a Rule before this one in the list may still take a transaction first.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Repeater {
            model: page.previewRows

            delegate: TransactionRow {
                required property var modelData

                Layout.fillWidth: true
                date: modelData.date
                otherParty: modelData.otherParty
                category: modelData.category
                amount: modelData.amount
                exactAmount: modelData.exactAmount
                incoming: modelData.incoming
                kind: modelData.kind
                account: modelData.account
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: page.previewCount > page.previewLimit
            text: page.i18nContext.i18nc("@info %1 is a number", "…and %1 more.", page.previewCount - page.previewLimit)
            font: Kirigami.Theme.smallFont
        }

        Kirigami.Heading {
            level: 2
            text: page.i18nContext.i18nc("@title", "Then")
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.ComboBox {
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Sort it:")
                model: [
                    page.i18nContext.i18nc("@item:inlistbox the Rule does not sort", "Leave as it is"),
                    page.i18nContext.i18nc("@item:inlistbox the Rule sorts into a category", "Into a category"),
                    page.i18nContext.i18nc("@item:inlistbox the Rule moves the money to one of the user's accounts", "Move it to one of my accounts")
                ]
                currentIndex: page.bookToKind === "category" ? 1 : page.bookToKind === "transfer" ? 2 : 0
                onActivated: index => {
                    page.bookToKind = ["", "category", "transfer"][index];
                    page.bookTo = page.targets.length > 0 && page.bookToKind !== "" ? page.targets[0].id : "";
                }
                Accessible.name: page.i18nContext.i18nc("@label:listbox", "What the Rule does with the transaction")
            }

            Controls.ComboBox {
                visible: page.bookToKind !== ""
                Kirigami.FormData.label: page.bookToKind === "transfer"
                    ? page.i18nContext.i18nc("@label:listbox", "Account:")
                    : page.i18nContext.i18nc("@label:listbox", "Category:")
                model: page.targets.map(target => target.name)
                currentIndex: Math.max(0, page.targets.findIndex(target => target.id === page.bookTo))
                onActivated: index => page.bookTo = page.targets[index].id
                Accessible.name: page.i18nContext.i18nc("@label:listbox", "Where the transaction goes")
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                visible: page.bookToKind === "transfer"
                text: page.i18nContext.i18nc("@info", "The money is then moved between your own accounts, for example cash taken out at a machine. It doesn't count as spending.")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }

            Controls.TextField {
                id: payeeField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Show this name as From or To:")
                placeholderText: page.i18nContext.i18nc("@info:placeholder", "Leave empty to keep it")
                Accessible.name: page.i18nContext.i18nc("@label:textbox", "The name to show")
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true
            visible: !page.adding

            Controls.CheckBox {
                id: rebookBox
                text: page.i18nContext.i18nc("@option:check", "Also sort again what this Rule sorted before")
                onToggled: {
                    page.rebookPreview = ({});
                    if (checked) {
                        page.showRebook();
                    }
                }
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                text: page.i18nContext.i18nc("@info", "Without this, only new transactions follow the changed Rule. Transactions you changed yourself are left alone either way.")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                visible: rebookBox.checked && page.rebookPreview.changed !== undefined
                text: page.rebookPreview.changed === undefined
                    ? ""
                    : page.i18nContext.i18ncp("@info %1 is a number", "%1 transaction would be sorted differently.", "%1 transactions would be sorted differently.", parseInt(page.rebookPreview.changed))
                        + (parseInt(page.rebookPreview.skipped) > 0
                            ? " " + page.i18nContext.i18nc("@info %1 is a number", "%1 can't be changed, for example because an exchange rate is missing.", page.rebookPreview.skipped)
                            : "")
                wrapMode: Text.Wrap
            }

            Controls.Button {
                visible: rebookBox.checked
                text: page.i18nContext.i18nc("@action:button", "Show what would change again")
                flat: true
                onClicked: page.showRebook()
            }
        }

        RowLayout {
            Controls.Button {
                text: page.adding
                    ? page.i18nContext.i18nc("@action:button", "Add Rule")
                    : page.i18nContext.i18nc("@action:button", "Save")
                icon.name: page.adding ? "list-add" : "document-save"
                onClicked: page.save()
            }

            Controls.Button {
                visible: !page.adding
                text: page.i18nContext.i18nc("@action:button", "Add a Rule inside this one…")
                icon.name: "list-add"
                flat: true
                onClicked: page.addSubRuleClicked(page.ruleId)
            }

            Controls.Button {
                visible: !page.adding
                text: page.i18nContext.i18nc("@action:button", "Remove")
                icon.name: "edit-delete"
                onClicked: page.remove()
            }
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: !page.adding
            text: page.i18nContext.i18nc("@info", "A Rule inside another one is only checked when the outer Rule fits, and can set a more specific category or name. Removing a Rule removes the Rules inside it. What it sorted stays sorted. Undo brings it back.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
