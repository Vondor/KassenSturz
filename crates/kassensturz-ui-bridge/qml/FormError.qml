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
