// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The plain words for the key figure cards (M6a.5). The core sends fixed
// names, never text (D-034); this is where they get their words.

import QtQuick
import org.kde.ki18n

QtObject {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function measure(key: string): string {
        switch (key) {
        case "income":
            return i18nContext.i18nc("@title:card a key figure", "Money in");
        case "expenses":
            return i18nContext.i18nc("@title:card a key figure", "Money out");
        case "net":
            return i18nContext.i18nc("@title:card a key figure", "What is left");
        case "net-worth":
        case "net_worth":
            return i18nContext.i18nc("@title:card a key figure: what all accounts hold, less what is owed", "What you are worth");
        case "assets":
            return i18nContext.i18nc("@title:card a key figure: what the checking, savings and cash accounts hold together", "What you have");
        case "liabilities":
            return i18nContext.i18nc("@title:card a key figure: the debt on the credit cards", "What you owe");
        case "savings-rate":
            return i18nContext.i18nc("@title:card a key figure", "Share you kept");
        case "formula":
            return i18nContext.i18nc("@title:card a key figure the user wrote a formula for", "Your formula");
        }
        return "";
    }

    // What the figure did against the earlier period, in words, so the arrow
    // is never the only sign (D-283).
    function direction(key: string): string {
        switch (key) {
        case "up":
            return i18nContext.i18nc("@info a figure against the earlier period", "More than before");
        case "down":
            return i18nContext.i18nc("@info a figure against the earlier period", "Less than before");
        case "same":
            return i18nContext.i18nc("@info a figure against the earlier period", "The same as before");
        }
        return i18nContext.i18nc("@info a figure against the earlier period", "Nothing to compare with");
    }

    function arrow(key: string): string {
        return key === "up" ? "↑" : key === "down" ? "↓" : key === "same" ? "=" : "";
    }

    function before(text: string): string {
        return i18nContext.i18nc("@info the figure of the earlier period, %1 is an amount", "Before: %1", text);
    }

    function noValue(): string {
        return i18nContext.i18nc("@info a figure that can't be worked out", "No value yet");
    }
}
