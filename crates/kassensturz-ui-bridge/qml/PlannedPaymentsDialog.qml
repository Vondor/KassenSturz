// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The planned payments (US7.1, D-769, docs/SCHEDULES.md 2.2 and 2.5): the
// payments whose day passed without an entry first ("Did this happen?"),
// then every plan with Edit, Pause and Remove. Nothing is booked, matched or
// skipped without the user's click, and each click is one change that Undo
// takes back. Every amount and date is text from Rust (D-065).
//
//     plannedPayments.show();

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    objectName: "plannedPaymentsDialog"

    required property PlannedPayments plans
    required property AccountRegister register

    readonly property var view: plans.view ?? ({})
    readonly property var planList: view.plans ?? []
    readonly property var toConfirm: view.toConfirm ?? []
    // The month by month table, only when a plan counts (D-770).
    readonly property var months: (view.forecast?.hasPlans ?? false) ? (view.forecast?.months ?? []) : []

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Opens and reads the plans; it opens even if reading them fails, so
    // the button never does nothing (as MonthlyMovesDialog, M6-B242).
    function show() {
        open();
        plans.refresh();
        error.result = view.code !== undefined ? view : ({});
        Qt.callLater(() => {
            if (!dialog.visible) {
                dialog.open();
            }
        });
    }

    // "Rent: 750,00 € out of Checking account, every month"
    function describe(plan: var): string {
        const how = plan.unit === "week"
            ? i18nContext.i18ncp("@info", "every week", "every %1 weeks", plan.every)
            : plan.unit === "year"
                ? i18nContext.i18ncp("@info", "every year", "every %1 years", plan.every)
                : i18nContext.i18ncp("@info", "every month", "every %1 months", plan.every);
        return plan.outgoing
            ? i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 how often, e.g. every month", "%1 out of %2, %3", plan.amount, plan.account, how)
            : i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 how often, e.g. every month", "%1 into %2, %3", plan.amount, plan.account, how);
    }

    function changed(result: var) {
        error.result = result.code !== undefined ? result : ({});
    }

    title: i18nContext.i18nc("@title:window", "Planned payments")
    standardButtons: Kirigami.Dialog.Close
    preferredWidth: Kirigami.Units.gridUnit * 32

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        FormError {
            id: error

            Layout.fillWidth: true
        }

        Controls.Button {
            icon.name: "list-add"
            text: dialog.i18nContext.i18nc("@action:button", "New planned payment…")
            onClicked: {
                ActionLog.taken("PlannedPaymentsDialog_new")
                planForm.create("");
            }
        }

        // No dead ends (AGENTS.md §6): say how a plan gets here.
        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: dialog.planList.length === 0 && error.result.code === undefined
            text: dialog.i18nContext.i18nc("@info", "Nothing planned yet")
            explanation: dialog.i18nContext.i18nc("@info", "Plan the payments that come again and again, such as rent or your salary. KassenSturz then matches your bank's entries to them. The quickest way: open an entry in an account and choose “Plan this again…”.")
        }

        // Payments whose day passed without an entry (2.5).
        Kirigami.Heading {
            Layout.fillWidth: true
            visible: dialog.toConfirm.length > 0
            level: 3
            text: dialog.i18nContext.i18nc("@title:group", "Did this happen?")
        }

        Repeater {
            model: dialog.toConfirm

            delegate: ColumnLayout {
                id: payment

                required property var modelData

                Layout.fillWidth: true
                spacing: 0

                Controls.Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font.bold: true
                    text: payment.modelData.outgoing
                        ? dialog.i18nContext.i18nc("@info %1 is the name of a planned payment, %2 an amount, %3 a date", "%1: %2 out, due %3", payment.modelData.name, payment.modelData.amount, payment.modelData.due)
                        : dialog.i18nContext.i18nc("@info %1 is the name of a planned payment, %2 an amount, %3 a date", "%1: %2 in, due %3", payment.modelData.name, payment.modelData.amount, payment.modelData.due)
                }

                Controls.Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font: Kirigami.Theme.smallFont
                    text: dialog.i18nContext.i18nc("@info %1 is the name of an account", "No entry in %1 fits it yet.", payment.modelData.account)
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    Controls.Button {
                        icon.name: "edit-link"
                        text: dialog.i18nContext.i18nc("@action:button", "Match an entry…")
                        onClicked: {
                            ActionLog.taken("PlannedPaymentsDialog_match")
                            matchDialog.ask(payment.modelData);
                        }
                    }

                    Controls.Button {
                        icon.name: "list-add"
                        text: dialog.i18nContext.i18nc("@action:button", "Book it…")
                        onClicked: {
                            ActionLog.taken("PlannedPaymentsDialog_book")
                            bookDialog.ask(payment.modelData);
                        }
                    }

                    Controls.Button {
                        icon.name: "media-skip-forward"
                        text: dialog.i18nContext.i18nc("@action:button", "Skip this time")
                        onClicked: {
                            ActionLog.taken("PlannedPaymentsDialog_skip")
                            const data = payment.modelData;
                            // Later: reading again destroys this button while
                            // its click is still being handled (M6-B242).
                            Qt.callLater(() => dialog.changed(dialog.plans.skip(data.planId, data.dueOn)));
                        }
                    }
                }
            }
        }

        Kirigami.Heading {
            Layout.fillWidth: true
            visible: dialog.planList.length > 0
            level: 3
            text: dialog.i18nContext.i18nc("@title:group", "Your planned payments")
        }

        Repeater {
            model: dialog.planList

            delegate: RowLayout {
                id: row

                required property var modelData

                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0

                    Controls.Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        font.bold: true
                        text: row.modelData.name
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        text: dialog.describe(row.modelData)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        font: Kirigami.Theme.smallFont
                        text: row.modelData.accountClosed
                            ? dialog.i18nContext.i18nc("@info", "Waits while its account is closed.")
                            : row.modelData.paused
                                ? dialog.i18nContext.i18nc("@info", "Paused: not expected, matched or counted.")
                                : row.modelData.nextDue !== ""
                                    ? dialog.i18nContext.i18nc("@info %1 is a date", "Next: %1", row.modelData.nextDue)
                                    : dialog.i18nContext.i18nc("@info", "No more payments: it has ended.")
                    }
                }

                Controls.Button {
                    icon.name: "document-edit"
                    text: dialog.i18nContext.i18nc("@action:button", "Edit")
                    display: Controls.AbstractButton.IconOnly
                    Controls.ToolTip.text: text
                    Controls.ToolTip.visible: hovered
                    Accessible.description: dialog.i18nContext.i18nc("@info %1 is the name of a planned payment", "Change “%1”", row.modelData.name)
                    onClicked: {
                        ActionLog.taken("PlannedPaymentsDialog_edit")
                        planForm.edit(row.modelData.id);
                    }
                }

                Controls.Button {
                    icon.name: row.modelData.paused ? "media-playback-start" : "media-playback-pause"
                    text: Plain.plain(row.modelData.paused
                        ? dialog.i18nContext.i18nc("@action:button", "Go on")
                        : dialog.i18nContext.i18nc("@action:button", "Pause"))
                    display: Controls.AbstractButton.IconOnly
                    visible: !row.modelData.accountClosed
                    Controls.ToolTip.text: text
                    Controls.ToolTip.visible: hovered
                    Accessible.description: dialog.i18nContext.i18nc("@info %1 is the name of a planned payment", "Pause “%1” or let it go on", row.modelData.name)
                    onClicked: {
                        ActionLog.taken("PlannedPaymentsDialog_pause")
                        const data = row.modelData;
                        Qt.callLater(() => dialog.changed(dialog.plans.setPaused(data.id, !data.paused)));
                    }
                }

                Controls.Button {
                    icon.name: "edit-delete"
                    text: dialog.i18nContext.i18nc("@action:button", "Remove")
                    display: Controls.AbstractButton.IconOnly
                    Controls.ToolTip.text: text
                    Controls.ToolTip.visible: hovered
                    Accessible.description: dialog.i18nContext.i18nc("@info %1 is the name of a planned payment", "Remove “%1”", row.modelData.name)
                    onClicked: {
                        ActionLog.taken("PlannedPaymentsDialog_remove")
                        removeDialog.ask(row.modelData);
                    }
                }
            }
        }

        // The months after this one (D-770, 5.2): only the balance and the
        // plans, each month's end the next one's start. Every figure is
        // text from Rust (D-065).
        Kirigami.Heading {
            Layout.fillWidth: true
            visible: dialog.months.length > 0
            level: 3
            text: dialog.i18nContext.i18nc("@title:group", "Month by month")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: dialog.months.length > 0
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
            text: dialog.i18nContext.i18nc("@info", "Based on your planned payments only. Spending that is not planned is not included.")
        }

        GridLayout {
            Layout.fillWidth: true
            visible: dialog.months.length > 0
            columns: 5
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.smallSpacing

            Repeater {
                model: [
                    dialog.i18nContext.i18nc("@title:column a month", "Month"),
                    dialog.i18nContext.i18nc("@title:column the balance a month starts with", "Start"),
                    dialog.i18nContext.i18nc("@title:column planned income", "In"),
                    dialog.i18nContext.i18nc("@title:column planned payments", "Out"),
                    dialog.i18nContext.i18nc("@title:column the balance a month ends with", "End")
                ]

                delegate: Controls.Label {
                    required property string modelData

                    textFormat: Text.PlainText
                    font.bold: true
                    text: modelData
                }
            }

            Repeater {
                // Five cells per month, in reading order.
                // A month that starts on a salary day is named by its
                // first and last day (#921).
                model: dialog.months.flatMap(month => [
                    (dialog.view.forecast?.startsOn ?? 1) === 1 ? month.month
                        : dialog.i18nContext.i18nc("@item a month that starts on a salary day, %1 its first and %2 its last day", "%1 to %2", month.first, month.last),
                    month.start, month.plannedIn, month.plannedOut, month.end])

                delegate: Controls.Label {
                    required property string modelData
                    required property int index

                    Layout.alignment: index % 5 === 0 ? Qt.AlignLeft : Qt.AlignRight
                    textFormat: Text.PlainText
                    text: modelData
                }
            }
        }
    }

    PlanFormDialog {
        id: planForm

        plans: dialog.plans
        register: dialog.register
    }

    // Removing says what stays (2.2).
    Kirigami.PromptDialog {
        id: removeDialog

        property var plan: ({})

        function ask(chosen: var) {
            plan = chosen;
            open();
        }

        title: dialog.i18nContext.i18nc("@title:window %1 is the name of a planned payment", "Remove “%1”?", Plain.plain(plan.name ?? ""))
        subtitle: dialog.i18nContext.i18nc("@info", "Entries already matched stay as they are. KassenSturz just stops expecting this payment. Undo brings it back.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: dialog.i18nContext.i18nc("@action:button", "Remove")
                icon.name: "edit-delete"
                onTriggered: {
                    ActionLog.taken("PlannedPaymentsDialog_removeConfirmed")
                    const id = removeDialog.plan.id;
                    removeDialog.close();
                    Qt.callLater(() => dialog.changed(dialog.plans.remove(id)));
                }
            },
            Kirigami.Action {
                text: dialog.i18nContext.i18nc("@action:button", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("PlannedPaymentsDialog_removeCancelled")
                    removeDialog.close()
                }
            }
        ]
    }

    // "Book it" (2.5): a new entry from the plan, the amount and date as the
    // user leaves them.
    Kirigami.Dialog {
        id: bookDialog

        property var payment: ({})

        function ask(chosen: var) {
            payment = chosen;
            const form = dialog.plans.bookForm(chosen.planId);
            bookError.result = form.code !== undefined ? form : ({});
            bookAmount.text = form.amount ?? "";
            bookDate.text = form.date ?? "";
            open();
        }

        title: dialog.i18nContext.i18nc("@title:window %1 is the name of a planned payment", "Book “%1”", Plain.plain(payment.name ?? ""))
        preferredWidth: Kirigami.Units.gridUnit * 24
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: dialog.i18nContext.i18nc("@action:button", "Book it")
                icon.name: "document-save"
                onTriggered: {
                    ActionLog.taken("PlannedPaymentsDialog_bookConfirmed")
                    const result = dialog.plans.bookIt(bookDialog.payment.planId, bookDialog.payment.dueOn, bookDate.text, bookAmount.text);
                    if (result.code !== undefined) {
                        bookError.result = result;
                    } else {
                        bookDialog.close();
                    }
                }
            },
            Kirigami.Action {
                text: dialog.i18nContext.i18nc("@action:button", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("PlannedPaymentsDialog_bookCancelled")
                    bookDialog.close()
                }
            }
        ]

        ColumnLayout {
            FormError {
                id: bookError

                Layout.fillWidth: true
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: dialog.i18nContext.i18nc("@info", "For an account you keep by hand: this adds the entry to the account, as you typed it. Undo takes it back.")
            }

            Kirigami.FormLayout {
                Layout.fillWidth: true

                Controls.TextField {
                    id: bookAmount

                    Kirigami.FormData.label: bookDialog.payment.outgoing ?? true
                        ? dialog.i18nContext.i18nc("@label:textbox", "Amount paid:")
                        : dialog.i18nContext.i18nc("@label:textbox", "Amount received:")
                    inputMethodHints: Qt.ImhFormattedNumbersOnly
                }

                Controls.TextField {
                    id: bookDate

                    Kirigami.FormData.label: dialog.i18nContext.i18nc("@label:textbox", "Date:")
                }
            }
        }
    }

    // "Match an entry…" (2.5): one of the account's entries the user picks.
    Kirigami.Dialog {
        id: matchDialog

        property var payment: ({})
        property var choices: []

        function ask(chosen: var) {
            payment = chosen;
            const found = dialog.plans.entryChoices(chosen.planId, chosen.dueOn);
            if (found?.length === undefined) {
                matchError.result = found;
                choices = [];
            } else {
                matchError.result = ({});
                choices = found;
            }
            open();
        }

        title: dialog.i18nContext.i18nc("@title:window %1 is the name of a planned payment", "Which entry is “%1”?", Plain.plain(payment.name ?? ""))
        preferredWidth: Kirigami.Units.gridUnit * 26
        standardButtons: Kirigami.Dialog.Cancel

        ColumnLayout {
            FormError {
                id: matchError

                Layout.fillWidth: true
            }

            Kirigami.PlaceholderMessage {
                Layout.fillWidth: true
                visible: matchDialog.choices.length === 0 && matchError.result.code === undefined
                text: dialog.i18nContext.i18nc("@info", "No entry to choose")
                explanation: dialog.i18nContext.i18nc("@info", "Only entries of the plan's account within a month of its day, with the money going the same way and not matched yet, are offered. If it was paid another way, choose “Book it…” or “Skip this time”.")
            }

            Repeater {
                model: matchDialog.choices

                delegate: Controls.ItemDelegate {
                    id: choice

                    required property var modelData

                    Layout.fillWidth: true
                    text: Plain.plain(dialog.i18nContext.i18nc("@info %1 is a date, %2 who paid or was paid, %3 an amount", "%1 · %2 · %3", choice.modelData.date, choice.modelData.payee, choice.modelData.amount))
                    onClicked: {
                        ActionLog.taken("PlannedPaymentsDialog_matchConfirmed")
                        const result = dialog.plans.matchEntry(matchDialog.payment.planId, matchDialog.payment.dueOn, choice.modelData.id);
                        if (result.code !== undefined) {
                            matchError.result = result;
                        } else {
                            matchDialog.close();
                        }
                    }
                }
            }
        }
    }
}
