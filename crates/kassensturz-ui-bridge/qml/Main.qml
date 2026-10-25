// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's window: it opens the book (D-033) and shows one screen at a
// time, each in a QML file of its own, chosen from the side menu. Until the
// book has an account, the first-run steps show instead (D-116): welcome and
// currency, the first account, an optional first transaction.
//
// Adding a screen: write its page in a file of its own, add it to
// `build.rs`, and give it a `Component` and an entry in `screens` and in the
// drawer's actions below. Pages are opened with `pushPage` and
// `replacePage`, not with the page stack's own `push` and `replace` (#77).
// Undo and redo are in the drawer too, with their shortcuts, and name the
// change they act on (M3.8). The import of a bank's file (M3.10) starts
// from the register and runs through its own pages.
// Saving a copy of the data, and starting a book from a file, are reached
// from Settings, and the second also from the first screen (M3.12).
// Amounts and dates arrive as finished text and go back as typed text
// (D-065), so nothing here calculates with them.

import QtQuick
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: root

    // Translations come from the "kassensturz" catalog (D-034), set up from
    // QML alone (D-074). Qt then warns once at startup that it "cannot filter
    // events for objects in a different thread" (#77): KI18n's QML library
    // keeps its language watcher in a static object, made on the thread that
    // loads the library, which is Qt's QML loader thread, and this context
    // puts it to work on the main thread. Translating works; only switching
    // languages while the app runs wouldn't.
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The screens the side menu leads to, by name.
    readonly property var screens: ({
        "overview": overviewPage,
        "accounts": accountsPage,
        "categories": categoriesPage,
        "rules": rulesPage,
        "settings": settingsPage,
    })
    // Whether the first-run steps are done, so the side menu is offered.
    readonly property bool started: bookSession.open && firstStartState.hasAccount

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

    // The first start (M3.4).
    FirstStart {
        id: firstStartState
    }

    // The overview (M3.9): accounts, one month's money in and out, the
    // latest transactions.
    BookOverview {
        id: bookOverview
    }

    // The accounts and categories screens (M3.3). A change in one of them
    // is read again by all the others.
    AccountList {
        id: accountList

        onChanged: root.screenChangedBook()
    }

    CategoryTree {
        id: categoryTree

        onChanged: root.screenChangedBook()
    }

    // The Rules and their suggestions (M3.11). A change in them is read
    // again by all the other screens, because a Rule can sort transactions.
    RuleBook {
        id: ruleBook

        onChanged: root.screenChangedBook()
    }

    // The register and its pages (M3.5): one list model, shown for the
    // account whose page is open.
    AccountRegister {
        id: accountRegister

        onChanged: root.screenChangedBook()
    }

    // Importing a file from the bank into an account (M3.10): one file at a
    // time, from the file dialog to the summary.
    // Named differently from the pages' `statementImport` property, which
    // would otherwise bind to itself.
    StatementImport {
        id: statementImporter

        onChanged: root.screenChangedBook()
    }

    // Saving a copy of the data in a new file (M3.12).
    DataExport {
        id: dataExporter
    }

    // Starting a book from a complete export or a KMyMoney book (M3.12). It
    // fills the open book if that is still empty (`changed`), otherwise it
    // makes a new one and opens it (`created`); the report comes next.
    BookFromFile {
        id: bookFromFileState

        onChanged: {
            root.bookChanged();
            root.replacePage(bookFromFileDonePage);
        }
        onCreated: {
            bookSession.adoptOpenBook();
            root.pushPage(bookFromFileDonePage);
        }
    }

    // Writes error codes as messages, for a notice.
    FormError {
        id: errorTexts

        visible: false
    }

    // Undo, redo and the list of changes (M3.8). Undoing or redoing changes
    // the book, so all the screens read it again.
    ChangeHistory {
        id: changeHistory

        onChanged: root.bookChanged()
    }

    ChangeNames {
        id: changeNames
    }

    // The bank connection (M4.6): one per app, with the dialogs for what a
    // bank asks while it is contacted.
    BankConnection {
        id: bankConnection
    }

    BankDialogs {
        bank: bankConnection
    }

    // The one AI switch (D-013): every AI element in the screens hides
    // behind its `enabled`.
    AiSwitch {
        id: aiSwitch
    }

    // The language (D-127), which applies at the next start.
    LanguageSettings {
        id: languageSettings
    }

    // How amounts look (D-002): the screens show them again when it changes.
    NumberFormatSettings {
        id: numberFormatSettings

        onChoicesChanged: {
            bookOverview.refresh();
            accountList.refresh();
            accountRegister.refresh();
        }
    }

    // The app's own book is called this in the user's language, e.g.
    // Haushalt.ksz in German.
    Component.onCompleted: bookSession.openLast(i18nContext.i18nc("@item the file name of the app's own book, without its extension", "Household"))

    // Shows the book that was just opened: its first-run steps if it has no
    // account yet, otherwise the overview.
    function showBook() {
        bookChanged();
        if (!firstStartState.hasAccount) {
            pageStack.clear();
            pushPage(welcomePage);
        } else {
            showScreen("overview");
        }
    }

    // The book changed on one of its screens: every screen reads it again.
    function bookChanged() {
        firstStartState.refresh();
        bookOverview.refresh();
        accountList.refresh();
        categoryTree.refresh();
        ruleBook.refresh();
        accountRegister.refresh();
        changeHistory.refresh();
    }

    // A screen changed the book: every screen reads it again, and the user
    // can undo the change from the notice (M3.8, D-086).
    function screenChangedBook() {
        bookChanged();
        if (started && changeHistory.undoChange?.action !== undefined) {
            showPassiveNotification(changeNames.name(changeHistory.undoChange.action), 8000, i18nContext.i18nc("@action:button take the last change back", "Undo"), () => root.undoLast());
        }
    }

    function undoLast() {
        afterStep(changeHistory.undo(), false);
    }

    function redoLast() {
        afterStep(changeHistory.redo(), true);
    }

    // Says what undo or redo did, or why it didn't work.
    function afterStep(result: var, redone: bool) {
        if (result.code !== undefined) {
            const page = pageStack.currentItem;
            if (page && page.failure !== undefined) {
                page.failure = result;
            } else {
                pushPage(historyPage, { failure: result });
            }
            return;
        }
        if (result.change === undefined) {
            return;
        }
        leaveStalePages();
        const what = changeNames.describe(result.change);
        if (redone) {
            showPassiveNotification(i18nContext.i18nc("@info %1 is what was done, e.g. Added a transaction", "Made again: %1", what), 10000, i18nContext.i18nc("@action:button", "Undo"), () => root.undoLast());
        } else {
            showPassiveNotification(i18nContext.i18nc("@info %1 is what was done, e.g. Added a transaction", "Undone: %1", what), 10000, i18nContext.i18nc("@action:button", "Redo"), () => root.redoLast());
        }
    }

    // After undo or redo, a page for something that may be gone (a form for
    // a transaction, an account's page) is closed. The first-run steps show
    // again if the book has no account anymore.
    function leaveStalePages() {
        if (!firstStartState.hasAccount) {
            showBook();
            return;
        }
        while (pageStack.depth > 1) {
            const page = pageStack.currentItem;
            if (page && page.survivesUndo !== undefined && page.survivesUndo()) {
                break;
            }
            pageStack.pop();
        }
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
        pushPage(problemPage);
    }

    function showScreen(name: string) {
        pageStack.clear();
        pushPage(screens[name]);
    }

    // Shows a new page made from `component` on top of the current one.
    function pushPage(component: Component, properties: var): Item {
        return pageStack.push(makePage(component, properties));
    }

    // Shows a new page made from `component` in place of the current one.
    function replacePage(component: Component, properties: var): Item {
        return pageStack.replace(makePage(component, properties));
    }

    // Makes a page for the page stack (#77). Given the component, Kirigami
    // makes the page under an object that isn't drawn, and Qt warns that the
    // page "was not placed in the graphics scene" until the stack takes it.
    // Made under the window instead, it is taken out of the window again at
    // once. The window, as its owner, keeps it from being collected as
    // garbage while it is in the stack; `onPageRemoved` below deletes it
    // when it leaves, which the stack does not always do (D-161).
    function makePage(component: Component, properties: var): Item {
        const page = component.createObject(root, properties ?? {});
        if (!page) {
            throw new Error("Error while making a page: " + component.errorString());
        }
        page.parent = null;
        return page;
    }

    function chooseBookToOpen() {
        openDialog.open();
    }

    function chooseNewBook() {
        newBookDialog.open();
    }

    function chooseFileForNewBook() {
        fromFileDialog.open();
    }

    // Reads `file` and shows what it holds, or says why a book can't be
    // started from it (M3.12).
    function startBookFromFile(file: url) {
        const result = bookFromFileState.start(file);
        if (result.code !== undefined) {
            showPassiveNotification(errorTexts.messageFor(result), 12000);
            return;
        }
        pushPage(bookFromFilePage);
    }

    function addAccount(firstRun: bool) {
        pushPage(newAccountPage, { firstRun: firstRun });
    }

    // Adds a transaction to the account with `accountId`, or lets the user
    // choose one if it is empty.
    function addTransaction(firstRun: bool, accountId: string) {
        if (firstRun) {
            // The account is there now: going back to add it again would
            // only add a second one, so the first-run steps are replaced.
            // The first account is the only one.
            const accounts = accountRegister.accountChoices();
            const first = accounts.length > 0 ? accounts[accounts.length - 1].id : "";
            pageStack.pop(pageStack.get(0));
            replacePage(transactionPage, { firstRun: true, accountId: first });
        } else {
            pushPage(transactionPage, { firstRun: false, accountId: accountId });
        }
    }

    function showRegister(accountId: string) {
        pushPage(registerPage, { accountId: accountId });
    }

    // Imports a file from the bank into the account with `accountId`
    // (M3.10): the file dialog first, then the screen the file needs.
    function importStatement(accountId: string) {
        importDialog.accountId = accountId;
        importDialog.open();
    }

    function startImport(accountId: string, file: url) {
        const result = statementImporter.start(accountId, file);
        if (result.code === "input.whole-book") {
            // A book from another program: a new book can be started from it.
            showPassiveNotification(errorTexts.messageFor(result), 15000, i18nContext.i18nc("@action:button", "Start a book from it"), () => root.startBookFromFile(file));
            return;
        }
        if (result.code !== undefined) {
            showPassiveNotification(errorTexts.messageFor(result), 12000);
            return;
        }
        pushPage(importStep());
    }

    // Looks at the statement just fetched from the bank for the account with
    // `accountId` (M4.7): the preview comes next, as for a file.
    function startBankImport(accountId: string) {
        const result = statementImporter.startFromBank(accountId);
        if (result.code !== undefined) {
            showPassiveNotification(errorTexts.messageFor(result), 12000);
            return;
        }
        pushPage(importStep());
    }

    // Whether bank connections can be used, asked once: the bank connection
    // starts its own thread the first time it is asked.
    property var bankUsableCache: undefined

    function bankUsable(): bool {
        if (bankUsableCache === undefined) {
            bankUsableCache = bankConnection.status().ready === true;
        }
        return bankUsableCache;
    }

    // The screen for the step the import is at.
    function importStep(): Component {
        switch (statementImporter.step) {
        case "statements":
            return importStatementsPage;
        case "csv":
            return csvSettingsPage;
        default:
            return importPreviewPage;
        }
    }

    // Opens a transaction to change or remove it: one broken down into Parts
    // on the Parts page (M3.6), every other on the transaction page.
    function openTransaction(accountId: string, transactionId: string) {
        const kind = accountRegister.details(accountId, transactionId).kind ?? "simple";
        pushPage(kind === "parts" ? partsPage : transactionPage, { accountId: accountId, transactionId: transactionId });
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
                icon.name: "view-filter"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Rules")
                onTriggered: root.showScreen("rules")
            },
            Kirigami.Action {
                icon.name: "settings-configure"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Settings")
                onTriggered: root.showScreen("settings")
            },
            Kirigami.Action {
                separator: true
            },
            Kirigami.Action {
                icon.name: "edit-undo"
                enabled: root.started && changeHistory.undoChange?.action !== undefined
                text: enabled
                    ? root.i18nContext.i18nc("@action:inmenu %1 is what was done, e.g. Added a transaction", "Undo: %1", changeNames.describe(changeHistory.undoChange))
                    : root.i18nContext.i18nc("@action:inmenu there is nothing to undo", "Undo")
                shortcut: StandardKey.Undo
                onTriggered: root.undoLast()
            },
            Kirigami.Action {
                icon.name: "edit-redo"
                enabled: root.started && changeHistory.redoChange?.action !== undefined
                text: enabled
                    ? root.i18nContext.i18nc("@action:inmenu %1 is what was done, e.g. Added a transaction", "Redo: %1", changeNames.describe(changeHistory.redoChange))
                    : root.i18nContext.i18nc("@action:inmenu there is nothing to redo", "Redo")
                shortcut: StandardKey.Redo
                onTriggered: root.redoLast()
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

    Dialogs.FileDialog {
        id: fromFileDialog

        title: root.i18nContext.i18nc("@title:window", "Start a book from a file")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [
            // First, because a KMyMoney database may have no ending at all
            // (it can be called just "KMyMoney"); KassenSturz tells the
            // kind of file by what is in it.
            root.i18nContext.i18nc("@item:inlistbox a file type", "All files (*)"),
            root.i18nContext.i18nc("@item:inlistbox a file type", "Books (*.kmy *.sqlite *.db *.json)")
        ]
        onAccepted: root.startBookFromFile(selectedFile)
    }

    Dialogs.FileDialog {
        id: importDialog

        // The account the file goes into.
        property string accountId: ""

        title: root.i18nContext.i18nc("@title:window", "Import a file from your bank")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [
            root.i18nContext.i18nc("@item:inlistbox a file type", "Bank files (*.xml *.sta *.ofx *.csv *.txt)"),
            root.i18nContext.i18nc("@item:inlistbox a file type", "All files (*)")
        ]
        onAccepted: root.startImport(accountId, selectedFile)
    }

    Component {
        id: importStatementsPage

        ImportStatementsPage {
            statementImport: statementImporter
            onChosen: root.replacePage(root.importStep())
        }
    }

    Component {
        id: csvSettingsPage

        CsvSettingsPage {
            statementImport: statementImporter
            onRead: root.replacePage(root.importStep())
            onCancelled: root.pageStack.pop()
        }
    }

    Component {
        id: importPreviewPage

        ImportPreviewPage {
            statementImport: statementImporter
            onImported: root.replacePage(importDonePage)
            onCancelled: root.pageStack.pop()
        }
    }

    Component {
        id: importDonePage

        ImportDonePage {
            statementImport: statementImporter
            register: accountRegister
            onDone: {
                statementImporter.cancel();
                root.pageStack.pop();
            }
            onCheckAgainstBankClicked: (accountId, bank, date) => root.pushPage(bankCheckPage, { accountId: accountId, initialBank: bank, initialDate: date })
        }
    }

    Component {
        id: welcomePage

        WelcomePage {
            firstStart: firstStartState
            onAddAccountClicked: root.addAccount(true)
            onStartFromFileClicked: root.chooseFileForNewBook()
        }
    }

    Component {
        id: problemPage

        BookProblemPage {
            session: bookSession
            onOpenAnotherClicked: root.chooseBookToOpen()
            onNewBookClicked: root.chooseNewBook()
            onStartFromFileClicked: root.chooseFileForNewBook()
            onBackClicked: {
                bookSession.dismissProblem();
                root.pageStack.pop();
            }
        }
    }

    Component {
        id: transactionPage

        TransactionPage {
            register: accountRegister
            onDone: firstRun ? root.showScreen("overview") : root.pageStack.pop()
            onCheckAgainstBankClicked: accountId => root.pushPage(bankCheckPage, { accountId: accountId, initialBank: "", initialDate: "" })
            // The Parts page takes this page's place, so saving goes back
            // to the list.
            onBreakDownClicked: (accountId, transactionId) => root.replacePage(partsPage, { accountId: accountId, transactionId: transactionId })
            // "Create a Rule from this transaction" (D-088): the Rule's
            // page replaces this one, which has nothing left to do.
            onCreateRuleClicked: (accountId, transactionId) => root.replacePage(rulePage, { initialForm: ruleBook.formFromTransaction(accountId, transactionId) })
        }
    }

    Component {
        id: partsPage

        PartsPage {
            register: accountRegister
            onDone: root.pageStack.pop()
        }
    }

    Component {
        id: overviewPage

        OverviewPage {
            overview: bookOverview
            onAddAccountClicked: root.addAccount(false)
            onAddTransactionClicked: root.addTransaction(false, "")
        }
    }

    Component {
        id: registerPage

        RegisterPage {
            register: accountRegister
            rules: ruleBook
            onAddTransactionClicked: accountId => root.addTransaction(false, accountId)
            onDetailsClicked: accountId => root.pushPage(accountDetailsPage, { accountId: accountId })
            onCreateRuleFromSuggestion: (payee, category) => root.pushPage(rulePage, { initialForm: ruleBook.formFromSuggestion(payee, category) })
            onImportClicked: accountId => root.importStatement(accountId)
            onGetTransactionsClicked: accountId => root.pushPage(bankFetchPage, { initialAccountId: accountId })
            onTransactionClicked: (accountId, transactionId) => root.openTransaction(accountId, transactionId)
            Component.onCompleted: bankReady = root.bankUsable()
        }
    }

    Component {
        id: accountsPage

        AccountsPage {
            accounts: accountList
            onAddAccountClicked: root.addAccount(false)
            onAccountClicked: accountId => root.showRegister(accountId)
            onGetTransactionsClicked: root.pushPage(bankFetchPage, { initialAccountId: "" })
            Component.onCompleted: bankReady = root.bankUsable()
        }
    }

    Component {
        id: newAccountPage

        NewAccountPage {
            accounts: accountList
            onAdded: firstRun ? root.addTransaction(true, "") : root.pageStack.pop()
        }
    }

    Component {
        id: accountDetailsPage

        AccountDetailsPage {
            accounts: accountList
            onCheckClicked: (bank, date) => root.pushPage(bankCheckPage, { accountId: accountId, initialBank: bank, initialDate: date })
            // Closed or opened again: back to the list, past the account's
            // transactions, which the details were opened from.
            onAccountClosedOrOpened: root.pageStack.pop(root.pageStack.get(Math.max(root.pageStack.depth - 3, 0)))
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
            onAddCategoryClicked: root.pushPage(categoryPage)
            onCategoryClicked: categoryId => {
                const row = categoryTree.view.rows.find(row => row.id === categoryId);
                root.pushPage(categoryPage, {
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
        id: rulesPage

        RulesPage {
            rules: ruleBook
            onAddRuleClicked: root.pushPage(rulePage)
            onRuleClicked: ruleId => root.pushPage(rulePage, { ruleId: ruleId })
            onTransactionClicked: (accountId, transactionId) => root.openTransaction(accountId, transactionId)
            onCreateFromSuggestion: (payee, category) => root.pushPage(rulePage, { initialForm: ruleBook.formFromSuggestion(payee, category) })
        }
    }

    Component {
        id: rulePage

        RulePage {
            rules: ruleBook
            onDone: root.pageStack.pop()
            onAddSubRuleClicked: ruleId => root.replacePage(rulePage, { parentId: ruleId })
        }
    }

    // A page that leaves the stack is deleted (D-161). The stack leaves a
    // page that was popped in a one-page window alive, hidden away to the
    // right of the visible page; it shows again as soon as the window is
    // made wider, over the page that is open (M4 Phase 2: "Done" on the bank
    // setup seemed not to close it). The delay lets the stack's own slide
    // out finish; if the stack deletes the page first, there is nothing left
    // to delete.
    Connections {
        target: root.pageStack

        function onPageRemoved(page: Item) {
            page.destroy(1000);
        }
    }

    Component {
        id: settingsPage

        SettingsPage {
            session: bookSession
            numberFormat: numberFormatSettings
            language: languageSettings
            ai: aiSwitch
            history: changeHistory
            bank: bankConnection
            onConnectBankClicked: root.pushPage(bankSetupPage)
            onChangeBankClicked: login => root.pushPage(bankSetupPage, { replacesLogin: login.id, startIban: login.iban })
            onOpenAnotherClicked: root.chooseBookToOpen()
            onNewBookClicked: root.chooseNewBook()
            onSaveCopyClicked: root.pushPage(exportPage)
            onBookFromFileClicked: root.chooseFileForNewBook()
            onShowHistoryClicked: root.pushPage(historyPage)
        }
    }

    // Getting the accounts' new transactions from the bank (M4.7, D-153):
    // the statement of an account is looked at like a file's.
    Component {
        id: bankFetchPage

        BankFetchPage {
            bank: bankConnection
            onLinked: root.screenChangedBook()
            onConnectBankClicked: root.pushPage(bankSetupPage)
            onLookAtClicked: accountId => root.startBankImport(accountId)
        }
    }

    Component {
        id: bankSetupPage

        BankSetupPage {
            bank: bankConnection
            onLinked: root.screenChangedBook()
            onDone: {
                root.pageStack.pop();
                const settings = root.pageStack.currentItem;
                if (settings && settings.refreshBanks !== undefined) {
                    settings.refreshBanks();
                }
            }
        }
    }

    Component {
        id: exportPage

        ExportPage {
            dataExport: dataExporter
            folderUrl: bookSession.folderUrl
        }
    }

    Component {
        id: bookFromFilePage

        BookFromFilePage {
            bookFromFile: bookFromFileState
            folderUrl: bookSession.folderUrl
            onCancelled: {
                bookFromFileState.cancel();
                root.pageStack.pop();
            }
        }
    }

    Component {
        id: bookFromFileDonePage

        BookFromFileDonePage {
            bookFromFile: bookFromFileState
            register: accountRegister
            onDoneClicked: {
                root.showBook();
                bookFromFileState.cancel();
            }
        }
    }

    Component {
        id: historyPage

        HistoryPage {
            history: changeHistory
        }
    }
}
