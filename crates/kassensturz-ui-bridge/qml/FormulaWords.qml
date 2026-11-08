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

    // The words of "counts Pets (with Vet), Travel".
    function counts(lines: var): string {
        const parts = lines.map(line => {
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
