// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The form of a planned payment (US7.1, D-769, docs/SCHEDULES.md 2.3): a new
// one, one to change, or one "Plan this again…" filled from an entry. A line
// under the form shows the next three dates, so nobody has to work out "the
// 1st that is a Sunday". Every amount and date is text from Rust (D-065).
//
//     planForm.edit(planId);              // change a plan
//     planForm.create(accountId);         // a new plan
//     planForm.fromEntryForm(form);       // what formFromEntry() returned

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    objectName: "planFormDialog"

    required property PlannedPayments plans
    // For the accounts and the categories to choose from.
    required property AccountRegister register

    // The plan being changed; empty for a new one.
    property string planId: ""
    // The entry "Plan this again…" started from; empty otherwise.
    property string fromEntry: ""
    property var accountChoices: []
    property var categoryChoices: []
    property string accountId: ""
    property string categoryId: ""
    property bool outgoing: true
    property string unit: "month"
    // The next three dates, read again as the form changes.
    property var nextDates: []

    signal saved(string planId)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function create(account: string) {
        planId = "";
        fill(plans.form("", account));
    }

    function edit(id: string) {
        planId = id;
        fill(plans.form(id, ""));
    }

    function fromEntryForm(form: var) {
        planId = "";
        fill(form);
    }

    function fill(form: var) {
        accountChoices = register.accountChoices();
        if (form.code !== undefined) {
            error.result = form;
            open();
            return;
        }
        error.result = ({});
        fromEntry = form.fromEntry ?? "";
        nameField.text = form.name ?? "";
        accountId = form.accountId !== "" ? form.accountId : ((accountChoices[0] ?? ({})).id ?? "");
        outgoing = form.outgoing ?? true;
        amountField.text = form.amount ?? "";
        categoryId = form.categoryId ?? "";
        unit = form.unit !== "" ? form.unit : "month";
        everyField.value = parseInt(form.every) > 0 ? parseInt(form.every) : 1;
        firstDueField.text = form.firstDue ?? "";
        endsBox.checked = (form.ends ?? "") !== "";
        endsField.text = form.ends ?? "";
        toleranceField.text = form.tolerance ?? "";
        partyField.text = form.party ?? "";
        reloadCategories();
        refreshDates();
        open();
        nameField.forceActiveFocus();
    }

    function reloadCategories() {
        const found = register.categoryChoices(accountId, !outgoing, categoryId);
        const choices = [];
        if (found.code === undefined) {
            for (let i = 0; i < found.length; ++i) {
                choices.push(found[i]);
            }
        }
        categoryChoices = choices;
        const at = choices.findIndex(choice => choice.id === categoryId);
        categoryBox.currentIndex = at >= 0 ? at + 1 : 0;
    }

    function form(): var {
        return {
            name: nameField.text,
            accountId: accountId,
            outgoing: outgoing,
            amount: amountField.text,
            categoryId: categoryId,
            unit: unit,
            every: String(everyField.value),
            firstDue: firstDueField.text,
            ends: endsBox.checked ? endsField.text : "",
            tolerance: toleranceField.text,
            party: partyField.text,
            fromEntry: fromEntry,
        };
    }

    function refreshDates() {
        nextDates = plans.nextDates(form());
    }

    title: Plain.plain(planId !== ""
        ? i18nContext.i18nc("@title:window", "Change a planned payment")
        : i18nContext.i18nc("@title:window", "New planned payment"))
    preferredWidth: Kirigami.Units.gridUnit * 30
    standardButtons: Kirigami.Dialog.NoButton
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Save")
            icon.name: "document-save"
            enabled: nameField.text.trim() !== "" && amountField.text.trim() !== "" && dialog.accountId !== ""
            onTriggered: {
                ActionLog.taken("PlanFormDialog_save")
                const result = dialog.plans.save(dialog.planId, dialog.form());
                if (result.code !== undefined) {
                    error.result = result;
                } else {
                    dialog.close();
                    dialog.saved(result.id);
                }
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Cancel")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("PlanFormDialog_cancel")
                dialog.close()
            }
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        FormError {
            id: error

            Layout.fillWidth: true
        }

        Controls.Label {
            Layout.fillWidth: true
            text: dialog.i18nContext.i18nc("@info", "A planned payment is only a plan: it moves no money and sends nothing. When your bank's entry comes, KassenSturz matches it to the plan.")
            wrapMode: Text.Wrap
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.TextField {
                id: nameField

                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:textbox", "What is it for:")
                placeholderText: dialog.i18nContext.i18nc("@info:placeholder", "e.g. Rent")
                maximumLength: 60
            }

            Controls.ComboBox {
                id: accountBox

                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:listbox", "Account:")
                model: dialog.accountChoices.map(choice => choice.name)
                currentIndex: dialog.accountChoices.findIndex(choice => choice.id === dialog.accountId)
                onActivated: index => {
                    ActionLog.taken("PlanFormDialog_chooseAccount")
                    dialog.accountId = dialog.accountChoices[index]?.id ?? "";
                    dialog.reloadCategories();
                }
            }

            RowLayout {
                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label", "Money:")

                Controls.RadioButton {
                    text: dialog.i18nContext.i18nc("@option:radio", "Goes out")
                    checked: dialog.outgoing
                    onToggled: {
                        ActionLog.taken("PlanFormDialog_out")
                        dialog.outgoing = true;
                        dialog.reloadCategories();
                    }
                }

                Controls.RadioButton {
                    text: dialog.i18nContext.i18nc("@option:radio", "Comes in")
                    checked: !dialog.outgoing
                    onToggled: {
                        ActionLog.taken("PlanFormDialog_in")
                        dialog.outgoing = false;
                        dialog.reloadCategories();
                    }
                }
            }

            Controls.TextField {
                id: amountField

                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:textbox", "Amount:")
                inputMethodHints: Qt.ImhFormattedNumbersOnly
            }

            Controls.ComboBox {
                id: categoryBox

                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:listbox", "Category:")
                model: [dialog.i18nContext.i18nc("@item:inlistbox the planned payment has no category", "None")]
                    .concat(dialog.categoryChoices.map(choice => choice.path))
                onActivated: index => {
                    ActionLog.taken("PlanFormDialog_chooseCategory")
                    dialog.categoryId = index > 0 ? (dialog.categoryChoices[index - 1]?.id ?? "") : "";
                }
            }

            RowLayout {
                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label", "How often:")

                Controls.Label {
                    text: dialog.i18nContext.i18nc("@label followed by a number and weeks, months or years", "every")
                }

                Controls.SpinBox {
                    id: everyField

                    from: 1
                    to: 52
                    value: 1
                    Accessible.name: dialog.i18nContext.i18nc("@label:spinbox", "How many weeks, months or years apart")
                    onValueModified: dialog.refreshDates()
                }

                Controls.ComboBox {
                    id: unitBox

                    readonly property var units: ["week", "month", "year"]

                    model: [
                        dialog.i18nContext.i18ncp("@item:inlistbox every N weeks", "week", "weeks", everyField.value),
                        dialog.i18nContext.i18ncp("@item:inlistbox every N months", "month", "months", everyField.value),
                        dialog.i18nContext.i18ncp("@item:inlistbox every N years", "year", "years", everyField.value),
                    ]
                    currentIndex: units.indexOf(dialog.unit)
                    Accessible.name: dialog.i18nContext.i18nc("@label:listbox", "Weeks, months or years")
                    onActivated: index => {
                        ActionLog.taken("PlanFormDialog_chooseUnit")
                        dialog.unit = units[index];
                        dialog.refreshDates();
                    }
                }
            }

            Controls.TextField {
                id: firstDueField

                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:textbox", "First due on:")
                onTextEdited: dialog.refreshDates()
            }

            RowLayout {
                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label", "Ends:")

                Controls.CheckBox {
                    id: endsBox

                    text: dialog.i18nContext.i18nc("@option:check", "On")
                    onToggled: {
                        ActionLog.taken("PlanFormDialog_ends")
                        dialog.refreshDates();
                    }
                }

                Controls.TextField {
                    id: endsField

                    enabled: endsBox.checked
                    Accessible.name: dialog.i18nContext.i18nc("@label:textbox", "The day it ends")
                    onTextEdited: dialog.refreshDates()
                }
            }

            Controls.TextField {
                id: toleranceField

                Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:textbox", "The amount may vary by up to:")
                placeholderText: dialog.i18nContext.i18nc("@info:placeholder", "Optional, for a bill that is not always the same")
                inputMethodHints: Qt.ImhFormattedNumbersOnly
            }

            Controls.TextField {
                id: partyField

                Kirigami.FormData.label: dialog.outgoing
                    ? dialog.i18nContext.i18nc("@label:textbox", "Paid to:")
                    : dialog.i18nContext.i18nc("@label:textbox", "Received from:")
                placeholderText: dialog.i18nContext.i18nc("@info:placeholder", "Optional: a name or IBAN as your bank shows it")
                maximumLength: 140
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: dialog.nextDates.length > 0
                ? dialog.i18nContext.i18nc("@info %1 is a list of dates", "Next: %1", dialog.nextDates.join(", "))
                : dialog.i18nContext.i18nc("@info", "Next: no date yet. Check how often and the first date.")
        }
    }
}
