// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Shows why a form wasn't accepted. The Rust side only returns a code and
// its parameters as text; every message is written here, where KI18n's tools
// extract it (D-034). Each says what happened and what to do next
// (AGENTS.md §6).

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Kirigami.InlineMessage {
    id: message

    // What addAccount() or addTransaction() returned: empty, or the code and
    // its parameters.
    property var result: ({})
    // Whether the form is about a category rather than an account.
    property bool forCategory: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    type: Kirigami.MessageType.Error
    visible: result.code !== undefined
    text: visible ? withPart(result, messageFor(result)) : ""

    // A message about one Part of a transaction names its number (M3.6).
    function withPart(result: var, message: string): string {
        return result.part === undefined
            ? message
            : i18nContext.i18nc("@info %1 is the number of a part of a transaction, %2 a message about it", "Part %1: %2", result.part, message);
    }

    function messageFor(result: var): string {
        switch (result.code) {
        case "input.empty-amount":
            return i18nContext.i18nc("@info", "Type an amount.");
        case "input.malformed-amount":
            return i18nContext.i18nc("@info %1 is an example amount", "This isn't an amount. Type it like this: %1", result.example);
        case "input.too-many-decimal-places":
            return i18nContext.i18ncp("@info", "Amounts here have at most %1 decimal place. Leave out the extra ones; nothing is rounded for you.", "Amounts here have at most %1 decimal places. Leave out the extra ones; nothing is rounded for you.", parseInt(result.max));
        case "input.amount-too-large":
            return i18nContext.i18nc("@info", "This amount is too large. Check it for extra digits.");
        case "input.amount-not-positive":
            return i18nContext.i18nc("@info", "Type an amount above zero. Whether you paid or received it, you choose above.");
        case "input.negative-debt":
            return i18nContext.i18nc("@info", "Type what you owe without a minus sign.");
        case "input.malformed-date":
            return i18nContext.i18nc("@info %1 is an example date", "This isn't a date. Type it like this: %1", result.example);
        case "input.no-such-entry":
            return i18nContext.i18nc("@info", "This choice isn't there anymore. Choose again.");
        case "input.date-in-future":
            return i18nContext.i18nc("@info", "This day hasn't come yet. Type today or a day in the past.");
        case "input.unknown-currency":
            return i18nContext.i18nc("@info", "KassenSturz doesn't know this currency. Choose another one.");
        case "input.no-such-parent":
            return i18nContext.i18nc("@info %1 is the name of a category", "The default categories can't be written: %1 isn't among them. Please report this, so it can be fixed.", result.parent);
        case "input.not-editable-here":
            return i18nContext.i18nc("@info", "This transaction can't be changed on this page. Go back to the list and open it again.");
        case "input.no-parts":
            return i18nContext.i18nc("@info", "Add at least one part, so the transaction says what it was for.");
        case "input.too-many-open-parts":
            return i18nContext.i18nc("@info", "Only one part can be left without an amount; KassenSturz works that one out. Type the amounts of the others.");
        case "input.parts-dont-add-up":
            return result.missing !== undefined
                ? i18nContext.i18nc("@info %1, %2 and %3 are amounts of money", "The parts add up to %1, but the transaction is %2. Sort the remaining %3 into a part, or leave one part's amount empty and KassenSturz works it out.", result.parts, result.total, result.missing)
                : i18nContext.i18nc("@info %1, %2 and %3 are amounts of money", "The parts add up to %1, but the transaction is only %2. Take %3 out of the parts, or leave one part's amount empty and KassenSturz works it out.", result.parts, result.total, result.over);
        case "input.foreign-amount-needed":
            return i18nContext.i18nc("@info %1 is a currency code", "This part counts in %1, and your book has no exchange rate for it yet. Type what the part was in %1, e.g. from the receipt.", result.currency);
        case "input.foreign-sign-mismatch":
            return i18nContext.i18nc("@info", "The two amounts of this part point different ways. Give both the same sign.");
        case "input.whole-book":
            return i18nContext.i18nc("@info", "This file holds a whole book, e.g. from KMyMoney, not a statement of one account, so it can't be imported into an account. A new book can be started from it instead.");
        case "input.unknown-format":
            return i18nContext.i18nc("@info", "KassenSturz doesn't recognize this file. It reads statements as CAMT.053 (.xml), MT940 (.sta), OFX (.ofx) or CSV (.csv, .txt) files; download one of these from your online banking.");
        case "input.no-statements":
            return i18nContext.i18nc("@info", "The file holds no statement. Check that it is the right file, or download it again.");
        case "input.not-a-local-file":
            return i18nContext.i18nc("@info", "Choose a file on this computer.");
        case "import.io":
            return i18nContext.i18nc("@info", "The file couldn't be read. It may have been moved or removed, or you may not have permission to read it. Choose it again.");
        case "import.file-too-large":
            return i18nContext.i18nc("@info", "This file is too large to be a bank statement KassenSturz can read safely. Check that it is the right file. Nothing was imported.");
        case "import.too-deeply-nested":
        case "import.text-too-long":
            return i18nContext.i18nc("@info", "The file isn't built like a bank file; it may be damaged. Download it again. Nothing was imported.");
        case "import.too-many-entries":
            return i18nContext.i18nc("@info %1 is a number", "The file has more than %1 entries. Download a shorter period and import each part. Nothing was imported.", result.max);
        case "import.malformed":
            return result.line !== undefined
                ? i18nContext.i18nc("@info %1 is a line number", "The file isn't in the format chosen, or it is damaged (line %1). Check the choices, or download the file again. Nothing was imported.", result.line)
                : i18nContext.i18nc("@info", "The file isn't in the format chosen, or it is damaged. Check the choices, or download the file again. Nothing was imported.");
        case "import.bad-amount":
            return i18nContext.i18nc("@info %1 is text from the file", "\"%1\" should be an amount, but KassenSturz can't read it as one. Check the decimal mark and the amount columns. Nothing was imported.", result.found);
        case "import.bad-date":
            return i18nContext.i18nc("@info %1 is text from the file", "\"%1\" should be a date, but KassenSturz can't read it in the order of day, month and year chosen. Check that order. Nothing was imported.", result.found);
        case "import.wrong-encoding":
            return i18nContext.i18nc("@info", "The file has a character that doesn't exist in the text encoding chosen. Choose another encoding, or let KassenSturz tell it from the file. Nothing was imported.");
        case "import.column-not-found":
            return i18nContext.i18nc("@info %1 is the name of a column", "The file has no column called \"%1\". Your bank may have changed its file: check which column holds what. Nothing was imported.", result.column);
        case "import.column-not-unique":
            return i18nContext.i18nc("@info %1 is the name of a column", "The file has more than one column called \"%1\", so KassenSturz can't tell which one is meant. Nothing was imported.", result.column);
        case "import.bad-settings":
            return i18nContext.i18nc("@info", "The choices are incomplete or contradict each other, e.g. no column for the booking date or the amount, or a column name without a header line. Check them and try again.");
        case "import.too-many-decimal-places":
            return i18nContext.i18nc("@info %1 is text from the file, %2 a currency code", "The amount \"%1\" has more decimal places than %2 has. KassenSturz never rounds what a bank delivered, so nothing was imported. Check the decimal mark.", result.found, result.currency);
        case "import.amount-too-large":
            return i18nContext.i18nc("@info %1 is text from the file", "The amount \"%1\" is too large for KassenSturz to work with. The file may be damaged. Nothing was imported.", result.found);
        case "import.unknown-currency":
            return i18nContext.i18nc("@info %1 is a currency code", "The file uses the currency %1, which KassenSturz doesn't know. Nothing was imported.", result.currency);
        case "import.currency-mismatch":
            return i18nContext.i18nc("@info %1 and %2 are currency codes", "The file has an amount in %2 in a statement for an account in %1, so its entries can't be added up. Nothing was imported.", result.expected, result.found);
        case "import.account-currency-differs":
            return i18nContext.i18nc("@info %1 is a currency code, %2 the name of an account, %3 a currency code", "The statement is in %1, but %2 counts in %3. Import it into an account in %1. Nothing was imported.", result.statement_currency, result.account, result.account_currency);
        case "import.replaced-currency-differs":
            return i18nContext.i18nc("@info %1 and %2 are currency codes, %3 the name of an account, %4 a currency code", "The statement is in %1, which %2 replaced, but %3 counts in %4. Import it into an account in %2. Nothing was imported.", result.statement_currency, result.successor, result.account, result.account_currency);
        case "import.successor-needed":
        case "import.successor-not-current":
            return i18nContext.i18nc("@info %1 is a currency code", "The file uses %1, a currency KassenSturz doesn't know, most likely one that another currency replaced long ago. Such files can't be imported yet. Nothing was imported.", result.currency);
        case "import.decimal-places-differ":
            return i18nContext.i18nc("@info %1 is a currency code, %2 and %3 are numbers", "The file counts %1 with %2 decimal places, your book with %3, so its amounts can't be read safely. Nothing was imported.", result.currency, result.file, result.book);
        case "bank-check.cannot-correct":
            return i18nContext.i18nc("@info", "The starting balance is dated after this day, so it can't be corrected from here. Check a day on or after the starting balance.");
        case "store.empty-account-name":
            return forCategory
                ? i18nContext.i18nc("@info", "Give the category a name.")
                : i18nContext.i18nc("@info", "Give the account a name.");
        case "store.duplicate-account-name":
            return i18nContext.i18nc("@info %1 is the name of an account or category", "%1 already has this name. Choose another one, so you can tell them apart.", result.existing);
        case "store.account-not-empty":
            return i18nContext.i18nc("@info %1 is the name of an account, %2 an amount of money", "%1 still has %2 in it. Move the money to another account first, then close it.", result.account, result.balance);
        case "store.unknown-account":
            return i18nContext.i18nc("@info", "This account or category isn't in the book anymore. It may have just been removed.");
        case "store.parent-kind-mismatch":
            return i18nContext.i18nc("@info", "Categories for money you receive go inside categories for money you receive, and the same for spending. Choose another category.");
        case "store.parent-currency-mismatch":
            return i18nContext.i18nc("@info %1 is the name of a category, %2 and %3 are currency codes", "%1 counts in %2, this category in %3. Choose a category in the same currency.", result.parent, result.parent_currency, result.currency);
        case "store.category-inside-itself":
            return i18nContext.i18nc("@info %1 is the name of a category", "%1 can't go inside itself or one of the categories inside it. Choose another place.", result.account);
        case "store.unknown-transaction":
            return i18nContext.i18nc("@info", "This transaction isn't in the book anymore. It may have just been removed.");
        case "store.imported-split-changed":
            return i18nContext.i18nc("@info", "This entry came from your bank or a file, so its amount stays as it was delivered. You can change the rest.");
        case "store.account-closed":
            return i18nContext.i18nc("@info %1 is the name of an account", "%1 is closed. Open it again to add or change its transactions.", result.account);
        case "store.corrupt.history-mismatch":
            return i18nContext.i18nc("@info", "The book doesn't match its history anymore, probably because another program changed it. Nothing was changed. To go on with Undo, open Settings and use \"Clean up history\".");
        default:
            return i18nContext.i18nc("@info %1 is an error code", "Something went wrong (%1). Please report this, so it can be fixed.", result.code);
        }
    }
}
