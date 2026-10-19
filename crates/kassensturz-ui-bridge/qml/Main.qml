// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The first-run mock (M1.M, D-068): welcome, add an account, add a
// transaction, see the overview. Amounts and dates arrive as finished text
// and go back as typed text (D-065), so nothing here calculates with them.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: root

    // Translations come from the "kassensturz" catalog (D-034), set up from
    // QML alone, so no KDE Frameworks library is linked into the Rust program.
    // KI18n then warns once at startup that it "cannot filter events for
    // objects in a different thread": Qt loads its QML plugin on a loader
    // thread. Translating works; only switching languages while the app runs
    // wouldn't.
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The app's name; it isn't translated.
    title: "KassenSturz"
    width: Kirigami.Units.gridUnit * 40
    height: Kirigami.Units.gridUnit * 38
    minimumWidth: Kirigami.Units.gridUnit * 20
    minimumHeight: Kirigami.Units.gridUnit * 20

    FirstRunMock {
        id: mockBook
    }

    // Made-up categories for trying the flow. The real default set comes with
    // the first-run flow in M3 (D-029); like it, these are stored in the
    // language the book is started in (D-067).
    Component.onCompleted: mockBook.start([
        i18nContext.i18nc("@item a spending category", "Groceries"),
        i18nContext.i18nc("@item a spending category", "Rent"),
        i18nContext.i18nc("@item a spending category", "Eating out"),
        i18nContext.i18nc("@item a spending category", "Getting around"),
        i18nContext.i18nc("@item a spending category", "Clothes"),
        i18nContext.i18nc("@item a spending category", "Free time"),
        i18nContext.i18nc("@item a spending category", "Other spending"),
    ], [
        i18nContext.i18nc("@item an income category", "Salary"),
        i18nContext.i18nc("@item an income category", "Other income"),
    ])

    function addAccount(firstRun: bool) {
        pageStack.push(accountPage, { firstRun: firstRun });
    }

    function addTransaction(firstRun: bool) {
        if (firstRun) {
            // The account is there now: going back to add it again would
            // only add a second one, so the first-run steps are replaced.
            pageStack.pop(pageStack.get(0));
            pageStack.replace(transactionPage, { firstRun: true });
        } else {
            pageStack.push(transactionPage, { firstRun: false });
        }
    }

    function showOverview() {
        pageStack.replace(overviewPage);
    }

    // One page at a time, also in a wide window: the first start is a
    // sequence of steps.
    pageStack.columnView.columnResizeMode: Kirigami.ColumnView.SingleColumn
    pageStack.initialPage: WelcomePage {
        onAddAccountClicked: root.addAccount(true)
    }

    Component {
        id: accountPage

        AccountPage {
            mock: mockBook
            onAdded: firstRun ? root.addTransaction(true) : root.pageStack.pop()
        }
    }

    Component {
        id: transactionPage

        TransactionPage {
            mock: mockBook
            onDone: firstRun ? root.showOverview() : root.pageStack.pop()
        }
    }

    Component {
        id: overviewPage

        OverviewPage {
            mock: mockBook
            onAddAccountClicked: root.addAccount(false)
            onAddTransactionClicked: root.addTransaction(false)
            onAccountClicked: index => root.pageStack.push(registerPage, { account: index })
        }
    }

    Component {
        id: registerPage

        RegisterPage {
            mock: mockBook
        }
    }
}
