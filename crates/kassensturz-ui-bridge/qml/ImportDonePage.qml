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

    // The user is finished here.
    signal done()
    // Look into a balance that differs, with the bank's figure and day.
    signal checkAgainstBankClicked(string accountId, string bank, string date)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title %1 is the name of an account", "Imported into %1", view.accountName ?? "")
    header: FormError {
        id: error
    }

    actions: Kirigami.Action {
        icon.name: "dialog-ok"
        text: page.i18nContext.i18nc("@action:button", "Done")
        onTriggered: page.done()
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
                        onClicked: page.checkAgainstBankClicked(page.accountId, balanceRow.modelData.checkAmount, balanceRow.modelData.checkDate)
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
