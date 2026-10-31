// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The words of the budget screens that more than one file uses (M6c.5): how
// often a budget repeats, and a stretch of days. The Rust side sends fixed
// words and dates as text; the sentences are written here, where KI18n's
// tools extract them (D-034).

import QtQuick
import org.kde.ki18n

QtObject {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The fixed words, in the order the sheet offers them.
    readonly property var everyWords: ["month", "quarter", "year", "range"]

    function every(word: string): string {
        switch (word) {
        case "month":
            return i18nContext.i18nc("@item:inlistbox how often a budget repeats", "Every month");
        case "quarter":
            return i18nContext.i18nc("@item:inlistbox how often a budget repeats", "Every quarter");
        case "year":
            return i18nContext.i18nc("@item:inlistbox how often a budget repeats", "Every year");
        default:
            return i18nContext.i18nc("@item:inlistbox a budget for one stretch of days", "Once, for some days");
        }
    }

    // A stretch of days, e.g. "1 Nov 2026 to 30 Nov 2026".
    function days(first: string, last: string): string {
        return i18nContext.i18nc("@info %1 and %2 are dates", "%1 to %2", first, last);
    }

    // A period of the sheet's lists; the current one says so.
    function period(choice: var): string {
        return choice.current
            ? i18nContext.i18nc("@item:inlistbox %1 is a stretch of days", "%1 (this one)", days(choice.firstDay, choice.lastDay))
            : days(choice.firstDay, choice.lastDay);
    }
}
