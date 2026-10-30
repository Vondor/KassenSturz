// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The plain words for the grid editor (M6b.3, D-311). The core sends fixed
// names, never text (D-034); this is where they get their words.

import QtQuick
import org.kde.ki18n

QtObject {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

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
        case "group.period":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a framed area with a period of its own for the widgets inside it", "Period box");
        case "notice.transfers":
            return i18nContext.i18nc("@item:inlistbox a part of the layout: a note that money moved between the user's own accounts is not counted", "Note about moved money");
        }
        return i18nContext.i18nc("@item:inlistbox a part of the layout that this version doesn't know", "Unknown");
    }

    // The title the user gave a component (or the text of a heading), if any.
    function titleOf(item: var): string {
        const fields = item.subject?.fields ?? [];
        const field = fields.find(f => f.key === "title" || f.key === "text");
        return field ? field.text : "";
    }

    // What a screen reader says for a component: what it is, what it is
    // called and where it sits (D-288).
    function name(item: var): string {
        const title = titleOf(item);
        const what = title !== "" ? i18nContext.i18nc("@info:tooltip %1 is a kind of component, e.g. Bars, %2 the title the user gave it", "%1: %2", typeName(item.type), title) : typeName(item.type);
        return i18nContext.i18nc("@info %1 is what the component is, %2 and %3 are numbers of a column and a row, %4 and %5 are how many columns wide and rows high", "%1, column %2, row %3, %4 columns wide, %5 rows high", what, item.col + 1, item.row + 1, item.cols, item.rows);
    }

    // What a component is called when its user gave it no title: the
    // built-in Overview has none, so its chart and table say what they show.
    function automaticTitle(item: var): string {
        const dataset = item?.subject?.dataset;
        const shows = item?.subject?.preview?.shows;
        if (dataset === "spending_by_category" && shows === "chart") {
            return i18nContext.i18nc("@title", "Where your money went");
        }
        if (dataset === "spending_by_category" && shows === "table") {
            return i18nContext.i18nc("@title", "Your biggest spending");
        }
        if (dataset === "income_expense_by_month" && shows === "chart") {
            return i18nContext.i18nc("@title", "Money in and out");
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
        return "";
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
            return i18nContext.i18nc("@info:tooltip what was done to the layout", "Went back to the built-in layout");
        }
        return "";
    }
}
