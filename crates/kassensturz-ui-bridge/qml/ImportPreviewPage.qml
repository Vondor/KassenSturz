// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What an import would do, before it does it (M3.10, D-092): how many
// entries are new, which look like transactions the account already has
// (the user decides about each), which are skipped, and the bank's balances
// against KassenSturz's (D-029, D-111). Nothing changes until "Import".
// Every amount and date arrives as text (D-065); the sentences are written
// here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property StatementImport statementImport
    readonly property var view: statementImport.view ?? ({})
    readonly property var entries: view.entries ?? []
    readonly property var balances: view.balances ?? []
    readonly property int toImport: view.imported ?? 0

    // The import was made; the summary comes next.
    signal imported()
    // The user gave up on the file.
    signal cancelled()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title %1 is the name of an account", "Import into %1", view.accountName ?? "")
    header: FormError {
        id: error
    }

    actions: [
        Kirigami.Action {
            icon.name: "document-import"
            enabled: page.toImport > 0
            text: enabled
                ? page.i18nContext.i18ncp("@action:button", "Import %1 transaction", "Import %1 transactions", page.toImport)
                : page.i18nContext.i18nc("@action:button nothing to import", "Nothing new to import")
            onTriggered: {
                ActionLog.taken("ImportPreviewPage_importNow")
                page.importNow()
            }
        },
        Kirigami.Action {
            icon.name: "dialog-cancel"
            text: page.i18nContext.i18nc("@action:button", "Don't import")
            onTriggered: {
                ActionLog.taken("ImportPreviewPage_donTImport")
                page.statementImport.cancel();
                page.cancelled();
            }
        }
    ]

    function importNow() {
        error.result = statementImport.importNow();
        if (error.result.code === undefined) {
            imported();
        }
    }

    function decide(key: string, on: bool) {
        error.result = statementImport.setImportAnyway(key, on);
    }

    // What the bank or KassenSturz says: a debt reads "you owe".
    function says(amount: string, owes: bool): string {
        return owes ? i18nContext.i18nc("@label %1 is an amount of money", "you owe %1", amount) : amount;
    }

    function balanceText(balance: var): string {
        return balance.which === "opening"
            ? i18nContext.i18nc("@info %1 is a date, %2 what the bank says the account held", "Before these transactions, on %1, your bank says: %2", balance.date, says(balance.bank, balance.bankOwes))
            : i18nContext.i18nc("@info %1 is a date, %2 what the bank says the account held", "After them, on %1, your bank says: %2", balance.date, says(balance.bank, balance.bankOwes));
    }

    function balanceDescription(balance: var): string {
        if (balance.matches) {
            return balance.stated
                ? i18nContext.i18nc("@info", "KassenSturz agrees, and will remember this as what your bank said.")
                : i18nContext.i18nc("@info", "KassenSturz agrees.");
        }
        return balance.bookHigher
            ? i18nContext.i18nc("@info %1 is what KassenSturz has, %2 the difference", "KassenSturz has %1 for that day, %2 more. The summary after the import shows where the difference can come from.", says(balance.book, balance.bookOwes), balance.difference)
            : i18nContext.i18nc("@info %1 is what KassenSturz has, %2 the difference", "KassenSturz has %1 for that day, %2 less. The summary after the import shows where the difference can come from.", says(balance.book, balance.bookOwes), balance.difference);
    }

    function outcomeText(entry: var): string {
        switch (entry.outcome) {
        case "imported":
            if (entry.category !== "") {
                return i18nContext.i18nc("@info %1 is the name of a category", "New · a Rule sorts it into %1", entry.category);
            }
            return entry.similar.length > 0
                ? i18nContext.i18nc("@info", "New · imported although it looks like one you have")
                : i18nContext.i18nc("@info", "New");
        case "similar":
            return i18nContext.i18nc("@info", "Maybe in your book already · left out unless you tick it");
        case "already-in-book":
            return i18nContext.i18nc("@info", "Already in your book · skipped");
        case "twice-in-file":
            return i18nContext.i18nc("@info", "Twice in the file · counted once");
        case "pending":
            return i18nContext.i18nc("@info", "Not booked by your bank yet · left out");
        default:
            return entry.outcome;
        }
    }

    // A ListView, not a Repeater: a statement can hold thousands of
    // entries, and a Repeater builds a row for every one of them before the
    // first frame (M4.W); the ListView builds the visible rows only.
    ListView {
        id: entryList

        clip: true
        model: page.entries

        header: ColumnLayout {
            width: entryList.width
            spacing: 0

            FormCard.FormHeader {
                title: page.i18nContext.i18nc("@title:group", "What the import does")
            }

            FormCard.FormCard {
                FormCard.FormTextDelegate {
                    descriptionItem.textFormat: Text.PlainText
                    icon.name: "document-import"
                    text: page.view.fromBank
                        ? page.i18nContext.i18nc("@info", "From your bank")
                        : page.i18nContext.i18nc("@info %1 is a file name", "From %1", page.view.fileName ?? "")
                    textItem.textFormat: Text.PlainText
                    description: page.toImport > 0
                        ? page.i18nContext.i18ncp("@info", "%1 new transaction goes into the account.", "%1 new transactions go into the account.", page.toImport)
                        : page.i18nContext.i18nc("@info", "Nothing new: every transaction is already in your book or waits for your decision below.")
                }

                FormCard.FormDelegateSeparator {
                    visible: (page.view.rules ?? 0) > 0
                }

                FormCard.FormTextDelegate {
                    textItem.textFormat: Text.PlainText
                    visible: (page.view.rules ?? 0) > 0
                    icon.name: "view-filter"
                    text: page.i18nContext.i18ncp("@info", "Your Rules sort %1 of them into a category.", "Your Rules sort %1 of them into categories.", page.view.rules ?? 0)
                }

                FormCard.FormDelegateSeparator {
                    visible: (page.view.toDecide ?? 0) > 0
                }

                FormCard.FormTextDelegate {
                    textItem.textFormat: Text.PlainText
                    visible: (page.view.toDecide ?? 0) > 0
                    icon.name: "dialog-question"
                    text: page.i18nContext.i18ncp("@info", "%1 transaction looks like one already in your book.", "%1 transactions look like transactions already in your book.", page.view.toDecide ?? 0)
                    description: page.i18nContext.i18nc("@info", "Maybe you typed it in before the statement came. It is left out unless you tick \"Import it anyway\" below.")
                }

                FormCard.FormDelegateSeparator {
                    visible: (page.view.duplicates ?? 0) > 0
                }

                FormCard.FormTextDelegate {
                    textItem.textFormat: Text.PlainText
                    visible: (page.view.duplicates ?? 0) > 0
                    icon.name: "edit-copy"
                    text: page.i18nContext.i18ncp("@info", "%1 transaction is already in your book and is skipped.", "%1 transactions are already in your book and are skipped.", page.view.duplicates ?? 0)
                }

                FormCard.FormDelegateSeparator {
                    visible: (page.view.pending ?? 0) > 0
                }

                FormCard.FormTextDelegate {
                    textItem.textFormat: Text.PlainText
                    visible: (page.view.pending ?? 0) > 0
                    icon.name: "chronometer"
                    text: page.i18nContext.i18ncp("@info", "%1 transaction isn't booked by your bank yet and is left out.", "%1 transactions aren't booked by your bank yet and are left out.", page.view.pending ?? 0)
                    description: page.i18nContext.i18nc("@info", "It comes with a later statement, once the bank has booked it.")
                }

                FormCard.FormDelegateSeparator {
                    visible: (page.view.transfers ?? 0) > 0
                }

                FormCard.FormTextDelegate {
                    textItem.textFormat: Text.PlainText
                    visible: (page.view.transfers ?? 0) > 0
                    icon.name: "edit-link"
                    text: page.i18nContext.i18ncp("@info", "%1 transaction looks like money moved between your own accounts.", "%1 transactions look like money moved between your own accounts.", page.view.transfers ?? 0)
                    description: page.i18nContext.i18nc("@info", "KassenSturz asks you about each one after the import.")
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
                    }
                }
            }

            FormCard.FormHeader {
                title: page.i18nContext.i18ncp("@title:group", "%1 transaction in the file", "%1 transactions in the file", page.entries.length)
            }
        }

        delegate: ColumnLayout {
            id: entryRow
            width: ListView.view.width

            required property int index
            required property var modelData
            readonly property bool waiting: modelData.outcome === "similar" || (modelData.outcome === "imported" && modelData.similar.length > 0)

            Layout.fillWidth: true
            spacing: 0

            FormCard.FormDelegateSeparator {
                visible: entryRow.index > 0
            }

            FormCard.AbstractFormDelegate {
                Layout.fillWidth: true
                background: null
                // Text from a bank file could contain anything.
                Accessible.name: entryRow.modelData.date + " " + entryRow.modelData.otherParty + " " + entryRow.modelData.exactAmount

                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.largeSpacing

                        Controls.Label {
                            textFormat: Text.PlainText
                            text: entryRow.modelData.date
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            text: entryRow.modelData.otherParty !== ""
                                ? entryRow.modelData.otherParty
                                : page.i18nContext.i18nc("@info an entry that names nobody", "Without a name")
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            opacity: entryRow.modelData.otherParty !== "" ? 1 : 0.6
                        }

                        AmountLabel {
                            textFormat: Text.PlainText
                            text: entryRow.modelData.amount
                            exactText: entryRow.modelData.exactAmount
                            horizontalAlignment: Text.AlignRight
                            color: entryRow.modelData.incoming ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor
                            opacity: entryRow.modelData.outcome === "imported" ? 1 : 0.6
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: entryRow.modelData.purpose !== ""
                        text: entryRow.modelData.purpose
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        font: Kirigami.Theme.smallFont
                        opacity: 0.8
                    }

                    Controls.Label {
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        text: page.outcomeText(entryRow.modelData)
                        wrapMode: Text.Wrap
                        font: Kirigami.Theme.smallFont
                        color: entryRow.modelData.outcome === "similar" ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.textColor
                    }

                    Repeater {
                        model: entryRow.modelData.similar

                        delegate: Controls.Label {
                            required property var modelData

                            Layout.fillWidth: true
                            Layout.leftMargin: Kirigami.Units.largeSpacing
                            text: modelData.category === ""
                                ? page.i18nContext.i18nc("@info a transaction in the book: %1 is a date, %2 a name, %3 an amount", "In your book: %1 · %2 · %3", modelData.date, modelData.otherParty, modelData.exactAmount)
                                : page.i18nContext.i18nc("@info a transaction in the book: %1 is a date, %2 a name, %3 an amount, %4 a category", "In your book: %1 · %2 · %3 · %4", modelData.date, modelData.otherParty, modelData.exactAmount, modelData.category)
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            font: Kirigami.Theme.smallFont
                        }
                    }

                    Controls.CheckBox {
                        visible: entryRow.waiting
                        text: page.i18nContext.i18nc("@option:check import an entry that looks like one already in the book", "Import it anyway, it is a different transaction")
                        checked: entryRow.modelData.importAnyway
                        onToggled: {
                            ActionLog.taken("ImportPreviewPage_importAnywayDifferent")
                            page.decide(entryRow.modelData.key, checked)
                        }
                    }
                }
            }
        }

        footer: ColumnLayout {
            width: entryList.width
            spacing: 0

            FormCard.FormSectionText {
                text: page.i18nContext.i18nc("@info", "Nothing has changed yet. The import is one change, so Undo takes all of it back at once.")
            }
        }
    }
}
