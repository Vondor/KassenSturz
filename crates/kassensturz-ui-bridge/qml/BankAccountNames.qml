// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What an account at the bank is called on the screens that list them
// (M4.6, M4.7). A real bank names its accounts in its own words, and those
// are shown as they come (as plain text, D-143). The made-up bank of
// `KASSENSTURZ_FAKE_BANK` has English names in its code, which the German
// app would show as they are, so for it, and for an account the bank gives
// no name, the name comes from what kind of account it is.

import QtQuick
import org.kde.ki18n

QtObject {
    id: names

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // `account` has a `name` and a `kind` (`FoundAccount` in the bridge);
    // `fake` says whether the bank is the made-up one.
    function name(account: var, fake: bool): string {
        if (!fake && (account.name ?? "") !== "") {
            return account.name;
        }
        switch (account.kind) {
        case "current":
            return i18nContext.i18nc("@label an account at the bank, by its kind", "Checking account");
        case "savings":
            return i18nContext.i18nc("@label an account at the bank, by its kind", "Savings account");
        case "credit-card":
            return i18nContext.i18nc("@label an account at the bank, by its kind", "Credit card");
        default:
            return i18nContext.i18nc("@label an account at the bank without a name", "Account");
        }
    }
}
