// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One account's transactions, newest first, and its balance (M1.M).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property FirstRunMock mock
    // The account's place in the overview's list.
    property int account: 0
    // Read again whenever the book changes.
    readonly property var view: {
        mock.overview;
        return mock.transactions(account);
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The balance, with `amount` as shown or exactly. A debt reads "You owe"
    // (D-067).
    function balanceText(amount: string): string {
        return view.owes
            ? i18nContext.i18nc("@label %1 is an amount of money", "You owe: %1", amount)
            : i18nContext.i18nc("@label %1 is an amount of money", "Balance: %1", amount);
    }

    title: view.name
    header: MockNotice {}

    footer: Controls.ToolBar {
        contentItem: AmountLabel {
            text: page.balanceText(page.view.balance)
            exactText: page.balanceText(page.view.exactBalance)
            font.bold: true
            horizontalAlignment: Text.AlignRight
        }
    }

    ListView {
        model: page.view.rows

        delegate: Controls.ItemDelegate {
            id: delegate

            required property var modelData

            width: ListView.view.width
            Accessible.name: transaction.Accessible.name

            contentItem: TransactionRow {
                id: transaction
                modelData: delegate.modelData
            }
        }
    }
}
