// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The kinds of account the user can choose from, and their names. The keys
// are the ones the Rust side knows (first_run::type_key).

import QtQuick
import org.kde.ki18n

QtObject {
    id: types

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // In the order the UI offers them.
    readonly property var all: [
        {
            key: "checking",
            name: i18nContext.i18nc("@item:inlistbox an account type", "Checking account"),
            hint: i18nContext.i18nc("@info", "The account your pay goes into and your bills are paid from."),
        },
        {
            key: "savings",
            name: i18nContext.i18nc("@item:inlistbox an account type", "Savings account"),
            hint: i18nContext.i18nc("@info", "Money you put aside."),
        },
        {
            key: "cash",
            name: i18nContext.i18nc("@item:inlistbox an account type", "Cash"),
            hint: i18nContext.i18nc("@info", "The money in your wallet."),
        },
        {
            key: "credit-card",
            name: i18nContext.i18nc("@item:inlistbox an account type", "Credit card"),
            hint: i18nContext.i18nc("@info", "What you buy with it, you pay back later. KassenSturz shows what you owe on it."),
        },
    ]

    // The name of the type with this key; empty for an account that has none.
    function nameOf(key: string): string {
        const found = all.find(type => type.key === key);
        return found ? found.name : "";
    }
}
