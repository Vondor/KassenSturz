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
    text: visible ? Plain.plain(withTarget(result, withCondition(result, withPart(result, messageFor(result))))) : ""

    // A message about one Part of a transaction names its number (M3.6).
    function withPart(result: var, message: string): string {
        return result.part === undefined
            ? message
            : i18nContext.i18nc("@info %1 is the number of a part of a transaction, %2 a message about it", "Part %1: %2", result.part, message);
    }

    // Why a bill of the receipt inbox didn't move (US7, #333), from the
    // reason word of `bill.move-failed` (docs/ERROR_CODES.md).
    function billReason(reason: string): string {
        switch (reason) {
        case "changed":
            return i18nContext.i18nc("@info", "The file changed after it was read.");
        case "not-found":
            return i18nContext.i18nc("@info", "The file isn't there any more.");
        case "exists":
            return i18nContext.i18nc("@info", "A file of that name is already there.");
        case "no-space":
            return i18nContext.i18nc("@info", "The drive is full.");
        case "no-permission":
            return i18nContext.i18nc("@info", "KassenSturz may not write there.");
        case "inbox-gone":
            return i18nContext.i18nc("@info", "The inbox folder isn't there any more.");
        case "archive-gone":
            return i18nContext.i18nc("@info", "The archive folder isn't there any more.");
        case "verify":
            return i18nContext.i18nc("@info", "The copy didn't match the original, so it was removed.");
        default:
            return i18nContext.i18nc("@info", "The file couldn't be copied.");
        }
    }

    // Why a planned payment can't be kept as it is (D-769).
    function scheduleInvalid(reason: string): string {
        switch (reason) {
        case "name":
            return i18nContext.i18nc("@info", "Give the planned payment a name of 1 to 60 characters on one line, such as Rent.");
        case "amount-zero":
            return i18nContext.i18nc("@info", "Type an amount other than zero.");
        case "every-range":
            return i18nContext.i18nc("@info", "Choose how often it comes: every 1 to 52 weeks, months or years.");
        case "end-before-start":
            return i18nContext.i18nc("@info", "The end is before the first payment. Choose a later end, or none.");
        case "tolerance-range":
            return i18nContext.i18nc("@info", "The amount may vary by less than the amount itself. Type a smaller difference, or none.");
        case "party":
            return i18nContext.i18nc("@info", "The name or IBAN it is paid to or received from may have at most 140 characters.");
        case "booked-amount":
            return i18nContext.i18nc("@info", "To book it, type an amount other than zero.");
        default:
            return i18nContext.i18nc("@info", "This planned payment can't be kept as it is. Check the form.");
        }
    }

    // Why a chart description can't be read, at the place it names. The
    // place is the user's own text and is shown as it is (D-034).
    function specInvalid(result: var): string {
        const at = result.path ?? "";
        const place = at === "" ? "" : " " + i18nContext.i18nc("@info %1 is a place in a chart description, as text from it", "(at %1)", at);
        switch (result.reason) {
        case "not-json":
            return i18nContext.i18nc("@info", "This isn't a chart description: it isn't valid JSON text. Check the brackets, quotes and commas.") + place;
        case "not-utf8":
            return i18nContext.i18nc("@info", "This file isn't plain UTF-8 text, so it can't be read. Save it as UTF-8 and open it again.");
        case "unreadable":
            return i18nContext.i18nc("@info", "This file can't be read. Check that it is an ordinary file you may open.");
        case "not-an-object":
            return i18nContext.i18nc("@info", "A chart description is one object: it starts with { and ends with }.");
        case "duplicate-key":
            return i18nContext.i18nc("@info", "The same name is written twice in one place, so it is not clear which one counts. Keep only one of them.") + place;
        case "bad-schema":
            return i18nContext.i18nc("@info", "The $schema line must be an address that starts with https://vega.github.io/schema/vega-lite/. It is only checked, never opened.") + place;
        case "missing":
            return i18nContext.i18nc("@info", "Something this chart needs is missing. Add it.") + place;
        case "wrong-type":
            return i18nContext.i18nc("@info", "A value here is of the wrong kind, for example text where a number is needed. Change it.") + place;
        case "bad-choice":
            return i18nContext.i18nc("@info", "This word is not one KassenSturz knows here. Use one of the words the Vega-Lite documentation lists for it.") + place;
        case "unknown-dataset":
            return i18nContext.i18nc("@info", "The data name is not one of the data a chart of your book can show. Use one of the names in the description's help.") + place;
        case "unknown-field":
            return i18nContext.i18nc("@info", "The data you named has no such field. Use one of its fields.") + place;
        case "type-mismatch":
            return i18nContext.i18nc("@info", "The type does not fit this field: amounts and percents are quantitative, days are temporal, names and yes or no are nominal. Change the type.") + place;
        case "bad-channel-for-mark":
            return i18nContext.i18nc("@info", "This mark does not take this encoding. Take it out or use another mark.") + place;
        case "layers-differ":
            return i18nContext.i18nc("@info", "The layers of this chart do not fit together: they must use the same kind of axis. Change one of them.") + place;
        case "bad-aggregate":
            return i18nContext.i18nc("@info", "This way of adding up does not fit the field: percents can only be counted, or the smallest or largest taken. Change it.") + place;
        case "fraction-number":
            return i18nContext.i18nc("@info", "A number with decimals must be written in quotes, such as \"12.50\", so it is read exactly. Put it in quotes.") + place;
        case "bad-number":
            return i18nContext.i18nc("@info", "This is not a number KassenSturz can read here. Change it.") + place;
        case "bad-date":
            return i18nContext.i18nc("@info", "A date is written like \"2026-01-31\" (year, month, day). Change it.") + place;
        case "bad-text":
            return i18nContext.i18nc("@info", "This text has characters that can't be used here. Change it.") + place;
        case "nothing-to-draw":
            return i18nContext.i18nc("@info", "There is nothing to draw: a chart needs data from your book, a mark and what the mark shows. Add what is missing.") + place;
        }
        return i18nContext.i18nc("@info", "This chart description can't be used. Check it against the Vega-Lite documentation.") + place;
    }

    // A message about the target of a progress bar says so (M6b.7).
    function withTarget(result: var, message: string): string {
        return result.target === undefined
            ? message
            : i18nContext.i18nc("@info %1 is a message about the target formula of a progress bar", "Target: %1", message);
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
        case "input.outside-test":
            return i18nContext.i18nc("@info", "KassenSturz runs in test mode, which never reads your own books. Choose a file in the test folder, or try an example book. The file wasn't read.");
        case "input.not-kmymoney":
            return i18nContext.i18nc("@info", "This isn't a KMyMoney file. Reports can be added from a KMyMoney book, saved as a .kmy file or as its SQLite database.");
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

    // The mistakes of a formula's spelling, by what was expected (D-292).
    function formulaSyntax(result: var): string {
        switch (result.reason) {
        case "empty":
            return i18nContext.i18nc("@info", "The formula is empty. Type one, for example =Pets - Travel.");
        case "expected-value":
            return i18nContext.i18nc("@info %1 is a character position", "A value is missing at character %1. Put a number, the name of a category or a function there.", result.position);
        case "missing-close":
            return i18nContext.i18nc("@info %1 is a character position", "A closing bracket ) is missing at character %1. Every ( needs a ) that closes it.", result.position);
        case "missing-open":
            return i18nContext.i18nc("@info %1 is a character position", "There is a closing bracket ) at character %1 without a ( before it. Take it out, or add the ( that belongs to it.", result.position);
        case "unexpected":
            return i18nContext.i18nc("@info %1 is a character position", "Something is out of place at character %1. Take it out, or put + - * or / before it.", result.position);
        case "two-comparisons":
            return i18nContext.i18nc("@info %1 is a character position", "A formula can compare only once, and there is a second comparison at character %1. Use AND or OR to put tests together.", result.position);
        case "wrong-separator":
            return i18nContext.i18nc("@info %1 is a character position, %2 a separator", "Put %2 between the parts of a call, at character %1, as in SUM(Pets%2 Travel).", result.position, result.separator);
        case "bad-number":
            return i18nContext.i18nc("@info %1 is a character position, %2 a decimal mark", "The number at character %1 isn't written the way numbers are here. Use digits and one decimal mark (%2), and no separators between thousands.", result.position, result.decimal);
        case "bad-character":
            return i18nContext.i18nc("@info %1 is a character position", "Character %1 isn't one formulas use. Take it out, or put a name with spaces or special characters in single quotes: 'Eating out'.", result.position);
        case "missing-quote":
            return i18nContext.i18nc("@info %1 is a character position", "A quote at character %1 is never closed. Add the quote that closes it.", result.position);
        case "unknown-kind":
            return i18nContext.i18nc("@info %1 is a character position", "The word before the dot at character %1 isn't one formulas know. Write Income or Spending before the dot, for example Spending.Pets, or put a name with a dot in single quotes.", result.position);
        case "bad-period":
            return i18nContext.i18nc("@info %1 is a character position", "The time range at character %1 isn't written the way ranges are. Use words in double quotes, such as @\"last month\", or dates, such as @2026-03 or @2026-01..2026-03.", result.position);
        }
        return i18nContext.i18nc("@info %1 is a character position", "This isn't written the way formulas are, at character %1. Change it at that place.", result.position);
    }

    // A period word with a number outside its range names the range
    // (#759 point 3, D-740).
    function formulaPeriodCount(result: var): string {
        switch (result.unit) {
        case "days":
            return i18nContext.i18nc("@info %1 is a character position, %2 the largest number", "The number of days in the period at character %1 must be a whole number from 1 to %2, as in @\"last 7 days\".", result.position, result.most);
        case "weeks":
            return i18nContext.i18nc("@info %1 is a character position, %2 the largest number", "The number of weeks in the period at character %1 must be a whole number from 1 to %2, as in @\"last 4 weeks\".", result.position, result.most);
        case "years":
            return i18nContext.i18nc("@info %1 is a character position, %2 the largest number", "The number of years in the period at character %1 must be a whole number from 1 to %2, as in @\"last 2 years\".", result.position, result.most);
        }
        return i18nContext.i18nc("@info %1 is a character position, %2 the largest number", "The number of months in the period at character %1 must be a whole number from 1 to %2, as in @\"last 6 months\" or @\"2 months ago\".", result.position, result.most);
    }

    function formulaArguments(result: var): string {
        switch (result.reason) {
        case "too-few":
            return i18nContext.i18nc("@info %1 is a function name, %2 a character position", "%1 was given too few parts (character %2). Add the ones that are missing.", result.function, result.position);
        case "too-many":
            return i18nContext.i18nc("@info %1 is a function name, %2 a character position", "%1 was given too many parts (character %2). Take some out.", result.function, result.position);
        case "names-only":
            return i18nContext.i18nc("@info %1 is a function name, %2 a character position", "%1 counts only categories and groups (character %2). Give it their names, not figures.", result.function, result.position);
        case "places":
            return i18nContext.i18nc("@info %1 is a function name, %2 a character position", "The decimal places of %1 (character %2) must be a whole number from -9 to 10.", result.function, result.position);
        }
        return i18nContext.i18nc("@info %1 is a function name, %2 a character position", "%1 was given the wrong parts (character %2). Check them.", result.function, result.position);
    }

    function formulaKinds(result: var): string {
        switch (result.reason) {
        case "amount-times-amount":
            return i18nContext.i18nc("@info %1 is a character position", "Two amounts can't be multiplied (character %1). Multiply an amount by a plain number instead.", result.position);
        case "number-over-amount":
            return i18nContext.i18nc("@info %1 is a character position", "A number can't be divided by an amount (character %1). Divide the amount by the number, or use PERCENT_OF(part; whole) for a share.", result.position);
        case "yes-no-in-arithmetic":
            return i18nContext.i18nc("@info %1 is a character position", "A yes-or-no test can't be calculated with (character %1). Put it inside IF(test; yes; no).", result.position);
        case "yes-no-result":
            return i18nContext.i18nc("@info %1 is a character position", "The formula ends in a yes-or-no test, not in a figure (character %1). Put it inside IF(test; yes; no).", result.position);
        case "needs-yes-no":
            return i18nContext.i18nc("@info %1 is a character position", "IF, AND, OR and NOT need a comparison such as Pets > 100 (character %1).", result.position);
        case "if-kinds-differ":
            return i18nContext.i18nc("@info %1 is a character position", "The two answers of IF (character %1) must both be figures or both be yes or no.", result.position);
        case "ratio-kinds":
            return i18nContext.i18nc("@info %1 is a character position", "PERCENT_OF and CHANGE need two amounts or two numbers (character %1), not one of each.", result.position);
        }
        return i18nContext.i18nc("@info %1 is a character position", "These parts of the formula don't go together (character %1). Change it at that place.", result.position);
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
        case "store.amount-overflow":
            return i18nContext.i18nc("@info", "This would make the amounts in your book add up to more than KassenSturz can work with. Nothing was changed. Check the amount for extra digits.");
        case "input.amount-not-positive":
            return i18nContext.i18nc("@info", "Type an amount above zero. Whether you paid or received it, you choose above.");
        case "input.negative-debt":
            return i18nContext.i18nc("@info", "Type what you owe without a minus sign.");
        case "input.malformed-date":
            return i18nContext.i18nc("@info %1 is an example date", "This isn't a date. Type it like this: %1", result.example);
        case "input.no-such-entry":
            return i18nContext.i18nc("@info", "This choice isn't there anymore. Choose again.");
        // Joining two sides of a move of money (D-691).
        case "transfer.not-a-side":
            return i18nContext.i18nc("@info", "This transaction can't be one side of a move between your accounts: it is broken down into several parts, or has an amount in another currency.");
        case "transfer.part-holds-more":
            return result.what === "number"
                ? i18nContext.i18nc("@info", "Both entries have a different number of their own, and joining would lose one. Change the parts so only one number is left, then join them.")
                : i18nContext.i18nc("@info", "Both entries have a different note of their own, and joining would lose one. Change the parts so only one note is left, then join them.");
        case "input.date-in-future":
            return i18nContext.i18nc("@info", "This day hasn't come yet. Type today or a day in the past.");
        case "input.unknown-currency":
            return i18nContext.i18nc("@info", "KassenSturz doesn't know this currency. Choose another one.");
        case "input.no-such-parent":
            return i18nContext.i18nc("@info %1 is the name of a category", "The default categories can't be written: %1 isn't among them. Please report this, so it can be fixed.", result.parent);
        case "input.not-editable-here":
            return i18nContext.i18nc("@info", "This transaction can't be changed on this page. Go back to the list and open it again.");
        case "input.note-invalid":
            return i18nContext.i18nc("@info %1 is a number of characters", "A note can have up to %1 characters. Please shorten it, or remove any invisible special characters pasted into it.", result.max);
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
        case "import.encoding-contradiction":
            return i18nContext.i18nc("@info %1 is a text encoding such as UTF-8", "The file says it is written in %1, but its text isn't. It may have been saved again by another program. Download it again, or save it in UTF-8. Nothing was imported.", result.declared);
        case "import.unsupported-encoding":
            return i18nContext.i18nc("@info %1 is a text encoding such as UTF-32", "The file is written in the text encoding %1, which KassenSturz doesn't read. Download it again in UTF-8, or save it in UTF-8 or UTF-16. Nothing was imported.", result.found);
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
        case "store.view-name-taken":
            return i18nContext.i18nc("@info %1 is the name of a saved view", "You already have a saved view called %1. Choose another name, so you can tell them apart.", result.name);
        case "store.view-document-invalid":
        case "store.view-document-too-big":
        case "report.document-invalid":
        case "report.document-too-big":
            return i18nContext.i18nc("@info", "This view couldn't be saved or opened because it isn't one KassenSturz can use. Nothing was changed. Make it again in the Statistic Builder, and choose a short name.");
        case "report.dashboard-file-invalid":
            return i18nContext.i18nc("@info %1 is a place in the file, as text from the file", "This dashboard file can't be read (at %1), so nothing of it was added. Use a file saved by KassenSturz with Save to a file, or ask the person who gave it to you to save it again.", result.path === "" ? "-" : result.path);
        case "report.dashboard-file-too-big":
            return i18nContext.i18nc("@info", "This file is too large to be a dashboard file, so it was not read. Check that you chose the right file.");
        case "report.unsupported-dashboard-file-version":
            return i18nContext.i18nc("@info %1 is a version number", "This dashboard file was made by a newer KassenSturz (file version %1), so this one can't read it. Update KassenSturz, or ask for a file saved by this version.", result.version);
        case "report.filter-invalid":
            // A component's own tags (US5.2).
            if (result.dimension === "tags") {
                if (result.reason === "too-many") {
                    return i18nContext.i18nc("@info", "A filter can name at most 200 tags. Take some out of the filter, or choose none to count all money.");
                }
                if (result.reason === "repeated") {
                    return i18nContext.i18nc("@info", "The filter names the same tag twice. Take it out once, then try again.");
                }
                return i18nContext.i18nc("@info", "The tags in the filter include something that is not a tag. Take it out of the filter, or choose the tags again.");
            }
            if (result.reason === "too-many") {
                return result.dimension === "accounts"
                    ? i18nContext.i18nc("@info", "A filter can name at most 200 accounts. Take some out of the filter, or choose none to count all of them.")
                    : i18nContext.i18nc("@info", "A filter can name at most 200 categories. Take some out of the filter, or choose none to count all of them.");
            }
            if (result.reason === "repeated") {
                return result.dimension === "accounts"
                    ? i18nContext.i18nc("@info", "The filter names the same account twice. Take it out once, then try again.")
                    : i18nContext.i18nc("@info", "The filter names the same category twice. Take it out once, then try again.");
            }
            if (result.reason === "not-an-account") {
                return i18nContext.i18nc("@info", "The accounts in the filter include something that is not one of your accounts. Take it out of the filter, or choose the accounts again.");
            }
            return i18nContext.i18nc("@info", "The categories in the filter include something that is not a category or a group. Take it out of the filter, or choose the categories again.");
        case "report.unsupported-document-version":
            return i18nContext.i18nc("@info", "This view was made by a newer KassenSturz, so this one can't open it. Update KassenSturz to open it. You can still rename or delete it.");
        case "store.empty-name":
            return i18nContext.i18nc("@info", "Give it a name.");
        case "store.unknown-group":
            return i18nContext.i18nc("@info", "This group isn't in the book anymore. It may have just been removed.");
        case "store.group-name-taken":
            return i18nContext.i18nc("@info %1 is a name", "There is already a group or a category called %1. Choose another name, so a chart or a formula can tell them apart.", result.name);
        case "store.group-name-too-long":
            return i18nContext.i18nc("@info %1 is a number", "A group's name can have at most %1 characters. Choose a shorter name.", result.max_chars);
        case "store.group-wrong-kind":
            return i18nContext.i18nc("@info %1 is the name of a category or group", "%1 doesn't fit this group: a group of money spent takes only categories and groups of money spent, a group of money received only those of money received. Take it out, or put it in a group of its kind.", result.name);
        case "store.group-loop":
            return i18nContext.i18nc("@info %1 is the name of a group", "%1 would end up inside itself. Take out the group that contains it.", result.name);
        case "store.group-too-deep":
            return i18nContext.i18nc("@info %1 is a number", "Groups can go at most %1 levels deep: a group, a group inside it, and so on. Put the categories into this group directly instead.", result.max_levels);
        case "store.too-many-groups":
            return i18nContext.i18nc("@info %1 is a number", "A book keeps at most %1 groups. Remove a group you no longer need, then try again.", result.max);
        case "store.too-many-group-members":
            return i18nContext.i18nc("@info %1 is a number", "A group can have at most %1 members. Put some of them into another group, and then that group into this one.", result.max);
        case "report.grid-edit-refused":
            switch (result.reason) {
            case "unknown-component":
                return i18nContext.i18nc("@info", "That part of the layout isn't there anymore, so nothing was changed. It may have just been removed.");
            case "not-editable":
                return i18nContext.i18nc("@info", "This part of the layout was made by a newer KassenSturz, so this one can move or remove it but not change it. Nothing was changed.");
            case "too-many-components":
                return i18nContext.i18nc("@info", "The page can hold at most 100 widgets, so nothing was added. Remove one you no longer need, then try again.");
            case "no-room":
                return i18nContext.i18nc("@info", "There is no room for that: the page reaches down at most 1,000 rows. Nothing was changed. Move something up or remove it, then try again.");
            }
            return i18nContext.i18nc("@info", "That change to the layout couldn't be made, so nothing was changed. Try something else.");
        case "report.groups-overlap":
            return i18nContext.i18nc("@info %1 is the name of a category, %2 and %3 are the names of two groups", "%1 is in %2 and in %3; a chart that shows parts of a whole can't count it twice. Take it out of one of them, or show them as bars or lines.", result.category, result.first, result.second);
        case "report.formula-syntax":
            return formulaSyntax(result);
        case "report.formula-period-count":
            return formulaPeriodCount(result);
        case "report.formula-unknown-name":
            return i18nContext.i18nc("@info %1 is a name, %2 a character position", "%1 is not a category or one of My groups (character %2). Check the spelling, or put a name with spaces in single quotes: 'Eating out'.", result.name, result.position);
        case "report.formula-ambiguous-name":
            if (result.by === "kind")
                return i18nContext.i18nc("@info %1 is a name, %2 a list of names", "%1 fits more than one: %2. Write the one you mean as listed. Income. in front is money that came in, Spending. money that went out. Or rename a group or category of the same name.", result.name, result.paths);
            return i18nContext.i18nc("@info %1 is a name, %2 a list of names", "%1 fits more than one: %2. Write the one you mean with its path, as listed, or rename a group or category of the same name.", result.name, result.paths);
        case "report.formula-unknown-function":
            return i18nContext.i18nc("@info %1 is a function name, %2 a character position", "There is no function called %1 (character %2). The list of functions beside the formula shows the ones there are.", result.name, result.position);
        case "report.formula-arguments":
            return formulaArguments(result);
        case "report.formula-kinds":
            return formulaKinds(result);
        case "report.formula-mixed-currencies":
            return i18nContext.i18nc("@info %1 and %3 are names, %2 and %4 currency codes", "%1 is in %2 and %3 in %4; a formula works in one currency. Leave one of them out.", result.first, result.first_currency, result.second, result.second_currency);
        case "report.formula-no-value":
            return i18nContext.i18nc("@info %1 is a character position", "This divides by zero at character %1, so it has no value. Use IFERROR to show something else then.", result.position);
        case "report.formula-too-large":
            return i18nContext.i18nc("@info", "A figure in this formula is too large, or has too many decimal places, to be worked out exactly. Use smaller numbers.");
        case "report.formula-target":
            switch (result.reason) {
            case "zero":
                return i18nContext.i18nc("@info", "The target is zero, so there is nothing to measure the bar against. Change the target.");
            case "too-large":
                return i18nContext.i18nc("@info", "The bar can't be worked out from figures this large or small. Use a target nearer to the formula.");
            }
            return i18nContext.i18nc("@info", "The target and the formula must be the same kind of figure: amounts in one currency, percents or plain numbers. Change the target.");
        case "report.formula-too-big":
            return i18nContext.i18nc("@info", "This formula is bigger than KassenSturz works out: at most 1,000 characters, 500 parts, 16 brackets or calls inside each other, 30 parts in a call, 64 names and 8 different periods. Make it smaller.");
        case "report.spec-unsupported":
            return i18nContext.i18ncp("@info %1 is how many parts", "%1 part of this chart description isn't supported here, so the chart isn't drawn. Select the chart and open its properties to see which part, then choose to draw it without it, or change the description.", "%1 parts of this chart description aren't supported here, so the chart isn't drawn. Select the chart and open its properties to see which parts, then choose to draw it without them, or change the description.", Number(result.count));
        case "report.spec-too-big":
            switch (result.limit) {
            case "bytes":
                return i18nContext.i18nc("@info", "This chart description is longer than 32 KiB, so it can't be used. Make it shorter.");
            case "depth":
                return i18nContext.i18nc("@info", "This chart description has more than 10 brackets inside each other, so it can't be used. Flatten it.");
            case "layers":
                return i18nContext.i18nc("@info", "This chart description has more than 4 layers, so it can't be used. Take some layers out.");
            case "encodings":
                return i18nContext.i18nc("@info", "A layer of this chart description shows more than 8 things (x, y, color and so on), so it can't be used. Take some out.");
            case "filters":
                return i18nContext.i18nc("@info", "This chart description has more than 8 filters, so it can't be used. Take some out.");
            case "values":
                return i18nContext.i18nc("@info", "A list in this chart description has more than 50 values, or more than 8 colors, so it can't be used. Make the list shorter.");
            case "label":
                return i18nContext.i18nc("@info", "A text in this chart description is longer than 200 characters, so it can't be used. Make it shorter.");
            case "rows":
                return i18nContext.i18nc("@info", "This chart would use more than 10,000 rows of your book. Narrow it down with a filter in the description or choose a shorter period.");
            case "series":
                return i18nContext.i18nc("@info", "This chart would have more than 50 series or slices. Narrow it down with a filter in the description.");
            }
            return i18nContext.i18nc("@info", "This chart description is bigger than KassenSturz reads, so it can't be used. Make it smaller.");
        case "report.spec-invalid":
            return specInvalid(result);
        case "store.empty-rule-name":
            return i18nContext.i18nc("@info", "Give the Rule a name.");
        case "store.duplicate-rule-name":
            return i18nContext.i18nc("@info %1 is the name of a Rule", "%1 already has this name here. Choose another one, so you can tell them apart.", result.existing);
        case "store.unknown-rule":
            return i18nContext.i18nc("@info", "This Rule isn't in the book anymore. It may have just been removed.");
        case "store.tag-name":
            return result.reason === "line-break"
                ? i18nContext.i18nc("@info", "A tag's name must fit on one line. Take out the line break.")
                : i18nContext.i18nc("@info %1 is a number", "A tag's name can have at most %1 characters. Choose a shorter name.", result.max_chars);
        case "store.tag-too-many":
            switch (result.limit) {
            case "part":
                return i18nContext.i18nc("@info %1 is a number", "A part can have at most %1 tags of its own. Take a tag away, then try again.", result.max);
            case "book":
                return i18nContext.i18nc("@info %1 is a number", "A book keeps at most %1 tags. Remove or merge tags you no longer need, then try again.", result.max);
            default:
                return i18nContext.i18nc("@info %1 is a number", "A transaction can have at most %1 tags. Take a tag away, then try again.", result.max);
            }
        case "store.unknown-tag":
            return i18nContext.i18nc("@info", "One of the tags isn't in the book anymore. It may have just been removed. Take it away, then try again.");
        case "store.duplicate-tag-name":
            return i18nContext.i18nc("@info %1 is the name of a tag", "%1 already has this name. Choose another one, or merge the two tags.", result.existing);
        case "store.tag-merged-into-itself":
            return i18nContext.i18nc("@info", "A tag can't be merged into itself. Choose another tag to merge it into.");
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
        case "store.budget-unknown":
            return i18nContext.i18nc("@info", "This budget isn't in the book anymore. It may have just been deleted.");
        case "store.budget-not-spending":
            return i18nContext.i18nc("@info %1 is the name of a category", "%1 isn't a category for money you spend, so it can't have a budget. Choose a category for spending.", result.category);
        case "store.budget-overlap":
            return i18nContext.i18nc("@info %1 is the name of a category", "%1 already has a budget for some of these days that counts the same accounts. A category's money is in one budget at a time: change or stop the other one first, start this one after it ends, or count other accounts in it.", result.category);
        case "store.budget-choice":
            switch (result.reason) {
            case "no-category":
                return i18nContext.i18nc("@info", "A budget needs a category for money you spend. Choose one.");
            case "currencies":
                return i18nContext.i18nc("@info", "The categories of one budget need the same currency. Choose categories in one currency, or set a budget for each currency.");
            case "not-own-account":
                return i18nContext.i18nc("@info", "A budget can only count money paid from your own accounts. Choose among your accounts.");
            default:
                return i18nContext.i18nc("@info", "Each category and each account can be chosen once. Choose them again.");
            }
        case "store.budget-amount":
            switch (result.reason) {
            case "too-large":
                return i18nContext.i18nc("@info", "This amount is larger than KassenSturz can add up over the periods of a budget. Enter a smaller amount.");
            case "currency":
                return i18nContext.i18nc("@info", "The amount isn't in the currency of this category. Enter an amount in the category's currency.");
            default:
                return i18nContext.i18nc("@info", "A budget needs an amount of more than zero. Enter what you plan to spend.");
            }
        case "store.budget-period":
            switch (result.reason) {
            case "not-period-start":
                return i18nContext.i18nc("@info", "A budget for every month, quarter or year, and a change to it, starts on the first day of one. Choose one of the periods in the list.");
            case "not-period-end":
                return i18nContext.i18nc("@info", "A budget for every month, quarter or year ends on the last day of one. Choose one of the periods in the list.");
            case "ends-before-starts":
                return i18nContext.i18nc("@info", "The last day can't be before the first day. Choose other days.");
            case "no-last-day":
                return i18nContext.i18nc("@info", "A budget for some days needs a last day. Enter the last day.");
            case "too-long":
                return i18nContext.i18nc("@info", "A budget for some days can cover at most 100 years. Choose other days.");
            case "carry-over-on-range":
                return i18nContext.i18nc("@info", "A budget for some days has no next period to carry anything into. Switch off carrying over.");
            case "amount-before-start":
            case "amount-after-end":
                return i18nContext.i18nc("@info", "A change can't apply before the budget starts or after it ends. Choose one of the periods in the list.");
            case "before-current-period":
                return i18nContext.i18nc("@info", "What a budget counts changes from this period on, so the results of earlier periods stay. Choose this period or a later one.");
            default:
                return i18nContext.i18nc("@info", "These days don't fit the budget. Choose other days.");
            }
        case "store.budget-name":
            if (result.reason === "line-break") {
                return i18nContext.i18nc("@info", "A budget's name fits on one line. Remove the line break.");
            }
            return i18nContext.i18nc("@info %1 is a number", "A budget's name can have at most %1 characters. Choose a shorter name.", result.max_chars);
        case "store.budget-too-many":
            switch (result.limit) {
            case "amounts":
                return i18nContext.i18nc("@info %1 is a number", "A budget can keep at most %1 changes of its amount. Change the amount from an earlier period on instead.", result.max);
            case "categories":
                return i18nContext.i18nc("@info %1 is a number", "A budget can count at most %1 categories. Choose fewer, or a category that has the others inside it.", result.max);
            case "accounts":
                return i18nContext.i18nc("@info %1 is a number", "A budget can count at most %1 accounts. Choose fewer, or tick none to count every account.", result.max);
            case "counts":
                return i18nContext.i18nc("@info %1 is a number", "A budget can keep at most %1 changes of what it counts. Stop it and set a new budget instead.", result.max);
            }
            return i18nContext.i18nc("@info %1 is a number", "A book can keep at most %1 budgets. Delete a budget you no longer need.", result.max);
        case "store.category-in-use":
            switch (result.used_by) {
            case "subcategories":
                return i18nContext.i18nc("@info %1 is the name of a category", "%1 has categories inside it, so it can't be deleted. Move or delete those first.", result.category);
            case "rules":
                return i18nContext.i18nc("@info %1 is the name of a category", "A Rule books to %1 or looks at it, so it can't be deleted. Change or delete the Rule first.", result.category);
            case "budgets":
                return i18nContext.i18nc("@info %1 is the name of a category", "%1 has a budget, so it can't be deleted. Delete its budget first.", result.category);
            case "schedules":
                return i18nContext.i18nc("@info %1 is the name of a category", "A planned payment uses %1, so it can't be deleted. Give the planned payment another category first.", result.category);
            default:
                return i18nContext.i18nc("@info %1 is the name of a category", "%1 has transactions, so it can't be deleted. Put them into another category first.", result.category);
            }
        case "store.category-inside-itself":
            return i18nContext.i18nc("@info %1 is the name of a category", "%1 can't go inside itself or one of the categories inside it. Choose another place.", result.account);
        case "store.unknown-transaction":
            return i18nContext.i18nc("@info", "This transaction isn't in the book anymore. It may have just been removed.");
        case "store.imported-split-changed":
            return i18nContext.i18nc("@info", "This transaction came from your bank or a file, so its amount stays as it was delivered. You can change the rest.");
        case "store.delivered-kept":
            return i18nContext.i18nc("@info", "KassenSturz keeps what your bank or a file delivered exactly as it came, so this change wasn't made and nothing was lost. This is a mistake in KassenSturz or in another program that changed the book; please report it.");
        case "store.account-closed":
            return i18nContext.i18nc("@info %1 is the name of an account", "%1 is closed. Open it again to add or change its transactions.", result.account);
        case "store.nothing-moves":
            return i18nContext.i18nc("@info %1 is the name of an account or category", "Every part of this transaction is in %1, so no money moves. Choose another account or category for one of them.", result.account);
        case "store.zero-amount":
            return i18nContext.i18nc("@info", "This transaction is zero, so no money moves. Enter its amount.");
        // Planned payments (US7.1, D-769).
        case "input.plan-every":
            return i18nContext.i18nc("@info", "Choose how often it comes: every 1 to 52 weeks, months or years.");
        case "store.schedule-unknown":
            return i18nContext.i18nc("@info", "This planned payment isn't in the book anymore. It may have just been removed.");
        case "store.schedule-invalid":
            return scheduleInvalid(result.reason ?? "");
        case "store.schedule-account-closed":
            return i18nContext.i18nc("@info %1 is the name of an account", "%1 is closed, so it gets no new planned payment and no new entry. Open it again first; its planned payments go on then.", result.account);
        case "store.schedule-currency-differs":
            return i18nContext.i18nc("@info %1 is the name of an account, %2 a currency code", "%1 doesn't count in %2. A planned payment is in its account's currency.", result.account, result.currency);
        case "store.schedule-category":
            return i18nContext.i18nc("@info %1 is the name of a category", "%1 isn't a category in the planned payment's currency. Choose another one, or none.", result.category);
        case "store.schedule-too-many":
            return i18nContext.i18nc("@info %1 is a number", "A book holds at most %1 planned payments. Remove one you don't need anymore first.", result.max);
        case "store.schedule-occurrence-unknown":
            return i18nContext.i18nc("@info", "This planned payment isn't due on that day. It may have just been changed. Look at the list again.");
        case "store.schedule-occurrence-decided":
            return i18nContext.i18nc("@info", "This payment was matched or skipped already, so nothing was changed. To take a match back, use Undo.");
        case "store.schedule-entry-taken":
            return i18nContext.i18nc("@info", "This entry is matched to a planned payment already. Each entry pays one planned payment.");
        case "store.schedule-entry-not-fitting":
            return i18nContext.i18nc("@info", "This entry can't pay this planned payment: it must be in the plan's account, with the money going the same way, and not a move between your own accounts.");
        case "store.corrupt.history-mismatch":
            return i18nContext.i18nc("@info", "The book doesn't match its history anymore, probably because another program changed it. Nothing was changed. To go on with Undo, open Settings and use \"Clean up history\".");
        // The receipt inbox (US7, #333): a bill can't move back or again,
        // so nothing was changed (docs/RECEIPT_INBOX.md §7).
        case "bill.undo-blocked":
            return i18nContext.i18nc("@info %1 is a list of files, %2 why the first can't be moved", "Nothing was undone: these bills can't be moved back into your inbox: %1. %2 Put them back, or undo anyway and leave them where they are.", result.files, billReason(result.reason));
        case "bill.redo-blocked":
            return i18nContext.i18nc("@info %1 is a list of files, %2 why the first can't be moved", "Nothing was made again: these bills can't be filed again: %1. %2 Put them back into your inbox, then try again.", result.files, billReason(result.reason));
        case "input.no-accounts-chosen":
            return i18nContext.i18nc("@info", "Tick at least one account.");
        case "input.range-reversed":
            return i18nContext.i18nc("@info", "The last day is before the first. Swap them.");
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
            return i18nContext.i18nc("@info", "This copy of KassenSturz isn't registered with the German banks' FinTS system, so it can't connect to a bank. The official releases are registered. Import a file from your bank instead.");
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
        case "bank.securities-account":
            return i18nContext.i18nc("@info", "This account is linked to a securities account (depot) at your bank. KassenSturz can't get securities accounts yet, so nothing was fetched for it. Unlink it on the Get new transactions page if you don't want it tried again.");
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
        case "bank.tan-method-needed":
            if (result.methods !== undefined) {
                return i18nContext.i18nc("@info", "Your bank wants an approval method chosen first. Your PIN was not called wrong. Pick your method in the list of the advanced settings, type your PIN again and try again.");
            }
            return result.offered !== undefined
                ? i18nContext.i18nc("@info %1 is a list of method numbers", "Your bank wants an approval method chosen first. Your PIN was not called wrong. Open the advanced settings, enter one of these numbers: %1, and try again.", result.offered)
                : i18nContext.i18nc("@info", "Your bank wants an approval method chosen first. Your PIN was not called wrong. Open the advanced settings, enter your bank's approval method number, and try again.");
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
        case "bank.keepassxc-answer":
            return i18nContext.i18nc("@info", "KeePassXC answered, but KassenSturz couldn't read its answer, so nothing was saved, read or removed. Try again; if it keeps happening, report it, and type the PIN each time for now.");
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
        case "input.bank-address-userinfo":
            return i18nContext.i18nc("@info", "A FinTS address has no name and @ in front of the server, as in https://name@server. Check it against what your bank gives.");
        case "input.bank-address-private":
            return i18nContext.i18nc("@info", "That address points to a computer in your own network, not to a bank. Check it against what your bank gives.");
        case "input.address-not-confirmed":
            return i18nContext.i18nc("@info", "This address isn't the one in KassenSturz's bank lists. Tick the box under the address if your bank gave you exactly this address.");
        case "input.bad-tan-method":
            return i18nContext.i18nc("@info", "A TAN method is a number from 900 to 999, for example 912. Leave the field empty to let your bank choose.");
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
