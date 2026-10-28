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
    // Whether the form starts a book from a file (M3.12), where a file's
    // problems are those of a whole book, not of a bank statement.
    property bool forNewBook: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    type: Kirigami.MessageType.Error
    visible: result.code !== undefined
    // An InlineMessage shows markup as markup: nothing from a file may look
    // like it (D-143).
    text: visible ? Plain.plain(withCondition(result, withPart(result, messageFor(result)))) : ""

    // A message about one Part of a transaction names its number (M3.6).
    function withPart(result: var, message: string): string {
        return result.part === undefined
            ? message
            : i18nContext.i18nc("@info %1 is the number of a part of a transaction, %2 a message about it", "Part %1: %2", result.part, message);
    }

    // A message about something the user typed in one condition of a Rule
    // names its number (M3.11). The book's own messages about a condition
    // (`rule.*`) already do.
    function withCondition(result: var, message: string): string {
        return result.condition === undefined || !result.code.startsWith("input.")
            ? message
            : i18nContext.i18nc("@info %1 is the number of a condition of a Rule, %2 a message about it", "Condition %1: %2", result.condition, message);
    }

    // The messages that differ when the file is a whole book (M3.12): a book
    // from another program, or one KassenSturz saved. Empty for the others.
    function messageForNewBook(result: var): string {
        switch (result.code) {
        case "import.file-too-large":
            return i18nContext.i18nc("@info", "This file is too large to be a book KassenSturz can read safely. Check that it is the right file. Nothing was made.");
        case "import.too-deeply-nested":
        case "import.text-too-long":
            return i18nContext.i18nc("@info", "The file isn't built like a book KassenSturz can read; it may be damaged. Nothing was made.");
        case "import.too-many-entries":
            return i18nContext.i18nc("@info %1 is a number", "The file has more than %1 entries, more than KassenSturz reads safely. Nothing was made.", result.max);
        case "import.malformed":
            return result.line !== undefined
                ? i18nContext.i18nc("@info %1 is a line number", "The file isn't a KMyMoney book KassenSturz can read, or it is damaged (line %1). Save it again in KMyMoney, then choose it again. Nothing was made.", result.line)
                : i18nContext.i18nc("@info", "The file isn't a KMyMoney book KassenSturz can read, or it is damaged. Save it again in KMyMoney, then choose it again. Nothing was made.");
        case "import.bad-amount":
            return i18nContext.i18nc("@info %1 is text from the file", "\"%1\" should be an amount, but KassenSturz can't read it as one. The file may be damaged. Nothing was made.", result.found);
        case "import.bad-date":
            return i18nContext.i18nc("@info %1 is text from the file", "\"%1\" should be a date, but KassenSturz can't read it as one. The file may be damaged. Nothing was made.", result.found);
        case "import.too-many-decimal-places":
            return i18nContext.i18nc("@info %1 is text from the file, %2 a currency code", "The amount \"%1\" has more decimal places than %2 has. KassenSturz never rounds amounts, so nothing was made.", result.found, result.currency);
        case "import.amount-too-large":
            return i18nContext.i18nc("@info %1 is text from the file", "The amount \"%1\" is too large for KassenSturz to work with. The file may be damaged. Nothing was made.", result.found);
        case "import.unknown-currency":
            return i18nContext.i18nc("@info %1 is a currency code", "The file uses the currency %1, which isn't in its list of currencies, so KassenSturz can't tell how many decimal places it has. Nothing was made.", result.currency);
        case "import.successor-needed":
            return i18nContext.i18nc("@info %1 is a currency code", "Say which current currency replaced %1, how many of it one unit of the new currency was worth, and from which day.", result.currency);
        case "import.successor-not-current":
            return i18nContext.i18nc("@info %1 and %2 are currency codes", "%2 isn't a currency in use today, so %1 can't be counted in it. Choose the current currency that replaced %1 in the end.", result.currency, result.successor);
        case "input.is-a-book":
            return i18nContext.i18nc("@info", "This is a KassenSturz book already. To use it, open it in Settings with \"Open another book…\".");
        case "input.is-a-statement":
            return i18nContext.i18nc("@info", "This is a statement from a bank, not a book. To bring it in, open an account's list of transactions and choose \"Import from my bank\".");
        case "input.replacement-missing":
            return i18nContext.i18nc("@info %1 is a currency code", "Choose the currency that replaced %1.", result.currency);
        case "input.empty-rate":
            return i18nContext.i18nc("@info %1 is a currency code", "Type how many %1 one unit of the new currency was worth.", result.currency);
        case "input.empty-date":
            return i18nContext.i18nc("@info %1 is a currency code", "Type the day the change from %1 took effect.", result.currency);
        case "input.malformed-rate":
            return i18nContext.i18nc("@info %1 is a currency code, %2 an example number", "This isn't a rate for %1. Type it like this: %2", result.currency, result.example);
        case "input.rate-too-many-decimal-places":
            return i18nContext.i18nc("@info %1 is a number", "A rate here has at most %1 decimal places. Leave out the extra ones; nothing is rounded for you.", result.max);
        case "input.rate-too-large":
            return i18nContext.i18nc("@info", "This rate is too large. Check it for extra digits.");
        case "input.rate-not-positive":
            return i18nContext.i18nc("@info", "A rate is above zero. Type what one unit of the new currency was worth in the old one.");
        default:
            return "";
        }
    }

    function messageFor(result: var): string {
        if (forNewBook) {
            const text = messageForNewBook(result);
            if (text !== "") {
                return text;
            }
        }
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
        case "rule.no-conditions":
            return i18nContext.i18nc("@info %1 is the name of a Rule", "%1 has no conditions, so it would apply to every transaction. Add at least one condition.", result.rule);
        case "rule.empty-text":
            return i18nContext.i18nc("@info %1 is the number of a condition, %2 the name of a Rule", "Condition %1 of %2 has no text to look for. Type some, or remove the condition.", result.condition, result.rule);
        case "rule.bad-pattern":
            return i18nContext.i18nc("@info %1 is the number of a condition, %2 the name of a Rule", "The pattern in condition %1 of %2 isn't one KassenSturz can use. Check its brackets and special characters, or look for plain text instead.", result.condition, result.rule);
        case "rule.date-range-reversed":
            return i18nContext.i18nc("@info %1 is the number of a condition, %2 the name of a Rule", "In condition %1 of %2, the last day is before the first. Swap them.", result.condition, result.rule);
        case "store.empty-rule-name":
            return i18nContext.i18nc("@info", "Give the Rule a name.");
        case "store.duplicate-rule-name":
            return i18nContext.i18nc("@info %1 is the name of a Rule", "%1 already has this name here. Choose another one, so you can tell them apart.", result.existing);
        case "store.unknown-rule":
            return i18nContext.i18nc("@info", "This Rule isn't in the book anymore. It may have just been removed.");
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
            return i18nContext.i18nc("@info", "This transaction came from your bank or a file, so its amount stays as it was delivered. You can change the rest.");
        case "store.account-closed":
            return i18nContext.i18nc("@info %1 is the name of an account", "%1 is closed. Open it again to add or change its transactions.", result.account);
        case "store.corrupt.history-mismatch":
            return i18nContext.i18nc("@info", "The book doesn't match its history anymore, probably because another program changed it. Nothing was changed. To go on with Undo, open Settings and use \"Clean up history\".");
        case "input.no-accounts-chosen":
            return i18nContext.i18nc("@info", "Tick at least one account.");
        case "export.io":
            switch (result.kind) {
            case "not-found":
                return i18nContext.i18nc("@info", "The folder doesn't exist. Choose another one. Nothing was saved.");
            case "permission-denied":
                return i18nContext.i18nc("@info", "KassenSturz may not write there. Choose a folder you may write to. Nothing was saved.");
            case "is-a-directory":
                return i18nContext.i18nc("@info", "That is a folder. Type a file name after it. Nothing was saved.");
            case "disk-full":
                return i18nContext.i18nc("@info", "The disk is full. Free some space or choose another drive. Nothing was saved.");
            default:
                return i18nContext.i18nc("@info", "The file couldn't be written. Choose another place. Nothing was saved.");
            }
        case "export.already-exists":
            return i18nContext.i18nc("@info %1 is a file name", "There is a file called \"%1\" already. KassenSturz never overwrites a file, so choose another name, or move the old file away first. Nothing was saved.", result.file);
        case "export.bad-settings":
            return i18nContext.i18nc("@info", "A comma can't both separate the columns and be the decimal mark. Choose a semicolon or a tab between the columns, or a point for decimals. Nothing was saved.");
        case "export.currency-mismatch":
        case "export.bad-decimal-places":
        case "export.wrong-kind":
        case "export.mark-without-part":
            return i18nContext.i18nc("@info", "KassenSturz can't write this data exactly, which is a mistake in KassenSturz or in the book. Please report it. Nothing was saved.");
        case "export.unlisted-currency":
            return i18nContext.i18nc("@info %1 is a currency code", "The file uses %1, which isn't in its own list of currencies, so its decimal places aren't known. If KassenSturz saved the file, please report this. Nothing was made.", result.currency);
        case "export.rules-too-deep":
            return i18nContext.i18nc("@info %1 is a number", "Rules sit inside each other more than %1 levels deep. Move some of them up a level and try again.", result.max);
        case "export.too-large":
            return i18nContext.i18nc("@info", "This file is larger than a book saved by KassenSturz can be, so it wasn't read. Check that it is the right file.");
        case "export.not-json":
        case "export.not-a-book":
            return i18nContext.i18nc("@info", "This isn't a file KassenSturz can start a book from. Choose a book from KMyMoney, or a file saved with \"Save the whole book\". Nothing was made.");
        case "export.newer-version":
            return i18nContext.i18nc("@info", "This file was saved by a newer KassenSturz. Update KassenSturz to read it. Nothing was made.");
        case "export.missing-value":
        case "export.unknown-value":
        case "export.bad-value":
        case "export.duplicate":
        case "export.invalid-transaction":
            return i18nContext.i18nc("@info %1 is where in the file the problem is", "The file doesn't read as a book saved by KassenSturz (at %1). It may have been changed by hand, or by another program. Save the book again, or correct the file. Nothing was made.", result.path ?? "");
        case "store.book-not-empty":
            return i18nContext.i18nc("@info", "This book already has something in it. Start a new book from the file instead.");
        case "book.no-app-folder":
            return i18nContext.i18nc("@info", "KassenSturz found no folder to keep a new book in. Check that your user folder can be written to.");
        case "store.already-exists":
            return i18nContext.i18nc("@info %1 is a file name", "There is a book called \"%1\" already. Nothing was made.", result.file);
        case "bank.not-installed":
            return i18nContext.i18nc("@info", "Bank connections need AqBanking, a free program KassenSturz uses to talk to banks, and it isn't installed on this computer. Install it from your distribution, or import a file from your bank instead.");
        case "bank.too-old":
            return i18nContext.i18nc("@info %1 is a program's name, %2 and %3 version numbers", "The installed %1 is version %2; KassenSturz needs %3 or newer. Update it, or import a file from your bank instead.", result.library === "gwenhywfar" ? "Gwenhywfar" : "AqBanking", result.found, result.minimum);
        case "bank.not-registered":
            return i18nContext.i18nc("@info", "This version of KassenSturz isn't registered with the German banks' FinTS system yet, so it can't connect to a bank. Import a file from your bank instead.");
        case "bank.malformed-registration":
            return i18nContext.i18nc("@info", "A FinTS product registration number has exactly 25 letters and digits. Check it against the registration letter and type it again.");
        case "bank.malformed-iban":
            return i18nContext.i18nc("@info", "This isn't an IBAN. It has two letters, two digits and then up to 30 letters or digits, for example DE89 3704 0044 0532 0130 00; spaces don't matter. Check it against what your bank shows.");
        case "bank.unknown-login":
            return i18nContext.i18nc("@info", "This bank login no longer exists in KassenSturz. Set it up again.");
        case "bank.unknown-account":
            return i18nContext.i18nc("@info", "Your bank doesn't list the bank account this account is linked to for this login anymore. Link the account to another bank account, or connect the bank again in Settings.");
        case "bank.no-bank-account":
            return i18nContext.i18nc("@info", "None of your bank logins lists the bank account this account is linked to. Connect the bank again in Settings, or link the account to another bank account.");
        case "bank.pin-needed":
            return i18nContext.i18nc("@info", "Your bank asks for your PIN. Type it to continue.");
        case "bank.pin-rejected":
            return i18nContext.i18nc("@info", "Your bank says the PIN is wrong. KassenSturz has forgotten it, so it won't be sent again. Check it and type it again; after a few wrong PINs a bank blocks the login.");
        case "bank.unreachable":
            return i18nContext.i18nc("@info", "Your bank's server couldn't be reached. Check your internet connection and the bank's address, and try again later.");
        case "bank.refused":
            return result.text !== undefined
                ? i18nContext.i18nc("@info %1 is the bank's own message", "Your bank refused the request. It says: %1. Try again later; if it keeps happening, ask your bank.", result.text)
                : i18nContext.i18nc("@info", "Your bank refused the request. Try again later; if it keeps happening, ask your bank.");
        case "bank.aborted":
            return i18nContext.i18nc("@info", "The bank's question was closed before it was answered, so nothing was done. Start again when you are ready.");
        case "bank.tan-needed":
            return i18nContext.i18nc("@info", "Your bank asks for a TAN, and KassenSturz had no way to ask you for it. Nothing was done. Try again.");
        case "bank.tan-method-unsupported":
            return i18nContext.i18nc("@info", "Your bank shows the TAN as a picture (a flicker code, photoTAN or QR code), which KassenSturz can't show yet. Choose a TAN method at your bank that sends the TAN as text or asks you to approve in its app, then try again.");
        case "bank.untrusted-server":
            return i18nContext.i18nc("@info", "The bank server's security certificate didn't pass every check, so KassenSturz sent nothing to it. Check your internet connection and the bank's address; if it keeps happening, ask your bank.");
        case "bank.needs-setup-dialog":
            return i18nContext.i18nc("@info", "A chip-card login can't be set up in KassenSturz yet. Use a login with PIN and TAN for this bank, or import a file from your bank instead.");
        case "bank.setup-failed":
            return result.text !== undefined
                ? i18nContext.i18nc("@info %1 is the bank software's own message", "The bank login couldn't be set up, so nothing of it was kept. The bank software says: %1. Check the login name and the bank's address and try again.", result.text)
                : i18nContext.i18nc("@info", "The bank login couldn't be set up, so nothing of it was kept. Check the login name and the bank's address and try again.");
        case "bank.folder":
            return i18nContext.i18nc("@info", "The folder KassenSturz keeps its bank settings in couldn't be created or opened. Check that your user folder can be written to.");
        case "bank.busy":
            return i18nContext.i18nc("@info", "KassenSturz is still talking to your bank. Wait for it to finish and try again.");
        case "bank.keepassxc":
            return i18nContext.i18nc("@info", "KeePassXC couldn't be reached, or it said no. Start KeePassXC, open and unlock your database, make sure Browser Integration is on in its settings, allow KassenSturz when KeePassXC asks, and try again. Until then, type the PIN each time.");
        case "bank.keychain":
            return i18nContext.i18nc("@info", "The system keychain couldn't be reached, so the PIN couldn't be saved, read or removed. Make sure the keychain (for example KWallet) is unlocked, or choose to type the PIN each time.");
        case "bank.malformed-amount":
            return i18nContext.i18nc("@info %1 is an amount as the bank sent it", "Your bank sent an amount KassenSturz can't read: %1. Nothing was imported. If it keeps happening, report it with the amount as shown.", result.amount);
        case "bank.inexact-amount":
            return i18nContext.i18nc("@info %1 is an amount as the bank sent it", "Your bank sent an amount with a fraction of the smallest unit (%1), which KassenSturz never rounds. Nothing was imported. If it keeps happening, report it.", result.amount);
        case "bank.amount-too-large":
            return i18nContext.i18nc("@info %1 is an amount as the bank sent it", "Your bank sent an amount too large for KassenSturz to work with: %1. Nothing was imported.", result.amount);
        case "bank.missing-currency":
            return i18nContext.i18nc("@info", "Your bank didn't say which currency the amounts are in. Nothing was imported. Check the account's currency with your bank.");
        case "bank.currency-mismatch":
            return i18nContext.i18nc("@info %1 and %2 are currency codes", "Your bank sent amounts in %2 for an account in %1. Nothing was imported. Check the account's currency with your bank.", result.expected, result.found);
        case "bank.missing-date":
            return i18nContext.i18nc("@info", "Your bank sent a transaction without a date. Nothing was imported. If it keeps happening, report it.");
        case "bank.statement":
            return result.cause !== undefined
                ? i18nContext.i18nc("@info %1 is an error code", "The transactions your bank sent couldn't be used (%1). Nothing was imported. If it keeps happening, report it.", result.cause)
                : i18nContext.i18nc("@info", "The transactions your bank sent couldn't be used. Nothing was imported. If it keeps happening, report it.");
        case "store.not-linked":
            return i18nContext.i18nc("@info %1 is the name of an account", "%1 isn't linked to a bank account yet. Link it first, then get its new transactions.", result.account);
        case "store.bank-account-already-linked":
            return i18nContext.i18nc("@info %1 is the name of an account", "%1 is already linked to this bank account. Choose another bank account.", result.existing);
        case "input.no-fints":
            return i18nContext.i18nc("@info", "Banks outside Germany can't be connected yet. Import the files your bank offers instead.");
        case "input.empty-bank-address":
            return i18nContext.i18nc("@info", "Type your bank's FinTS address. Your bank's website or support can tell you.");
        case "input.bank-address-not-https":
            return i18nContext.i18nc("@info", "A FinTS address starts with https:// and has no spaces, for example %1. Check it against what your bank gives.", "https://fints.example-bank.de/fints");
        case "input.empty-login-name":
            return i18nContext.i18nc("@info", "Type the login name you use for your bank's website.");
        case "input.empty-customer-id":
            return i18nContext.i18nc("@info", "This bank gives its customers a customer ID next to the login name. Type it too; you find it in your bank's letters.");
        case "input.empty-pin":
            return i18nContext.i18nc("@info", "Type your PIN. KassenSturz keeps it only as long as you choose below.");
        default:
            return i18nContext.i18nc("@info %1 is an error code", "Something went wrong (%1). Please report this, so it can be fixed.", result.code);
        }
    }
}
