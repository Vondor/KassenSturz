// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What a formula counts, in words (M6b.7): the formula field shows it under
// its result, and the review page of the AI under a formula the AI wrote,
// before the user confirms it (US1.17, D-650). The lines come from Rust
// (`formulas.rs`); only the sentence is made here, where KI18n's tools
// extract it (D-034).

import QtQuick
import org.kde.ki18n

QtObject {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // `Spending.ALL` and `Income.ALL` in words.
    function allName(kind: string): string {
        return kind === "income"
            ? i18nContext.i18nc("@info a name in a list of what a formula counts: all money received", "all income")
            : i18nContext.i18nc("@info a name in a list of what a formula counts: all money spent", "all spending");
    }

    // A figure of the planned payments in words (D-826): what the plans
    // still expect from today up to `lastDay`, or what is overdue, which
    // the forecast lists as "Did this happen?".
    function planName(kind: string, lastDay: string): string {
        switch (kind) {
        case "plans-out-due":
            return i18nContext.i18nc("@info a name in a list of what a formula counts; %1 is a date", "what your planned payments still expect to go out until %1", lastDay);
        case "plans-in-due":
            return i18nContext.i18nc("@info a name in a list of what a formula counts; %1 is a date", "planned income still expected until %1", lastDay);
        case "plans-out-overdue":
            return i18nContext.i18nc("@info a name in a list of what a formula counts: planned payments whose day passed with no entry", "planned payments that are overdue and not confirmed yet");
        default:
            return i18nContext.i18nc("@info a name in a list of what a formula counts: planned income whose day passed with no entry", "planned income that is overdue and not confirmed yet");
        }
    }

    // The words of "counts Pets (with Vet), Travel".
    function counts(lines: var): string {
        const parts = lines.map(line => {
            if (line.all.startsWith("plans-")) {
                // Its days are in its words: from today on.
                return planName(line.all, line.lastDay);
            }
            let name = line.all ? allName(line.all) : line.name;
            if (line.inside.length > 0) {
                const more = line.insideMore > 0
                    ? i18nContext.i18ncp("@info the end of a list of names", "and %1 more", "and %1 more", line.insideMore) : "";
                name = i18nContext.i18nc("@info %1 is a category, %2 the names inside it", "%1 (with %2)", name, [line.inside.join(", "), more].filter(part => part !== "").join(" "));
            }
            if (!line.ownPeriod) {
                name = line.month !== ""
                    ? i18nContext.i18nc("@info %1 is a category, %2 a month", "%1 in %2", name, line.month)
                    : i18nContext.i18nc("@info %1 is a category, %2 and %3 are dates", "%1 from %2 to %3", name, line.firstDay, line.lastDay);
            }
            return name;
        });
        return parts.join(", ");
    }
}
