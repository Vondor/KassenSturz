// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The names of the ready chart descriptions (US4.11, #700 part A, D-720).
// The examples themselves are written once, in Rust
// (`kassensturz_report::spec::SPEC_EXAMPLES`), and reach QML through
// `ChartDescription.examples()`; the core sends their fixed keys, never
// text (D-034), so their names are put into words here. A test checks that
// every key has a case below.

import QtQuick
import org.kde.ki18n

QtObject {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function name(key: string): string {
        switch (key) {
        case "balance-and-flows":
            return i18nContext.i18nc("@item:inmenu a ready chart description", "Balance as bars, money in and out as lines");
        case "left-each-month":
            return i18nContext.i18nc("@item:inmenu a ready chart description", "What is left each month");
        case "spending-ring":
            return i18nContext.i18nc("@item:inmenu a ready chart description", "Spending as a ring, by main category");
        case "worth-and-average":
            return i18nContext.i18nc("@item:inmenu a ready chart description", "What you are worth, with its average");
        }
        return key;
    }
}
