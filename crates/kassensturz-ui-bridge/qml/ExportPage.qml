// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Saving a copy of the user's data in a new file (M3.12, D-099, D-100,
// D-125): the transactions of chosen accounts as CSV for a spreadsheet, or
// the whole book, which a new book can be started from again. The file is
// always a new one; KassenSturz never overwrites a file. Every text comes
// from Rust (D-065); the sentences are written here (D-034).

import QtQuick
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property DataExport dataExport
    // The folder the open book is in, where the file dialogs start.
    required property url folderUrl

    // The accounts to choose from and which are ticked, by ID.
    property var accounts: []
    property var ticked: ({})
    // The choices for the CSV file.
    property var choices: ({})
    // What the last save did: an error, or the file written.
    property var result: ({})
    // Whether the last file written was the whole book.
    property bool wholeBook: false
    // Whether the extra choices for the CSV file are shown.
    property bool moreChoices: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Save a copy of my data")
    header: FormError {
        result: page.result
    }

    Component.onCompleted: {
        accounts = dataExport.accounts();
        const all = {};
        for (const account of accounts) {
            all[account.id] = true;
        }
        ticked = all;
        choices = dataExport.csvChoices();
        separator.currentIndex = separator.indexOfValue(choices.separator);
        decimalMark.currentIndex = decimalMark.indexOfValue(choices.decimalMark);
    }

    // The label of a check box reads markup, and an account's name could
    // contain anything.
    function escaped(name: string): string {
        return name.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }

    function chosenIds(): var {
        return accounts.filter(account => ticked[account.id]).map(account => account.id);
    }

    function startUrl(extension: string): url {
        return folderUrl + "/" + encodeURIComponent(dataExport.suggestedName(extension));
    }

    function saveCsv(file: url) {
        wholeBook = false;
        result = dataExport.saveCsv(file, chosenIds(), choices);
    }

    function saveBook(file: url) {
        wholeBook = true;
        result = dataExport.saveBook(file);
    }

    Dialogs.FileDialog {
        id: csvDialog

        title: page.i18nContext.i18nc("@title:window", "Save the transactions as a spreadsheet file")
        fileMode: Dialogs.FileDialog.SaveFile
        defaultSuffix: "csv"
        nameFilters: [page.i18nContext.i18nc("@item:inlistbox a file type", "Spreadsheet files (*.csv)")]
        onAccepted: {
            ActionLog.taken("ExportPage_saveCsv")
            page.saveCsv(selectedFile)
        }
    }

    Dialogs.FileDialog {
        id: bookDialog

        title: page.i18nContext.i18nc("@title:window", "Save the whole book")
        fileMode: Dialogs.FileDialog.SaveFile
        defaultSuffix: "json"
        nameFilters: [page.i18nContext.i18nc("@item:inlistbox a file type", "KassenSturz book files (*.json)")]
        onAccepted: {
            ActionLog.taken("ExportPage_saveBook")
            page.saveBook(selectedFile)
        }
    }

    FormCard.FormCard {
        visible: page.result.fileName !== undefined

        FormCard.FormTextDelegate {
            icon.name: "dialog-ok"
            text: page.i18nContext.i18nc("@info %1 is a file name", "Saved in %1", page.result.fileName ?? "")
            description: page.wholeBook
                ? page.i18nContext.i18ncp("@info %1 is how many", "The whole book: %1 transaction.", "The whole book: %1 transactions.", page.result.transactions ?? 0)
                : page.i18nContext.i18nc("@info %1 is a number of transactions, %2 a number of accounts", "%1 in %2", page.i18nContext.i18ncp("@info %1 is how many", "%1 transaction", "%1 transactions", page.result.transactions ?? 0), page.i18nContext.i18ncp("@info %1 is how many", "%1 account", "%1 accounts", page.result.accounts ?? 0))
            // A file's name could contain anything.
            textItem.textFormat: Text.PlainText
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "For a spreadsheet")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A file of your transactions that Excel, LibreOffice Calc and other spreadsheets open: one line for each transaction of each account you choose. KassenSturz writes a new file and never overwrites one.")
    }

    FormCard.FormCard {
        Repeater {
            model: page.accounts

            delegate: FormCard.FormCheckDelegate {
                required property var modelData

                text: page.escaped(modelData.name)
                description: modelData.closed
                    ? page.i18nContext.i18nc("@info %1 is a currency code", "%1, closed", modelData.currency)
                    : modelData.currency
                checked: page.ticked[modelData.id] === true
                onToggled: {
                    ActionLog.taken("ExportPage_toggled")
                    const next = Object.assign({}, page.ticked);
                    next[modelData.id] = checked;
                    page.ticked = next;
                }
            }
        }
    }

    FormCard.FormCard {
        FormCard.FormButtonDelegate {
            descriptionItem.textFormat: Text.PlainText
            icon.name: page.moreChoices ? "arrow-up" : "arrow-down"
            text: page.moreChoices
                ? page.i18nContext.i18nc("@action:button", "Fewer choices")
                : page.i18nContext.i18nc("@action:button", "More choices for the file")
            description: page.moreChoices
                ? ""
                : page.i18nContext.i18nc("@info", "How the columns are separated, how decimals are written, and more. The usual choices fit your region.")
            onClicked: {
                ActionLog.taken("ExportPage_moreChoices")
                page.moreChoices = !page.moreChoices
            }
        }

        FormCard.FormDelegateSeparator {
            visible: page.moreChoices
        }

        FormCard.FormComboBoxDelegate {
            id: separator

            visible: page.moreChoices
            text: page.i18nContext.i18nc("@label:listbox", "Columns are separated by")
            textRole: "text"
            valueRole: "value"
            model: [
                { value: "comma", text: page.i18nContext.i18nc("@item:inlistbox", "A comma") },
                { value: "semicolon", text: page.i18nContext.i18nc("@item:inlistbox", "A semicolon (usual where decimals have a comma)") },
                { value: "tab", text: page.i18nContext.i18nc("@item:inlistbox", "A tab") },
            ]
            onActivated: {
                ActionLog.taken("ExportPage_columnsAreSeparated")
                page.choices = Object.assign({}, page.choices, { separator: currentValue })
            }
        }

        FormCard.FormDelegateSeparator {
            visible: page.moreChoices
        }

        FormCard.FormComboBoxDelegate {
            id: decimalMark

            visible: page.moreChoices
            text: page.i18nContext.i18nc("@label:listbox", "Decimals are written with")
            textRole: "text"
            valueRole: "value"
            model: [
                { value: "point", text: page.i18nContext.i18nc("@item:inlistbox", "A point: 1234.50") },
                { value: "comma", text: page.i18nContext.i18nc("@item:inlistbox", "A comma: 1234,50") },
            ]
            onActivated: {
                ActionLog.taken("ExportPage_decimalsAreWritten")
                page.choices = Object.assign({}, page.choices, { decimalMark: currentValue })
            }
        }

        FormCard.FormDelegateSeparator {
            visible: page.moreChoices
        }

        FormCard.FormSwitchDelegate {
            visible: page.moreChoices
            text: page.i18nContext.i18nc("@option:check", "The first line names the columns")
            checked: page.choices.header === true
            onToggled: {
                ActionLog.taken("ExportPage_firstLineNames")
                page.choices = Object.assign({}, page.choices, { header: checked })
            }
        }

        FormCard.FormDelegateSeparator {
            visible: page.moreChoices
        }

        FormCard.FormSwitchDelegate {
            visible: page.moreChoices
            text: page.i18nContext.i18nc("@option:check", "Help Excel show letters like ä and é")
            description: page.i18nContext.i18nc("@info", "Adds a small mark at the start of the file. Turn it on if Excel shows strange characters.")
            checked: page.choices.byteOrderMark === true
            onToggled: {
                ActionLog.taken("ExportPage_helpExcelShow")
                page.choices = Object.assign({}, page.choices, { byteOrderMark: checked })
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-save-as"
            text: page.i18nContext.i18nc("@action:button", "Save as a spreadsheet file…")
            enabled: page.chosenIds().length > 0
            onClicked: {
                ActionLog.taken("ExportPage_saveSpreadsheetFile")
                csvDialog.currentFolder = page.folderUrl;
                csvDialog.currentFile = page.startUrl("csv");
                csvDialog.open();
            }
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "The whole book")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Everything in your book in one file: accounts, categories, transactions, Rules and exchange rates. Keep it as a backup, or use it to start a new book, for example on another computer. The history of changes isn't included, so Undo starts fresh in a book made from it. Other programs can read the file too; it is described in KassenSturz's documentation.")
    }

    FormCard.FormCard {
        FormCard.FormButtonDelegate {
            icon.name: "document-save"
            text: page.i18nContext.i18nc("@action:button", "Save the whole book…")
            onClicked: {
                ActionLog.taken("ExportPage_saveWholeBook")
                bookDialog.currentFolder = page.folderUrl;
                bookDialog.currentFile = page.startUrl("json");
                bookDialog.open();
            }
        }
    }
}
