// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// An account's balance. A debt reads "You owe" (D-067). Like AmountLabel,
// pointing at it shows the exact amount if it is shown rounded (D-002).

import QtQuick
import org.kde.ki18n

AmountLabel {
    id: balance

    // The amount as shown and exactly, both text, and whether it is a debt.
    property string amount
    property string exactAmount
    property bool owes: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function phrase(amount: string): string {
        return owes ? i18nContext.i18nc("@label %1 is an amount of money", "You owe %1", amount) : amount;
    }

    text: phrase(amount)
    exactText: phrase(exactAmount)
}
