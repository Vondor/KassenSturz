// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What an import did (M3.10, D-092): the counts, the bank's balances
// against KassenSturz's with the way to "Check against my bank" when they
// differ (D-029, D-111, D-123), and the question about every Transfer the
// new entries suggest (D-120). Everything arrives as text (D-065).

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property StatementImport statementImport
    // Any register will do for joining Transfers (D-120).
    required property AccountRegister register
    readonly property var view: statementImport.view ?? ({})
    readonly property var balances: view.balances ?? []
    readonly property string accountId: statementImport.accountId
    readonly property var joined: view.joined ?? []
    // The user took the joins back with "Undo these joins".
    property bool joinsUndone: false
    // New entries matched to planned payments (US7.1, D-769), and whether
    // the user took the matches back with "Undo these matches" (taking the
    // joins back takes them back too: they were made after the joins).
    readonly property var planned: view.planned ?? []
    property bool matchesUndone: false

    // The user is finished here.
    signal done()
    // Look into a balance that differs, with the bank's figure and day.
    signal checkAgainstBankClicked(string accountId, string bank, string date)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: Plain.plain(i18nContext.i18nc("@title %1 is the name of an account", "Imported into %1", view.accountName ?? ""))
    header: FormError {
        id: error
    }

    actions: Kirigami.Action {
        icon.name: "dialog-ok"
        text: page.i18nContext.i18nc("@action:button", "Done")
        onTriggered: {
            ActionLog.taken("ImportDonePage_done")
            page.done()
        }
    }

    // The Transfers the new entries suggest are asked about once the page
    // is up; a popup opened earlier wouldn't show.
    Timer {
        interval: 300
        running: true
        repeat: false
        onTriggered: transferPrompt.ask(page.statementImport.transferSuggestions())
    }

    TransferPrompt {
        id: transferPrompt

        register: page.register
    }

    // What the bank or KassenSturz says: a debt reads "you owed".
    function said(amount: string, owes: bool): string {
        return owes ? i18nContext.i18nc("@label %1 is an amount of money", "you owed %1", amount) : amount;
    }

    function balanceText(balance: var): string {
        return balance.which === "opening"
            ? i18nContext.i18nc("@info %1 is a date, %2 what the bank said the account held", "Before these transactions, on %1, your bank said: %2", balance.date, said(balance.bank, balance.bankOwes))
            : i18nContext.i18nc("@info %1 is a date, %2 what the bank said the account held", "After them, on %1, your bank said: %2", balance.date, said(balance.bank, balance.bankOwes));
    }

    function balanceDescription(balance: var): string {
        if (balance.matches) {
            return balance.stated
                ? i18nContext.i18nc("@info", "KassenSturz agrees. It remembers this as what your bank said, and the account's page tells you if that stops being true.")
                : i18nContext.i18nc("@info", "KassenSturz agrees.");
        }
        return balance.bookHigher
            ? i18nContext.i18nc("@info %1 is what KassenSturz has, %2 the difference", "KassenSturz has %1 for that day, %2 more. Nothing was changed about that; \"Check against my bank\" shows where the difference can come from.", said(balance.book, balance.bookOwes), balance.difference)
            : i18nContext.i18nc("@info %1 is what KassenSturz has, %2 the difference", "KassenSturz has %1 for that day, %2 less. Nothing was changed about that; \"Check against my bank\" shows where the difference can come from.", said(balance.book, balance.bookOwes), balance.difference);
    }

    ColumnLayout {
        spacing: 0

        FormCard.FormHeader {
            title: page.i18nContext.i18nc("@title:group", "What was imported")
        }

        FormCard.FormCard {
            FormCard.FormTextDelegate {
                icon.name: "dialog-ok-apply"
                text: page.view.fromBank
                    ? page.i18nContext.i18ncp("@info", "%1 transaction from your bank was added.", "%1 transactions from your bank were added.", page.view.imported ?? 0)
                    : page.i18nContext.i18ncp("@info %2 is a file name", "%1 transaction from %2 was added.", "%1 transactions from %2 were added.", page.view.imported ?? 0, page.view.fileName ?? "")
                textItem.textFormat: Text.PlainText
                description: page.i18nContext.i18nc("@info", "Each one is in the account's list now. Undo takes the whole import back.")
            }

            FormCard.FormDelegateSeparator {
                visible: (page.view.rules ?? 0) > 0
            }

            FormCard.FormTextDelegate {
                textItem.textFormat: Text.PlainText
                visible: (page.view.rules ?? 0) > 0
                icon.name: "view-filter"
                text: page.i18nContext.i18ncp("@info", "Your Rules sorted %1 of them into a category.", "Your Rules sorted %1 of them into categories.", page.view.rules ?? 0)
                description: page.i18nContext.i18nc("@info", "The others show up as \"Not sorted yet\" until you give them a category.")
            }

            FormCard.FormDelegateSeparator {
                visible: (page.view.toDecide ?? 0) > 0
            }

            FormCard.FormTextDelegate {
                descriptionItem.textFormat: Text.PlainText
                textItem.textFormat: Text.PlainText
                visible: (page.view.toDecide ?? 0) > 0
                icon.name: "dialog-question"
                text: page.i18nContext.i18ncp("@info", "%1 transaction was left out because it looks like one you already have.", "%1 transactions were left out because they look like transactions you already have.", page.view.toDecide ?? 0)
                description: page.view.fromBank
                    ? page.i18nContext.i18nc("@info", "If one of them is a different transaction after all, add it by hand.")
                    : page.i18nContext.i18nc("@info", "Import the file again and tick it if it is a different transaction after all.")
            }

            FormCard.FormDelegateSeparator {
                visible: (page.view.duplicates ?? 0) > 0
            }

            FormCard.FormTextDelegate {
                textItem.textFormat: Text.PlainText
                visible: (page.view.duplicates ?? 0) > 0
                icon.name: "edit-copy"
                text: page.i18nContext.i18ncp("@info", "%1 transaction was already in your book and was skipped.", "%1 transactions were already in your book and were skipped.", page.view.duplicates ?? 0)
            }

            FormCard.FormDelegateSeparator {
                visible: (page.view.pending ?? 0) > 0
            }

            FormCard.FormTextDelegate {
                textItem.textFormat: Text.PlainText
                visible: (page.view.pending ?? 0) > 0
                icon.name: "chronometer"
                text: page.i18nContext.i18ncp("@info", "%1 transaction isn't booked by your bank yet and was left out.", "%1 transactions aren't booked by your bank yet and were left out.", page.view.pending ?? 0)
                description: page.i18nContext.i18nc("@info", "It comes with a later statement, once the bank has booked it.")
            }
        }

        // Pairs that fit a monthly move the user asked to be joined by
        // itself (US6.3, D-749): listed with the amounts as the banks sent
        // them, with one button that takes them all back.
        FormCard.FormHeader {
            visible: page.joined.length > 0
            title: page.i18nContext.i18nc("@title:group", "Joined by themselves")
        }

        FormCard.FormCard {
            visible: page.joined.length > 0

            FormCard.FormTextDelegate {
                textItem.textFormat: Text.PlainText
                descriptionItem.textFormat: Text.PlainText
                icon.name: page.joinsUndone ? "edit-undo" : "edit-link"
                text: page.joinsUndone
                    ? page.i18nContext.i18nc("@info", "The joins are taken back. Both entries are in their accounts again.")
                    : page.i18nContext.i18ncp("@info", "%1 move between your accounts was joined, as you asked.", "%1 moves between your accounts were joined, as you asked.", page.joined.length)
                description: page.joinsUndone ? "" : page.i18nContext.i18nc("@info", "They count as moving your own money around, not as spending and income. Both accounts keep their balance and the dates they show.")
            }

            Repeater {
                model: page.joinsUndone ? [] : page.joined

                delegate: ColumnLayout {
                    id: joinedRow

                    required property var modelData

                    Layout.fillWidth: true
                    spacing: 0

                    FormCard.FormDelegateSeparator {}

                    FormCard.FormTextDelegate {
                        textItem.textFormat: Text.PlainText
                        icon.name: "go-next"
                        text: page.i18nContext.i18nc("@info %1 is an amount, %2 and %4 the names of accounts, %3 and %5 dates", "%1 from “%2” (%3) to “%4” (%5) · monthly move", joinedRow.modelData.amount, joinedRow.modelData.from, joinedRow.modelData.fromDate, joinedRow.modelData.to, joinedRow.modelData.toDate)
                    }
                }
            }

            FormCard.FormDelegateSeparator {
                visible: !page.joinsUndone
            }

            FormCard.FormButtonDelegate {
                visible: !page.joinsUndone
                descriptionItem.textFormat: Text.PlainText
                icon.name: "edit-undo"
                text: page.i18nContext.i18nc("@action:button", "Undo these joins")
                description: page.planned.length > 0 && !page.matchesUndone
                    ? page.i18nContext.i18nc("@info", "Takes them all apart again, and the planned payments found below with them. The imported entries stay.")
                    : page.i18nContext.i18nc("@info", "Takes them all apart again. The imported entries stay.")
                onClicked: {
                    ActionLog.taken("ImportDonePage_undoJoins")
                    const result = page.statementImport.undoJoins();
                    error.result = result;
                    if (result.code === undefined) {
                        page.joinsUndone = true;
                        page.matchesUndone = true;
                    }
                }
            }
        }

        // New entries that fit a planned payment (US7.1, D-769, 2.4): each
        // with the amount as the bank sent it, with one button that takes
        // the matches back and keeps the entries.
        FormCard.FormHeader {
            visible: page.planned.length > 0
            title: page.i18nContext.i18nc("@title:group", "Planned payments found")
        }

        FormCard.FormCard {
            visible: page.planned.length > 0

            FormCard.FormTextDelegate {
                textItem.textFormat: Text.PlainText
                descriptionItem.textFormat: Text.PlainText
                icon.name: page.matchesUndone ? "edit-undo" : "view-calendar-upcoming-events"
                text: page.matchesUndone
                    ? page.i18nContext.i18nc("@info", "The matches are taken back. The entries stay, and the planned payments wait again.")
                    : page.i18nContext.i18ncp("@info", "%1 new entry is a payment you planned.", "%1 new entries are payments you planned.", page.planned.length)
                description: page.matchesUndone ? "" : page.i18nContext.i18nc("@info", "Nothing was booked or changed in amount or date. An entry without a category got the plan's.")
            }

            Repeater {
                model: page.matchesUndone ? [] : page.planned

                delegate: ColumnLayout {
                    id: plannedRow

                    required property var modelData

                    Layout.fillWidth: true
                    spacing: 0

                    FormCard.FormDelegateSeparator {}

                    FormCard.FormTextDelegate {
                        textItem.textFormat: Text.PlainText
                        descriptionItem.textFormat: Text.PlainText
                        icon.name: "checkmark"
                        text: page.i18nContext.i18nc("@info %1 is the name of a planned payment, %2 an amount, %3 a date", "%1: %2 on %3, matched", plannedRow.modelData.name, plannedRow.modelData.amount, plannedRow.modelData.date)
                        description: plannedRow.modelData.categorySet
                            ? page.i18nContext.i18nc("@info", "It had no category and got the plan's.")
                            : ""
                    }
                }
            }

            FormCard.FormDelegateSeparator {
                visible: !page.matchesUndone
            }

            FormCard.FormButtonDelegate {
                visible: !page.matchesUndone
                icon.name: "edit-undo"
                text: page.i18nContext.i18nc("@action:button", "Undo these matches")
                description: page.i18nContext.i18nc("@info", "The entries stay; the planned payments wait for them again.")
                onClicked: {
                    ActionLog.taken("ImportDonePage_undoMatches")
                    const result = page.statementImport.undoMatches();
                    error.result = result;
                    if (result.code === undefined) {
                        page.matchesUndone = true;
                    }
                }
            }
        }

        // Payments of planned payments that new entries fit, left for the
        // user: two entries fit equally well, or one fits two payments.
        FormCard.FormCard {
            visible: (page.view.plannedNeedsYou ?? 0) > 0

            FormCard.FormTextDelegate {
                textItem.textFormat: Text.PlainText
                descriptionItem.textFormat: Text.PlainText
                icon.name: "dialog-question"
                text: page.i18nContext.i18ncp("@info", "%1 planned payment has entries that were not matched.", "%1 planned payments have entries that were not matched.", page.view.plannedNeedsYou ?? 0)
                description: page.i18nContext.i18nc("@info", "More than one entry fits, or one entry fits two payments. Choose yourself: Accounts › Planned payments…")
            }
        }

        // Sides that fit a monthly move but were left for the user.
        FormCard.FormHeader {
            visible: (page.view.needsYou ?? 0) > 0
            title: page.i18nContext.i18nc("@title:group", "Waiting for you")
        }

        FormCard.FormCard {
            visible: (page.view.needsYou ?? 0) > 0

            FormCard.FormTextDelegate {
                textItem.textFormat: Text.PlainText
                descriptionItem.textFormat: Text.PlainText
                icon.name: "dialog-question"
                text: page.i18nContext.i18ncp("@info", "%1 monthly move has entries that were not joined.", "%1 monthly moves have entries that were not joined.", page.view.needsYou ?? 0)
                description: page.i18nContext.i18nc("@info", "Two entries fit equally well, or a join would lose something you wrote. Join them yourself: Accounts › Find moves between my accounts…")
            }
        }

        FormCard.FormHeader {
            visible: page.balances.length > 0
            title: page.i18nContext.i18nc("@title:group", "Your bank's balances")
        }

        FormCard.FormCard {
            visible: page.balances.length > 0

            Repeater {
                model: page.balances

                delegate: ColumnLayout {
                    id: balanceRow

                    required property int index
                    required property var modelData

                    Layout.fillWidth: true
                    spacing: 0

                    FormCard.FormDelegateSeparator {
                        visible: balanceRow.index > 0
                    }

                    FormCard.FormTextDelegate {
                        descriptionItem.textFormat: Text.PlainText
                        textItem.textFormat: Text.PlainText
                        icon.name: balanceRow.modelData.matches ? "dialog-ok-apply" : "dialog-warning"
                        text: page.balanceText(balanceRow.modelData)
                        description: page.balanceDescription(balanceRow.modelData)
                    }

                    FormCard.FormButtonDelegate {
                        visible: !balanceRow.modelData.matches
                        icon.name: "view-financial-account"
                        text: page.i18nContext.i18nc("@action:button", "Check against my bank")
                        description: page.i18nContext.i18nc("@info", "Compares the two for that day and explains the difference. The starting balance is corrected only if you say so.")
                        onClicked: {
                            ActionLog.taken("ImportDonePage_checkAgainstBank")
                            page.checkAgainstBankClicked(page.accountId, balanceRow.modelData.checkAmount, balanceRow.modelData.checkDate)
                        }
                    }
                }
            }
        }

        FormCard.FormSectionText {
            textFormat: Text.PlainText
            visible: page.balances.length === 0
            text: page.view.fromBank
                ? page.i18nContext.i18nc("@info", "Your bank didn't say what the account holds. To make sure the figures agree, open the account's page and use \"Check against my bank\".")
                : page.i18nContext.i18nc("@info", "The file didn't say what the account held afterwards. To make sure the figures agree, open the account's page and use \"Check against my bank\".")
        }
    }
}
