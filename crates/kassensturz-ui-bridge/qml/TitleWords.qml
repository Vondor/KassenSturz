// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Words that stand for something in a title (M6-B19 follow-up): `$month`,
// `$year`, `$period` and `$date` are replaced by what the period of the
// component is, in the language of the app, so a title can say "October"
// without anyone editing it each month. Presentation only: the stored title
// keeps the words as typed.

import QtQuick
import org.kde.ki18n
import io.github.vondor.kassensturz

QtObject {
    id: words

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property GridNames names: GridNames {}

    // The words offered to type or pick, with what each one means.
    readonly property var tokens: [
        {token: "$month", help: i18nContext.i18nc("@info:tooltip a word for titles", "The month of the period, e.g. October")},
        {token: "$year", help: i18nContext.i18nc("@info:tooltip a word for titles", "The year of the period, e.g. 2026")},
        {token: "$period", help: i18nContext.i18nc("@info:tooltip a word for titles", "The whole period, e.g. October 2026")},
        {token: "$date", help: i18nContext.i18nc("@info:tooltip a word for titles", "Today's date")}
    ]

    function hasWords(text: string): bool {
        return /\$(month|year|period|date)\b/.test(text);
    }

    // The year and month (1 to 12, or 0 when the period is not one month) a
    // period word stands for, with its plain name.
    function facts(word: string): var {
        const today = new Date();
        const found = /^(\d{4})-(\d{2})$/.exec(word);
        if (found) {
            return {year: Number(found[1]), month: Number(found[2])};
        }
        if (word === "this-month") {
            return {year: today.getFullYear(), month: today.getMonth() + 1};
        }
        if (word === "last-month") {
            const last = new Date(today.getFullYear(), today.getMonth() - 1, 1);
            return {year: last.getFullYear(), month: last.getMonth() + 1};
        }
        if (word === "this-year") {
            return {year: today.getFullYear(), month: 0};
        }
        if (word === "last-year") {
            return {year: today.getFullYear() - 1, month: 0};
        }
        return {year: 0, month: 0};
    }

    function monthName(month: int): string {
        return Qt.locale().monthName(month - 1, Locale.LongFormat);
    }

    function periodText(word: string): string {
        const f = facts(word);
        if (f.month > 0) {
            return monthName(f.month) + " " + f.year;
        }
        if (f.year > 0) {
            return String(f.year);
        }
        return names.periodName(word);
    }

    // `text` with its words replaced for the period `word` (a fixed period
    // name, a month as `YYYY-MM` or a range).
    function resolve(text: string, word: string): string {
        if (!hasWords(text)) {
            return text;
        }
        const f = facts(word);
        const today = new Date();
        return text.replace(/\$(month|year|period|date)\b/g, (all, name) => {
            switch (name) {
            case "month":
                return f.month > 0 ? monthName(f.month) : periodText(word);
            case "year":
                return f.year > 0 ? String(f.year) : String(today.getFullYear());
            case "period":
                return periodText(word);
            }
            return today.toLocaleDateString(Qt.locale(), Locale.LongFormat);
        });
    }
}
