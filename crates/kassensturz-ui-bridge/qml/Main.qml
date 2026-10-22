// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's window: it opens the book (D-033) and shows one screen at a
// time, each in a QML file of its own, chosen from the side menu. Until the
// book has an account, the first-run steps of the mock (M1.M) show instead.
//
// Adding a screen: write its page in a file of its own, add it to
// `build.rs`, and give it a `Component` and an entry in `screens` and in the
// drawer's actions below. Amounts and dates arrive as finished text and go
// back as typed text (D-065), so nothing here calculates with them.

import QtQuick
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: root

    // Translations come from the "kassensturz" catalog (D-034), set up from
    // QML alone (D-074). KI18n then warns once at startup that it "cannot
    // filter events for objects in a different thread": Qt loads its QML
    // plugin on a loader thread. Translating works; only switching languages
    // while the app runs wouldn't.
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The screens the side menu leads to, by name.
    readonly property var screens: ({
        "overview": overviewPage,
        "accounts": accountsPage,
        "categories": categoriesPage,
        "settings": settingsPage,
    })
    // Whether the first-run steps are done, so the side menu is offered.
    readonly property bool started: bookSession.open && mockBook.accounts.length > 0

    // The app's name; it isn't translated.
    title: "KassenSturz"
    width: Kirigami.Units.gridUnit * 40
    height: Kirigami.Units.gridUnit * 38
    minimumWidth: Kirigami.Units.gridUnit * 20
    minimumHeight: Kirigami.Units.gridUnit * 20

    BookSession {
        id: bookSession

        onOpened: root.showBook()
        onFailed: root.showProblem()
    }

    FirstRunMock {
        id: mockBook
    }

    // The accounts and categories screens (M3.3). A change in one of them,
    // or in the first-run mock, is read again by all the others.
    AccountList {
        id: accountList

        onChanged: root.bookChanged()
    }

    CategoryTree {
        id: categoryTree

        onChanged: root.bookChanged()
    }

    // How amounts look (D-002): the screens show them again when it changes.
    NumberFormatSettings {
        id: numberFormatSettings

        onChoicesChanged: {
            mockBook.refresh();
            accountList.refresh();
        }
    }

    // The app's own book is called this in the user's language, e.g.
    // Haushalt.ksz in German.
    Component.onCompleted: bookSession.openLast(i18nContext.i18nc("@item the file name of the app's own book, without its extension", "Household"))

    // Shows the book that was just opened: its first-run steps if it has no
    // account yet, otherwise the overview.
    function showBook() {
        // Made-up categories for trying the flow, for a book that has none.
        // The real default set comes with the first-run flow in M3.4
        // (D-029); like it, these are stored in the language the book is
        // started in (D-067).
        mockBook.start([
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
        ]);
        accountList.refresh();
        categoryTree.refresh();
        if (mockBook.accounts.length === 0) {
            pageStack.clear();
            pageStack.push(welcomePage);
        } else {
            showScreen("overview");
        }
    }

    // The accounts or categories changed on one of their screens: the
    // first-run mock and the other screen read the book again. The mock finds
    // the categories it already has, so it needs no names for new ones.
    function bookChanged() {
        mockBook.start([], []);
        accountList.refresh();
        categoryTree.refresh();
    }

    // Says why a book didn't open. A book that was open stays open, and the
    // page offers to go back to it.
    function showProblem() {
        if (pageStack.currentItem && pageStack.currentItem.showsBookProblem) {
            return;
        }
        if (!bookSession.open) {
            pageStack.clear();
        }
        pageStack.push(problemPage);
    }

    function showScreen(name: string) {
        pageStack.clear();
        pageStack.push(screens[name]);
    }

    function chooseBookToOpen() {
        openDialog.open();
    }

    function chooseNewBook() {
        newBookDialog.open();
    }

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

    // One page at a time, also in a wide window.
    pageStack.columnView.columnResizeMode: Kirigami.ColumnView.SingleColumn

    // A book on a network drive opens, with this warning on every screen
    // (D-038).
    header: Kirigami.InlineMessage {
        position: Kirigami.InlineMessage.Position.Header
        type: Kirigami.MessageType.Warning
        visible: bookSession.open && bookSession.networkDrive
        text: root.i18nContext.i18nc("@info", "This book is on a network drive. Network drives can damage a book, because they don't save changes as reliably as this computer's own drive. Keep the book on this computer; a sync folder, such as Nextcloud's, is fine.")
    }

    globalDrawer: Kirigami.GlobalDrawer {
        enabled: root.started
        modal: !root.wideScreen || !root.started
        drawerOpen: !modal
        handleVisible: modal && root.started

        actions: [
            Kirigami.Action {
                icon.name: "go-home"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Overview")
                onTriggered: root.showScreen("overview")
            },
            Kirigami.Action {
                icon.name: "view-financial-account"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Accounts")
                onTriggered: root.showScreen("accounts")
            },
            Kirigami.Action {
                icon.name: "view-financial-categories"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Categories")
                onTriggered: root.showScreen("categories")
            },
            Kirigami.Action {
                icon.name: "settings-configure"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Settings")
                onTriggered: root.showScreen("settings")
            }
        ]
    }

    Dialogs.FileDialog {
        id: openDialog

        title: root.i18nContext.i18nc("@title:window", "Open another book")
        fileMode: Dialogs.FileDialog.OpenFile
        currentFolder: bookSession.folderUrl
        nameFilters: [root.i18nContext.i18nc("@item:inlistbox a file type", "KassenSturz books (*.ksz)")]
        onAccepted: bookSession.openFile(selectedFile)
    }

    Dialogs.FileDialog {
        id: newBookDialog

        title: root.i18nContext.i18nc("@title:window", "Start a new book")
        fileMode: Dialogs.FileDialog.SaveFile
        currentFolder: bookSession.folderUrl
        defaultSuffix: "ksz"
        nameFilters: [root.i18nContext.i18nc("@item:inlistbox a file type", "KassenSturz books (*.ksz)")]
        onAccepted: bookSession.createFile(selectedFile)
    }

    Component {
        id: welcomePage

        WelcomePage {
            onAddAccountClicked: root.addAccount(true)
        }
    }

    Component {
        id: problemPage

        BookProblemPage {
            session: bookSession
            onOpenAnotherClicked: root.chooseBookToOpen()
            onNewBookClicked: root.chooseNewBook()
            onBackClicked: {
                bookSession.dismissProblem();
                root.pageStack.pop();
            }
        }
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
            onDone: firstRun ? root.showScreen("overview") : root.pageStack.pop()
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

    Component {
        id: accountsPage

        AccountsPage {
            accounts: accountList
            onAddAccountClicked: root.pageStack.push(newAccountPage)
            onAccountClicked: accountId => root.pageStack.push(accountDetailsPage, { accountId: accountId })
        }
    }

    Component {
        id: newAccountPage

        NewAccountPage {
            accounts: accountList
            onAdded: root.pageStack.pop()
        }
    }

    Component {
        id: accountDetailsPage

        AccountDetailsPage {
            accounts: accountList
            onCheckClicked: root.pageStack.push(bankCheckPage, { accountId: accountId })
            // Closed or opened again: back to the list.
            onAccountClosedOrOpened: root.pageStack.pop()
        }
    }

    Component {
        id: bankCheckPage

        BankCheckPage {
            accounts: accountList
        }
    }

    Component {
        id: categoriesPage

        CategoriesPage {
            categories: categoryTree
            onAddCategoryClicked: root.pageStack.push(categoryPage)
            onCategoryClicked: categoryId => {
                const row = categoryTree.view.rows.find(row => row.id === categoryId);
                root.pageStack.push(categoryPage, {
                    categoryId: row.id,
                    categoryName: row.name,
                    income: row.income,
                    parentId: row.parent,
                });
            }
        }
    }

    Component {
        id: categoryPage

        CategoryPage {
            categories: categoryTree
            onDone: root.pageStack.pop()
        }
    }

    Component {
        id: settingsPage

        SettingsPage {
            session: bookSession
            numberFormat: numberFormatSettings
            onOpenAnotherClicked: root.chooseBookToOpen()
            onNewBookClicked: root.chooseNewBook()
        }
    }
}
