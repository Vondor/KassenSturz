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
import QtQuick.Controls as Controls
import QtQuick.Layouts
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
        "budgets": budgetsPage,
        "rules": rulesPage,
        "statistics": statisticsPage,
        "settings": settingsPage,
    })
    // The window width, in grid units, from which the chat sidebar sits
    // beside the page: the sidebar's own 22 and 28 for the page.
    readonly property int chatBesideMinWidth: 50
    // The menu stays beside the page from this width (in grid units). Not
    // `wideScreen`, which flipped after a review of AI changes and turned
    // the menu into an overlay (#197).
    readonly property int menuBesideMinWidth: 50
    // The left menu's width (#205): it doesn't grow with its longest text, which
    // is cut with "…", so a long "Undo: …" can't squeeze the page. It starts at
    // `menuStandardWidth`, the user can drag its edge between `menuMinWidth` and
    // `menuMaxWidth`, and the width is kept between sessions.
    readonly property int menuStandardWidth: Kirigami.Units.gridUnit * 12
    readonly property int menuMinWidth: Kirigami.Units.gridUnit * 9
    readonly property int menuMaxWidth: Math.min(Kirigami.Units.gridUnit * 28, Math.round(root.width / 2))
    property int menuWidth: windowState.menuWidth > 0 ? windowState.menuWidth : menuStandardWidth
    // The chat sidebar's width the same way (#218): 22 grid units to begin
    // with, the user drags its left edge, and the width is kept.
    readonly property int chatStandardWidth: Kirigami.Units.gridUnit * 22
    readonly property int chatMinWidth: Kirigami.Units.gridUnit * 16
    readonly property int chatMaxWidth: Math.max(root.chatMinWidth, Math.min(Kirigami.Units.gridUnit * 50, Math.round(root.width * 2 / 3)))
    property int chatWidth: windowState.chatWidth > 0 ? windowState.chatWidth : chatStandardWidth
    // Whether the first-run steps are done, so the side menu is offered.
    readonly property bool started: bookSession.open && firstStartState.hasAccount

    // The app's name, which isn't translated, and in the developer mode a
    // marker that stays as long as it runs (D-227).
    title: developerMode.on
        ? "KassenSturz – " + i18nContext.i18nc("@title:window marker that KassenSturz was started in its developer mode", "developer mode")
        : "KassenSturz"
    // The size, place and maximized state of the last close come back (the
    // fallbacks are for the first start, or a settings file that can't be read).
    width: windowState.known ? windowState.width : Kirigami.Units.gridUnit * 40
    height: windowState.known ? windowState.height : Kirigami.Units.gridUnit * 38
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

    // The views saved in the Statistics workbench (M6a.7). A change is in the
    // history, so the notice offers Undo like for any other screen.
    // The Overview's dashboard and its grid editor (M6b.3).
    DashboardLayout {
        id: dashboardLayout
    }

    GridNames {
        id: layoutNames
    }

    SavedViews {
        id: savedViews

        onChanged: root.screenChangedBook()
    }

    // The accounts and categories screens (M3.3). A change in one of them
    // is read again by all the others.
    AccountList {
        id: accountList

        onChanged: root.screenChangedBook()
        onAdded: root.accountAdded()
    }

    CategoryTree {
        id: categoryTree

        onChanged: root.screenChangedBook()
    }

    // My groups and the hide list of the charts (M6b.5c).
    CategoryGroups {
        id: categoryGroups

        onChanged: root.screenChangedBook()
    }

    // The budgets and how much of each is spent (M6c.5).
    BudgetBook {
        id: budgetBook

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

    // While the switch is on, the history names a confirmed set after the
    // AI that prepared it (D-198).
    ChangeNames {
        id: changeNames

        appliedName: aiSwitch.enabled ? chatPanel.appliedName : ""
    }

    // The bank connection (M4.6): one per app, with the dialogs for what a
    // bank asks while it is contacted.
    BankConnection {
        id: bankConnection
    }

    BankDialogs {
        bank: bankConnection
    }

    // The developer mode (D-227): on only when KassenSturz was started from
    // a terminal for it. It is told every screen that comes up, which the
    // chat's problem reports name too (D-228).
    DeveloperMode {
        id: developerMode
    }

    Connections {
        target: root.pageStack

        function onCurrentItemChanged() {
            developerMode.screenShown(String(root.pageStack.currentItem));
        }
    }

    // The one AI switch (D-013): every AI element in the screens hides
    // behind its `enabled`.
    AiSwitch {
        id: aiSwitch
    }

    // The provider and the consent (M5.7); the screens that use it hide
    // behind the switch.
    AiSettings {
        id: aiSettings
    }

    // The conversation and the changes it prepares (M5.8). A confirmed set
    // changed the book, so all the screens read it again.
    AiChat {
        id: aiChat

        onChanged: root.bookChanged()
    }

    // Closing the sidebar and forgetting the conversation when the switch
    // goes off: nothing of it stays or runs (D-013).
    Connections {
        target: aiSwitch

        function onEnabledChanged() {
            if (!aiSwitch.enabled) {
                root.attachChat(false);
                chatDrawer.close();
                chatPanel.startOver();
            }
        }
    }

    // When the overview reminds the user to get new transactions (D-180).
    FetchGapSettings {
        id: fetchGapSettings
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
    WindowState {
        id: windowState
    }

    // The window's size and place while it isn't maximized, which is what the
    // next start gets back before it maximizes again. Recorded a moment after
    // the last change, so the size of a maximize in progress never counts.
    property rect normalGeometry: Qt.rect(x, y, width, height)

    Timer {
        id: geometryTimer
        interval: 300
        onTriggered: {
            if (root.visibility === Window.Windowed) {
                root.normalGeometry = Qt.rect(root.x, root.y, root.width, root.height);
            }
        }
    }
    onWidthChanged: geometryTimer.restart()
    onHeightChanged: geometryTimer.restart()
    onXChanged: geometryTimer.restart()
    onYChanged: geometryTimer.restart()

    onClosing: {
        // The chat window must not keep the app running.
        chatWindow.hide();
        const maximized = root.visibility === Window.Maximized;
        const g = maximized ? root.normalGeometry : Qt.rect(root.x, root.y, root.width, root.height);
        windowState.save(g.width, g.height, g.x, g.y, maximized);
    }

    Component.onCompleted: {
        if (windowState.known) {
            // Only where the window is still on the screen; on Wayland the
            // desktop decides the place and ignores this.
            const fits = windowState.x >= 0 && windowState.y >= 0
                && windowState.x + 100 <= Screen.desktopAvailableWidth
                && windowState.y + 100 <= Screen.desktopAvailableHeight;
            if (fits) {
                root.x = windowState.x;
                root.y = windowState.y;
            }
            if (windowState.maximized) {
                root.showMaximized();
            }
        }
        bookSession.openLast(i18nContext.i18nc("@item the file name of the app's own book, without its extension", "Household"))
    }

    // Shows the book that was just opened: its first-run steps if it has no
    // account yet, otherwise the overview.
    function showBook() {
        // A conversation is about one book.
        chatPanel.startOver();
        // A layout session belongs to the book it was started on.
        dashboardLayout.cancel();
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
        categoryGroups.refresh();
        budgetBook.refresh();
        ruleBook.refresh();
        accountRegister.refresh();
        savedViews.refresh();
        dashboardLayout.refresh();
        changeHistory.refresh();
    }

    // An account was added: only what lists accounts or counts money reads
    // the book again. The register, the categories and their groups, the
    // saved views and the layout show nothing of a new account, and reading
    // them again made saving it wait (M6-B8).
    function accountAdded() {
        firstStartState.refresh();
        bookOverview.refresh();
        ruleBook.refresh();
        changeHistory.refresh();
        offerUndo();
    }

    // A screen changed the book: every screen reads it again, and the user
    // can undo the change from the notice (M3.8, D-086).
    function screenChangedBook() {
        bookChanged();
        offerUndo();
    }

    function offerUndo() {
        if (started && changeHistory.undoChange?.action !== undefined) {
            showPassiveNotification(changeNames.name(changeHistory.undoChange.action), 8000, i18nContext.i18nc("@action:button take the last change back", "Undo"), () => root.undoLast());
        }
    }

    // The page that is arranging the layout, if the current one is: its Undo
    // and Redo take back its own edits, not the book's changes (D-288).
    readonly property var arrangingPage: pageStack.currentItem && pageStack.currentItem.layoutEditing === true ? pageStack.currentItem : null

    function undoLast() {
        if (arrangingPage) {
            arrangingPage.undoLayout();
            return;
        }
        afterStep(changeHistory.undo(), false);
    }

    function redoLast() {
        if (arrangingPage) {
            arrangingPage.redoLayout();
            return;
        }
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

    // The review page of the AI's changes `setId`, if it is in the stack.
    function changeReviewPageOf(setId: string): Item {
        for (let i = pageStack.depth - 1; i >= 0; --i) {
            const page = pageStack.get(i);
            if (page && page.shows !== undefined && page.setId === setId) {
                return page;
            }
        }
        return null;
    }

    // Opens the review of the AI's changes `setId`. A review that is open
    // already is shown again instead of a second one being stacked on it, so
    // that closing it leads back to where the user came from (#199).
    function showChangeReview(setId: string) {
        const open = changeReviewPageOf(setId);
        if (open) {
            pageStack.pop(open);
        } else {
            pushPage(aiChangeReviewPage, { setId: setId });
        }
    }

    // Closes the review of `setId`, and any other review of the same changes
    // under it, which would ask for a confirmation that was given (#199).
    function closeChangeReview(setId: string) {
        pageStack.pop();
        while (pageStack.depth > 1 && changeReviewPageOf(setId) === pageStack.currentItem) {
            pageStack.pop();
        }
    }

    // Makes a page for the page stack (#77). Given the component, Kirigami
    // makes the page under an object that isn't drawn, and Qt warns that the
    // page "was not placed in the graphics scene" until the stack takes it.
    // Made under the window instead, it is taken out of the window again at
    // once. The window, as its owner, keeps it from being collected as
    // garbage while it is in the stack; `onPageRemoved` below deletes it
    // when it leaves, which the stack does not always do (D-161).
    function makePage(component: Component, properties: var): Item {
        // The developer mode times how long a screen takes from here.
        developerMode.screenRequested();
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

    // Shows the page the AI asked to open (D-265): on top of the page that is
    // open, so Back leads to where the user was and nothing they have typed
    // is thrown away. `name` is one of the fixed names of the `open_page`
    // function, `id` the account or Rule it is about. A Rule that is gone,
    // or a name that isn't one of these, shows nothing.
    function showPageForAi(name: string, id: string) {
        if (name === "account") {
            showRegister(id);
        } else if (screens[name] !== undefined && name !== "settings") {
            const page = pushPage(screens[name]);
            if (name === "rules" && id !== "") {
                page.selectedId = id;
            }
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

    header: Item {
        id: headerBox

        // The header is not stretched by the window, so without this a long
        // banner is wider than the window and wraps nowhere.
        width: root.width
        implicitHeight: headerColumn.implicitHeight

        ColumnLayout {
            id: headerColumn

            // Kirigami moves the header left by the width of the open menu, and
            // the banners with it. This puts them back at the window's edge (#198).
            x: -headerBox.x
            width: root.width
            spacing: 0

            // The developer mode shows this on every screen for as long as it
            // runs, and it can't be closed (D-227).
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                position: Kirigami.InlineMessage.Position.Header
                type: Kirigami.MessageType.Error
                visible: developerMode.on
                text: root.i18nContext.i18nc("@info", "Developer mode: KassenSturz is writing a log of what it does on this computer, such as which screens open and which errors happen, and a full record of the chat: your questions, everything sent to the model and its replies, including data from your book. Your privacy can't be guaranteed while this mode runs. To end it, close KassenSturz and start it the usual way.")
                actions: [
                    Kirigami.Action {
                        icon.name: "folder-open"
                        text: root.i18nContext.i18nc("@action:button opens the folder of the developer mode's log", "Show the log")
                        onTriggered: Qt.openUrlExternally(developerMode.folderUrl())
                    },
                    Kirigami.Action {
                        icon.name: "edit-delete"
                        text: root.i18nContext.i18nc("@action:button deletes the developer mode's log", "Delete the log")
                        onTriggered: developerMode.clearLog()
                    }
                ]
            }

            // A book on a network drive opens, with this warning on every
            // screen (D-038).
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                position: Kirigami.InlineMessage.Position.Header
                type: Kirigami.MessageType.Warning
                visible: bookSession.open && bookSession.networkDrive
                text: root.i18nContext.i18nc("@info", "This book is on a network drive. Network drives can damage a book, because they don't save changes as reliably as this computer's own drive. Keep the book on this computer; a sync folder, such as Nextcloud's, is fine.")
            }
        }
    }

    globalDrawer: Kirigami.GlobalDrawer {
        enabled: root.started
        modal: root.width < Kirigami.Units.gridUnit * root.menuBesideMinWidth || !root.started
        width: Math.max(root.menuMinWidth, Math.min(root.menuWidth, root.menuMaxWidth))
        drawerOpen: !modal
        // Also visible in a wide window: if the drawer was closed there (see
        // onModalChanged), the handle is the way back to the menu (M5-B19).
        handleVisible: root.started
        // Opening a narrow window closes the drawer by hand, which ends the
        // binding above; without this the menu stayed hidden after maximizing.
        onModalChanged: drawerOpen = !modal

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
                icon.name: "wallet-open"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Budgets")
                onTriggered: root.showScreen("budgets")
            },
            Kirigami.Action {
                icon.name: "view-filter"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Rules")
                onTriggered: root.showScreen("rules")
            },
            Kirigami.Action {
                icon.name: "office-chart-bar"
                text: root.i18nContext.i18nc("@action:inmenu a screen", "Statistic Builder")
                onTriggered: root.showScreen("statistics")
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
                enabled: root.started && (root.arrangingPage ? root.arrangingPage.canUndoLayout : changeHistory.undoChange?.action !== undefined)
                text: enabled
                    ? root.i18nContext.i18nc("@action:inmenu %1 is what was done, e.g. Added a transaction", "Undo: %1", root.arrangingPage ? layoutNames.editName(root.arrangingPage.undoLayoutWord) : changeNames.describe(changeHistory.undoChange))
                    : root.i18nContext.i18nc("@action:inmenu there is nothing to undo", "Undo")
                // The menu cuts a long text, so the tooltip has all of it (#205).
                tooltip: text
                shortcut: StandardKey.Undo
                onTriggered: root.undoLast()
            },
            Kirigami.Action {
                icon.name: "edit-redo"
                enabled: root.started && (root.arrangingPage ? root.arrangingPage.canRedoLayout : changeHistory.redoChange?.action !== undefined)
                text: enabled
                    ? root.i18nContext.i18nc("@action:inmenu %1 is what was done, e.g. Added a transaction", "Redo: %1", root.arrangingPage ? layoutNames.editName(root.arrangingPage.redoLayoutWord) : changeNames.describe(changeHistory.redoChange))
                    : root.i18nContext.i18nc("@action:inmenu there is nothing to redo", "Redo")
                tooltip: text
                shortcut: StandardKey.Redo
                onTriggered: root.redoLast()
            }
        ]
    }

    // Opens and closes the chat sidebar from the keyboard. The menu has no entry
    // for it any more: the sidebar's handle is the way to it (#207). Dead while
    // the switch is off (D-013).
    Shortcut {
        id: chatShortcut

        sequence: "Ctrl+Shift+A"
        enabled: aiSwitch.enabled && root.started
        onActivated: chatDrawer.drawerOpen = !chatDrawer.drawerOpen
    }

    // The edge of the open menu, which the user drags to change its width
    // (#205). Over the page below the banners; the width is kept when the
    // button is let go.
    Item {
        id: menuGrip

        parent: Controls.Overlay.overlay
        visible: !root.globalDrawer.modal && root.globalDrawer.drawerOpen
        x: root.globalDrawer.width * root.globalDrawer.position - width / 2
        y: root.header ? root.header.height : 0
        width: Kirigami.Units.smallSpacing * 2
        height: parent ? parent.height - y : 0
        z: 1000

        Accessible.ignored: true

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.SizeHorCursor
            preventStealing: true
            onPositionChanged: mouse => {
                if (pressed) {
                    root.menuWidth = Math.max(root.menuMinWidth, Math.min(mapToItem(null, mouse.x, 0).x, root.menuMaxWidth));
                }
            }
            onReleased: windowState.saveMenuWidth(root.menuWidth)
        }
    }

    // The strip on the chat sidebar's left edge that the user drags to change
    // its width (#218), like the menu's.
    Item {
        id: chatGrip

        parent: Controls.Overlay.overlay
        visible: root.started && !root.chatDetached && chatDrawer.drawerOpen && aiSwitch.enabled
        x: root.width - chatDrawer.width * chatDrawer.position - width / 2
        y: root.header ? root.header.height : 0
        width: Kirigami.Units.smallSpacing * 2
        height: parent ? parent.height - y : 0
        z: 1000

        Accessible.ignored: true

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.SizeHorCursor
            preventStealing: true
            onPositionChanged: mouse => {
                if (pressed) {
                    root.chatWidth = Math.max(root.chatMinWidth, Math.min(root.width - mapToItem(null, mouse.x, 0).x, root.chatMaxWidth));
                }
            }
            onReleased: windowState.saveChatWidth(root.chatWidth)
        }
    }

    // True while the chat is in its own window (#209).
    property bool chatDetached: false

    function detachChat() {
        chatPanel.parent = chatWindowHolder;
        chatDetached = true;
        chatDrawer.close();
        showChatWindow();
    }

    // Puts the chat back into the sidebar. With `reopen` the sidebar opens
    // and the main window comes to the front, which is what the button
    // "Move back to the sidebar" is for; closing the chat window, or the AI
    // switch going off, leaves the sidebar closed.
    function attachChat(reopen) {
        chatPanel.parent = chatHolder;
        chatDetached = false;
        chatWindow.hide();
        if (reopen) {
            chatDrawer.open();
            root.raise();
            root.requestActivate();
        }
    }

    function showChatWindow() {
        chatWindow.show();
        chatWindow.raise();
        chatWindow.requestActivate();
    }

    function raiseMainWindow() {
        if (chatDetached) {
            root.raise();
            root.requestActivate();
        }
    }

    // The chat sidebar on the right (M5.8, D-047, D-210): opened and closed
    // with its handle or the menu entry. While the switch is off neither is
    // shown (D-013). Side by side with the page in a window wide enough for
    // both (`chatBesideMinWidth`), over it in a narrower one. It doesn't use
    // `wideScreen`, which also counts the width of the menu: that width
    // changes with the menu's texts, and when `wideScreen` flipped, both
    // sidebars closed by themselves while the user reviewed AI changes
    // (M5-B36).
    contextDrawer: Kirigami.OverlayDrawer {
        id: chatDrawer

        edge: Qt.RightEdge
        enabled: aiSwitch.enabled && root.started
        modal: root.width < Kirigami.Units.gridUnit * root.chatBesideMinWidth
        handleVisible: aiSwitch.enabled && root.started
        handleClosedIcon.source: "dialog-messages"
        handleOpenIcon.source: "go-next"
        width: Math.max(root.chatMinWidth, Math.min(root.chatWidth, root.chatMaxWidth))
        drawerOpen: false
        topPadding: 0
        bottomPadding: 0
        leftPadding: 0
        rightPadding: 0
        onDrawerOpenChanged: {
            if (drawerOpen && root.chatDetached) {
                // The chat lives in its window: bring that to the front.
                close();
                root.showChatWindow();
            } else if (drawerOpen) {
                chatPanel.refresh();
            }
        }

        contentItem: Item {
            id: chatHolder

            AiChatPanel {
                id: chatPanel

                anchors.fill: parent
                ai: aiSwitch
                chat: aiChat
                settings: aiSettings
                detached: root.chatDetached
                onSettingsClicked: {
                    if (chatDrawer.modal) {
                        chatDrawer.close();
                    }
                    root.pushPage(aiSettingsPage);
                    root.raiseMainWindow();
                }
                onReviewChangesClicked: setId => {
                    if (chatDrawer.modal) {
                        chatDrawer.close();
                    }
                    root.showChangeReview(setId);
                    root.raiseMainWindow();
                }
                onStatisticBuilderRequested: (title, component, period, dataset) => {
                    if (chatDrawer.modal) {
                        chatDrawer.close();
                    }
                    root.pushPage(statisticsPage, {
                        initialName: title,
                        initialComponent: component,
                        initialPeriod: period,
                        initialDataset: dataset
                    });
                    root.raiseMainWindow();
                }
                onCloseClicked: chatDrawer.close()
                onDetachClicked: root.chatDetached ? root.attachChat(true) : root.detachChat()
                onChangesArrived: chatDrawer.open()
                onOpenPageRequested: (name, id) => {
                    if (chatDrawer.modal) {
                        chatDrawer.close();
                    }
                    root.showPageForAi(name, id);
                    root.raiseMainWindow();
                }
            }
        }
    }

    // The chat in a window of its own (#209, D-250). The same panel moves
    // between the sidebar and this window, so the conversation, the
    // unsent text and the open tab stay as they are. Closing the window puts
    // the panel back into the sidebar, which stays closed. The window opens
    // where it was left (not remembered between sessions).
    Controls.ApplicationWindow {
        id: chatWindow

        width: Kirigami.Units.gridUnit * 26
        height: Kirigami.Units.gridUnit * 36
        minimumWidth: Kirigami.Units.gridUnit * 18
        minimumHeight: Kirigami.Units.gridUnit * 20
        visible: false
        title: chatPanel.windowTitle
        onClosing: root.attachChat(false)

        Item {
            id: chatWindowHolder

            anchors.fill: parent
        }
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
            dashboard: dashboardLayout
            savedSource: savedViews
            groupsSource: categoryGroups
            onAddAccountClicked: root.addAccount(false)
            onAddTransactionClicked: root.addTransaction(false, "")
            onGetTransactionsClicked: accountId => root.pushPage(bankFetchPage, { initialAccountId: accountId })
            onSetBudgetClicked: root.pushPage(budgetPage, { categoryId: "" })
        }
    }

    Component {
        id: statisticsPage

        StatisticsPage {
            saved: savedViews
            groupsSource: categoryGroups
        }
    }

    Component {
        id: registerPage

        RegisterPage {
            register: accountRegister
            rules: ruleBook
            suggestionsShown: aiSwitch.enabled && aiChat.available && aiSettings.allows("AllowSortTransactions") && !aiSettings.locked("LockCategory")
            onSuggestCategoriesClicked: accountId => {
                chatDrawer.open();
                chatPanel.askFeature("categories", accountId);
            }
            onAddTransactionClicked: accountId => root.addTransaction(false, accountId)
            onDetailsClicked: accountId => root.pushPage(accountDetailsPage, { accountId: accountId })
            onCreateRuleFromSuggestion: (payee, category) => root.pushPage(rulePage, { initialForm: ruleBook.formFromSuggestion(payee, category) })
            onImportClicked: accountId => root.importStatement(accountId)
            onGetTransactionsClicked: accountId => root.pushPage(bankFetchPage, { initialAccountId: accountId })
            onTransactionClicked: (accountId, transactionId) => root.openTransaction(accountId, transactionId)
            onReady: developerMode.screenReady("RegisterPage")
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
            onGroupsClicked: root.pushPage(categoryGroupsPage)
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
        id: categoryGroupsPage

        CategoryGroupsPage {
            groups: categoryGroups
            onAddGroupClicked: root.pushPage(categoryGroupPage)
            onGroupClicked: groupId => {
                const group = categoryGroups.view.groups.find(group => group.id === groupId);
                root.pushPage(categoryGroupPage, {
                    groupId: group.id,
                    groupName: group.name,
                    income: group.income,
                    memberIds: group.members.map(member => member.id),
                });
            }
        }
    }

    Component {
        id: categoryGroupPage

        CategoryGroupPage {
            groups: categoryGroups
            onDone: root.pageStack.pop()
        }
    }

    Component {
        id: categoryPage

        CategoryPage {
            categories: categoryTree
            onDone: root.pageStack.pop()
            onSetBudgetClicked: categoryId => root.pushPage(budgetPage, { categoryId: categoryId })
        }
    }

    Component {
        id: budgetsPage

        BudgetsPage {
            budgets: budgetBook
            onAddBudgetClicked: root.pushPage(budgetPage)
            onChangeBudgetClicked: budgetId => root.pushPage(budgetPage, { budgetId: budgetId })
        }
    }

    Component {
        id: budgetPage

        BudgetPage {
            budgets: budgetBook
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
            fetchGap: fetchGapSettings
            onConnectBankClicked: root.pushPage(bankSetupPage)
            onChangeBankClicked: login => root.pushPage(bankSetupPage, { replacesLogin: login.id, startIban: login.iban })
            onOpenAnotherClicked: root.chooseBookToOpen()
            onNewBookClicked: root.chooseNewBook()
            onSaveCopyClicked: root.pushPage(exportPage)
            onBookFromFileClicked: root.chooseFileForNewBook()
            onShowHistoryClicked: root.pushPage(historyPage)
            onAiSettingsClicked: root.pushPage(aiSettingsPage)
        }
    }

    Component {
        id: aiSettingsPage

        AiSettingsPage {
            ai: aiSwitch
            settings: aiSettings
        }
    }

    // Reading the changes the AI prepared, and confirming them (M5.8).
    Component {
        id: aiChangeReviewPage

        AiChangeReviewPage {
            ai: aiSwitch
            chat: aiChat
            onApplied: setId => chatPanel.markHandled(setId, true)
            onDiscarded: setId => {
                chatPanel.markHandled(setId, false);
                pageStack.pop();
            }
            onUndoClicked: {
                const result = changeHistory.undo();
                root.afterStep(result, false);
                if (result.code === undefined) {
                    aiChat.changesUndone();
                }
            }
            onDoneClicked: root.closeChangeReview(setId)
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
