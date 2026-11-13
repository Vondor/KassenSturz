// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The plain words for the grid editor (M6b.3, D-311). The core sends fixed
// names, never text (D-034); this is where they get their words.

import QtQuick
import org.kde.ki18n
import io.github.vondor.kassensturz

QtObject {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property PeriodRange range: PeriodRange {}

    function typeName(id: string): string {
        switch (id) {
        case "chart.bar":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "Bars");
        case "chart.pie":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "Pie");
        case "chart.donut":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "Ring");
        case "chart.line":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "Line");
        case "chart.area":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "Filled line");
        case "chart.stacked_bar":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "Stacked bars");
        case "chart.custom":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a chart the user describes in a short text", "Custom chart");
        case "chart.candlestick":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: the balance per day, week or month with its start, highest, lowest and end", "Balance candles");
        case "chart.flow":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: where money came from and where it went, as bands", "Where my money went");
        case "kpi.card":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "One figure");
        case "progress.bar":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "Progress bar");
        case "table.rows":
            return i18nContext.i18nc("@item:inlistbox a part of the layout", "Table");
        case "text.heading":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a line of text that names a section", "Heading");
        case "text.separator":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a thin line that divides the page", "Separator");
        case "notice.fetch_gap":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: warnings about bank accounts that were not updated for a long time", "Warnings about old bank data");
        case "card.net_worth":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: what all the accounts hold together, a card for each currency", "What you are worth");
        case "table.income_sources":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: where the month's money came from, by category", "Where your money came from");
        case "budget.progress":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: how much of each budget is spent and left", "Budgets");
        case "table.transactions":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a list of single transactions, newest first", "Transactions");
        case "group.period":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a framed area with a period of its own for the widgets inside it", "Period box");
        case "bank.update_all":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: one button that gets the new transactions of every account linked to a bank", "Update all accounts");
        case "page.period":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: the month the page shows, with buttons for the months before and after it", "Month and its buttons");
        case "page.tabs":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: the tabs that switch between the user's dashboards", "Dashboard tabs");
        case "page.filters":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: the bar that chooses the period, accounts and categories for the whole page", "Filter bar");
        case "notice.transfers":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a note that money moved between the user's own accounts is not counted", "Note about moved money");
        case "notice.unsorted":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a note with how many transactions have no category yet and the newest of them", "Transactions without a category");
        case "notice.alarms":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: lines that say when a budget or an account crosses a limit the user set", "Warnings");
        }
        return i18nContext.i18nc("@item:inlistbox a part of the layout that this version doesn't know", "Unknown");
    }

    // What a kind of component that starts with data of its own is called in
    // the Add menu (M6c.2).
    function dataName(dataset: string, kind: string): string {
        const ctx = i18nContext;
        if (dataset === "net_worth_over_time") {
            return ctx.i18nc("@item:inlistbox a part of the layout: a line of what the user is worth at the end of each month", "What you are worth over time");
        }
        if (dataset === "cash_flow_by_month" && kind === "table.rows") {
            return ctx.i18nc("@item:inlistbox a part of the layout: a table of the money in and out of the user's accounts and what they held, month by month", "Money in and out of your accounts (table)");
        }
        if (dataset === "cash_flow_by_month") {
            return ctx.i18nc("@item:inlistbox a part of the layout: bars of the money in and out of the user's accounts, month by month", "Money in and out of your accounts");
        }
        if (dataset === "account_balances" && kind === "table.rows") {
            return ctx.i18nc("@item:inlistbox a part of the layout: a table of what each account holds", "What your accounts hold");
        }
        if (dataset === "account_balances") {
            return ctx.i18nc("@item:inlistbox a part of the layout: one figure of what the user has, owes or is worth", "What you have, owe and are worth");
        }
        return typeName(kind);
    }

    // The title the user gave a component (or the text of a heading), if any.
    function titleOf(item: var): string {
        const fields = item.subject?.fields ?? [];
        const field = fields.find(f => f.key === "title" || f.key === "text");
        return field ? field.text : "";
    }

    // What a component is and what it is called, without where it sits.
    function what(item: var): string {
        const title = titleOf(item);
        return title !== "" ? i18nContext.i18nc("@info:tooltip %1 is a kind of component, e.g. Bars, %2 the title the user gave it", "%1: %2", typeName(item.type), title) : typeName(item.type);
    }

    // What a screen reader says for a component: what it is, what it is
    // called and where it sits (D-288). The width and height are counted
    // with their plural forms (#780).
    function name(item: var): string {
        const wide = i18nContext.i18ncp("@info how many columns of the layout a component spans", "%1 column wide", "%1 columns wide", item.cols);
        const high = i18nContext.i18ncp("@info how many rows of the layout a component spans", "%1 row high", "%1 rows high", item.rows);
        return i18nContext.i18nc("@info %1 is what the component is, %2 and %3 are numbers of a column and a row, %4 says how many columns wide (e.g. 2 columns wide), %5 how many rows high", "%1, column %2, row %3, %4, %5", what(item), item.col + 1, item.row + 1, wide, high);
    }

    // The title of the window that shows a whole tile (D-736): what the
    // tile shows, never where it sits in the layout (#780).
    function windowTitle(item: var): string {
        const title = titleOf(item);
        if (title !== "") {
            return title;
        }
        const automatic = automaticTitle(item);
        return automatic !== "" ? automatic : typeName(item.type);
    }

    // What a component is called when its user gave it no title: the
    // built-in Overview has none, so its chart and table say what they show.
    function automaticTitle(item: var): string {
        // A chart the user described names itself, or has no title.
        if (item?.type === "chart.custom") {
            return "";
        }
        const dataset = item?.subject?.dataset;
        const shows = item?.subject?.preview?.shows;
        if (dataset === "spending_by_category" && shows === "chart") {
            return i18nContext.i18nc("@title", "Where your money went");
        }
        if (dataset === "spending_by_category" && shows === "table") {
            return i18nContext.i18nc("@title", "Your biggest spending");
        }
        if (shows === "budget-progress" || shows === "budget-table") {
            return i18nContext.i18nc("@title", "Your budgets");
        }
        if (dataset === "income_expense_by_month" && shows === "chart") {
            return i18nContext.i18nc("@title", "Money in and out");
        }
        // The figures of the accounts (M6c.2). A card says what it shows
        // itself, so it needs no title.
        if (dataset === "net_worth_over_time" && shows !== "card") {
            return i18nContext.i18nc("@title", "What you are worth over time");
        }
        // The money flow names itself (US7, #821).
        if (item?.type === "chart.flow") {
            return i18nContext.i18nc("@title a chart of where money came from and where it went", "Where my money went");
        }
        // The balance as candles (US4.10).
        if (item?.type === "chart.candlestick") {
            return i18nContext.i18nc("@title a candle chart of the accounts' balance", "How your balance moved");
        }
        if (dataset === "cash_flow_by_month" && shows !== "card") {
            return i18nContext.i18nc("@title", "Money in and out of your accounts");
        }
        if (dataset === "account_balances" && shows !== "card") {
            return i18nContext.i18nc("@title", "What your accounts hold");
        }
        return "";
    }

    function periodName(word: string): string {
        switch (word) {
        case "page":
            return i18nContext.i18nc("@item:inlistbox a period: the one the page shows", "The period of the page");
        case "this-month":
            return i18nContext.i18nc("@item:inlistbox a period", "This month");
        case "last-month":
            return i18nContext.i18nc("@item:inlistbox a period", "Last month");
        case "last-12-months":
            return i18nContext.i18nc("@item:inlistbox a period", "The last 12 months");
        case "this-year":
            return i18nContext.i18nc("@item:inlistbox a period", "This year");
        case "last-year":
            return i18nContext.i18nc("@item:inlistbox a period", "Last year");
        }
        // A rolling period is named by its numbers (D-644, D-645), a range
        // the user typed by its dates.
        const described = range.describe(word);
        if (described.rolling === "months") {
            return described.complete
                ? i18nContext.i18ncp("@item:inlistbox a period of whole months that are over", "The last complete month", "The last %1 complete months", described.count)
                : i18nContext.i18ncp("@item:inlistbox a period of months up to and with this one", "This month", "The last %1 months", described.count);
        }
        if (described.rolling === "days") {
            return i18nContext.i18ncp("@item:inlistbox a period of days up to today, today included", "Today", "The last %1 days", described.count);
        }
        if (described.rolling === "salary") {
            return described.before
                ? i18nContext.i18nc("@item:inlistbox a period; %1 is a day of the month, 1 to 31", "The salary month before, from day %1", described.day)
                : i18nContext.i18nc("@item:inlistbox a period; %1 is a day of the month, 1 to 31", "The salary month from day %1", described.day);
        }
        return described.custom ? described.text : "";
    }

    // What an edit did, for the Undo and Redo texts.
    function editName(word: string): string {
        switch (word) {
        case "add":
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Added a widget");
        case "move":
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Moved a widget");
        case "resize":
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Resized a widget");
        case "duplicate":
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Copied a widget");
        case "delete":
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Removed a widget");
        case "change":
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Changed a widget");
        case "reset":
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Went back to the template");
        case "load":
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Loaded a saved layout");
        }
        return "";
    }
}
