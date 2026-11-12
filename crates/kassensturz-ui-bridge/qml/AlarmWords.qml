// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The words of the warnings tile (US4.9, D-721 to D-725): the sentence of a
// rule that applies, its short summary in the tile's settings, and the help
// of each reason. The Rust side (`BookAlarms`) sends fixed words and amounts
// as text; the sentences are written here, where KI18n's tools extract them
// (D-034). They say what is, never what to do with the money (AGENTS.md
// §3.1 rule 11).

import QtQuick
import org.kde.ki18n

QtObject {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The reasons, in the order the editor offers them.
    readonly property var reasons: ["budget_over", "budget_under", "account_below", "account_reserve"]

    function reasonName(reason: string): string {
        switch (reason) {
        case "budget_over":
            return i18nContext.i18nc("@option:radio a kind of warning", "A budget is used up past a share");
        case "budget_under":
            return i18nContext.i18nc("@option:radio a kind of warning", "A budget was used less than a share");
        case "account_below":
            return i18nContext.i18nc("@option:radio a kind of warning", "An account falls under an amount");
        default:
            return i18nContext.i18nc("@option:radio a kind of warning", "Keep a reserve on an account");
        }
    }

    function reasonHelp(reason: string): string {
        switch (reason) {
        case "budget_over":
            return i18nContext.i18nc("@info", "For example, tell me when Food has used more than 80 % this month.");
        case "budget_under":
            return i18nContext.i18nc("@info", "For example, tell me when less than 80 % of Savings was put away last month. It looks at the budget's last finished period.");
        case "account_below":
            return i18nContext.i18nc("@info", "For example, tell me when the shared account is under 500.");
        default:
            return i18nContext.i18nc("@info", "The account should always hold, say, one month of a budget. It is checked at the month's lowest point.");
        }
    }

    // The budget's period: this one, or the last finished one.
    function period(every: string, ended: bool): string {
        switch (every) {
        case "month":
            return ended ? i18nContext.i18nc("@info part of a sentence: when", "last month") : i18nContext.i18nc("@info part of a sentence: when", "this month");
        case "salary-month":
            return ended ? i18nContext.i18nc("@info part of a sentence: when; a salary month runs from payday to the day before the next one", "last salary month") : i18nContext.i18nc("@info part of a sentence: when; a salary month runs from payday to the day before the next one", "this salary month");
        case "quarter":
            return ended ? i18nContext.i18nc("@info part of a sentence: when", "last quarter") : i18nContext.i18nc("@info part of a sentence: when", "this quarter");
        case "year":
            return ended ? i18nContext.i18nc("@info part of a sentence: when", "last year") : i18nContext.i18nc("@info part of a sentence: when", "this year");
        default:
            return i18nContext.i18nc("@info part of a sentence: when; a budget for one stretch of days", "in its period");
        }
    }

    // How many of a budget's periods a reserve is, e.g. "2 months".
    function periods(every: string, times: int): string {
        switch (every) {
        case "month":
        case "salary-month":
            return i18nContext.i18ncp("@info part of a sentence", "%1 month", "%1 months", times);
        case "quarter":
            return i18nContext.i18ncp("@info part of a sentence", "%1 quarter", "%1 quarters", times);
        case "year":
            return i18nContext.i18ncp("@info part of a sentence", "%1 year", "%1 years", times);
        default:
            return i18nContext.i18ncp("@info part of a sentence: how many times a budget's amount", "%1 time", "%1 times", times);
        }
    }

    // The sentence of a row that is a line of its own: triggered, gone or
    // not checkable. Empty for a row that is fine.
    function sentence(row: var): string {
        if (row.state === "gone") {
            return row.reason === "account_below" || (row.reason === "account_reserve" && row.budgetName !== "")
                ? i18nContext.i18nc("@info", "A warning is about an account that is no longer in the book. Change or remove it in this tile's settings.")
                : i18nContext.i18nc("@info", "A warning is about a budget that no longer exists. Change or remove it in this tile's settings.");
        }
        if (row.state === "cant-check") {
            return row.why === "currencies"
                ? i18nContext.i18nc("@info %1 is an account, %2 a budget", "Can't check the reserve of %1: the account and the budget %2 are in different currencies, and they are never converted for a reserve.", row.accountName, row.budgetName)
                : i18nContext.i18nc("@info %1 is an account", "Can't check the warning for %1: its amount doesn't fit the account's currency. Change it in this tile's settings.", row.accountName);
        }
        if (row.state !== "triggered") {
            return "";
        }
        const when = period(row.every, row.ended);
        switch (row.reason) {
        case "budget_over":
            if (row.nothingToSpend) {
                return i18nContext.i18nc("@info %1 is a budget, %2 when, %3 an amount", "%1 has nothing left to spend %2, and %3 were spent.", row.budgetName, when, row.spent);
            }
            return row.share === 100
                ? i18nContext.i18nc("@info %1 is a budget, %2 an amount, %3 when", "%1 is over by %2 %3.", row.budgetName, row.over, when)
                : i18nContext.i18nc("@info %1 is a budget, %2 and %3 amounts, %4 when, %5 a whole percent", "%1 has used %2 of %3 %4, more than %5 %.", row.budgetName, row.spent, row.available, when, row.share);
        case "budget_under":
            return i18nContext.i18nc("@info %1 is a budget, %2 and %3 amounts, %4 when, %5 a whole percent", "%1 used only %2 of %3 %4, less than %5 %.", row.budgetName, row.spent, row.available, when, row.share);
        case "account_below":
            return row.lowest
                ? i18nContext.i18nc("@info %1 is an account, %2 and %3 amounts, %4 a date", "%1 went down to %2 this month (on %4), under your limit of %3.", row.accountName, row.balance, row.limit, row.balanceDay)
                : i18nContext.i18nc("@info %1 is an account, %2 and %3 amounts", "%1 holds %2, under your limit of %3.", row.accountName, row.balance, row.limit);
        default:
            return row.lowest
                ? i18nContext.i18nc("@info %1 is an account, %2 and %3 amounts, %4 how many periods, e.g. 1 month, %5 a budget, %6 a date", "%1 went down to %2 this month (on %6), under its reserve of %3: %4 of %5.", row.accountName, row.balance, row.limit, periods(row.every, row.times), row.budgetName, row.balanceDay)
                : i18nContext.i18nc("@info %1 is an account, %2 and %3 amounts, %4 how many periods, e.g. 1 month, %5 a budget", "%1 holds %2, under its reserve of %3: %4 of %5.", row.accountName, row.balance, row.limit, periods(row.every, row.times), row.budgetName);
        }
    }

    // The short summary of a rule in the tile's settings.
    function summary(row: var): string {
        const account = row.accountName !== "" ? row.accountName : i18nContext.i18nc("@info an account that was removed", "(a removed account)");
        const budget = row.budgetName !== "" ? row.budgetName : i18nContext.i18nc("@info a budget that was removed", "(a removed budget)");
        switch (row.reason) {
        case "budget_over":
            return i18nContext.i18nc("@item %1 is a budget, %2 a whole percent", "%1 used more than %2 %", budget, row.share);
        case "budget_under":
            return i18nContext.i18nc("@item %1 is a budget, %2 a whole percent", "%1 used less than %2 % (last finished period)", budget, row.share);
        case "account_below":
            return row.lowest
                ? i18nContext.i18nc("@item %1 is an account, %2 an amount", "%1 under %2 at its lowest point this month", account, row.amountInput)
                : i18nContext.i18nc("@item %1 is an account, %2 an amount", "%1 under %2", account, row.amountInput);
        default:
            return row.lowest
                ? i18nContext.i18nc("@item %1 is an account, %2 how many periods, e.g. 1 month, %3 a budget", "Reserve on %1: %2 of %3, at its lowest point this month", account, periods(row.every || "month", row.times), budget)
                : i18nContext.i18nc("@item %1 is an account, %2 how many periods, e.g. 1 month, %3 a budget", "Reserve on %1: %2 of %3", account, periods(row.every || "month", row.times), budget);
        }
    }
}
