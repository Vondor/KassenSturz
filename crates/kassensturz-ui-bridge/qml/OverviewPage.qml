// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The overview (M3.9): how the user is doing on one page: what all the
// accounts hold together (the net worth card), and one month's money in and money out with where
// it came from and where it went. The accounts and their transactions are on
// their own screens (round 1). Money moved between the user's own accounts
// counts as neither (D-021). Every amount, total and order comes from Rust,
// per currency (D-065); this page only shows text. The month's figures and
// the chart of where the money went, its five largest categories and the last 12
// months' line are report components (M6a.8, Q-215). Since M6b.3 they sit on
// the dashboard grid (`DashboardGrid`), which the user can arrange: "Edit
// layout" starts a session with its own undo, Done keeps it as one change in
// the history. Nothing on it is fixed (D-452): since M6d.8 the month header
// (`PeriodHeader`) and the filter bar (`FilterBar`) are widgets on the grid
// too, like the fetch-gap warnings, the note about transfers and the
// separator (D-327, D-328, D-334, D-474).
// Since M6d.1 the page shows one of the book's dashboards (D-436): tabs once
// there are two (`DashboardTabs`), the dashboard menu (`DashboardMenu`) and
// *New dashboard…* (`NewDashboardDialog`); the book opens at its default
// dashboard (D-448).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property BookOverview overview
    required property DashboardLayout dashboard
    // A `SavedViews`, for adding a view saved in Statistics to the page.
    property var savedSource: null
    readonly property var savedList: ((savedSource?.view ?? ({})).views ?? []).filter(row => row.usable)
    // A `CategoryGroups`, for picking My groups and categories in the panel.
    property var groupsSource: null
    // The app's `BankConnection` and whether bank connections can be used,
    // for the Update all accounts widget (M6d.7).
    property var bank: null
    property bool bankReady: false
    readonly property var view: overview.view ?? ({})

    signal addAccountClicked()
    signal addTransactionClicked()
    signal getTransactionsClicked(string accountId)
    signal setBudgetClicked()
    // The user chose a dashboard file to add (M6d.4): the app shows what it
    // holds before anything is added.
    signal dashboardFileChosen(url file)
    signal transactionClicked(string accountId, string transactionId)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // `amount` as the figure of the net worth card: a debt reads "You owe",
    // not a minus sign (D-067).
    GridNames {
        id: names
    }

    // The grid editor (M6b.3).
    readonly property bool layoutEditing: dashboard.view.editing ?? false
    readonly property bool canUndoLayout: dashboard.view.canUndo ?? false
    readonly property bool canRedoLayout: dashboard.view.canRedo ?? false
    readonly property string undoLayoutWord: dashboard.view.undoEdit ?? ""
    readonly property string redoLayoutWord: dashboard.view.redoEdit ?? ""

    // The editor's own undo and redo, which the keys and the menu use while
    // the layout is being arranged (D-288).
    function undoLayout() {
        dashboard.undo();
    }

    function redoLayout() {
        dashboard.redo();
    }

    // Done keeps the session in the book; Cancel throws it away, after asking
    // when there is something to lose.
    function finishLayout() {
        const result = dashboard.done();
        if (result.code !== undefined) {
            layoutError.result = result;
        }
    }

    function cancelLayout() {
        if (dashboard.view.hasChanges ?? false) {
            discardDialog.open();
        } else {
            dashboard.cancel();
        }
    }

    // The component the properties panel is changing, by ID.
    property string editedId: ""
    property bool loadingPanel: false

    function openProperties(id: string) {
        const entry = grid.byId[id];
        if (entry === undefined || entry.unknown) {
            return;
        }
        loadingPanel = true;
        panelModel.load(entry.subject.json);
        loadingPanel = false;
        editedId = id;
        propertiesSheet.open();
    }

    // Adds a copy of a saved view; it keeps its figures and period, and a
    // later change to the saved view doesn't reach it.
    function addSaved(id: string) {
        const opened = savedSource.open(id);
        if (opened.code !== undefined) {
            layoutError.result = opened;
            return;
        }
        const result = dashboard.addView(opened.component, opened.period);
        if (result.code !== undefined) {
            layoutError.result = result;
        } else {
            Qt.callLater(() => grid.select(result.id));
        }
    }

    AddMenu {
        id: addMenu

        savedList: page.savedList
        bankReady: page.bankReady
        pageKinds: (page.dashboard.view.items ?? []).map(item => item.type)
        onKindChosen: (kind, dataset) => grid.add(kind, dataset)
        onSavedChosen: id => page.addSaved(id)
    }

    ComponentProperties {
        id: panelModel
    }

    // What the panel changes goes to the editor, which checks it again and
    // keeps it as one edit that Undo takes back.
    Connections {
        target: panelModel

        function onViewChanged() {
            if (page.loadingPanel || page.editedId === "" || panelModel.view.json === undefined) {
                return;
            }
            const result = page.dashboard.setComponent(page.editedId, panelModel.view.json);
            if (result.code !== undefined) {
                layoutError.result = result;
            }
        }
    }

    // The part of the grid that is on screen, for the grid to make only what
    // is seen (M6-B30).
    function updateGridView() {
        const scroller = page.flickable;
        if (!scroller || !grid) {
            return;
        }
        const top = grid.mapToItem(scroller.contentItem, 0, 0).y;
        grid.viewTop = scroller.contentY - top;
        grid.viewBottom = grid.viewTop + scroller.height;
    }

    Connections {
        target: page.flickable

        function onContentYChanged() {
            page.updateGridView();
        }

        function onHeightChanged() {
            page.updateGridView();
        }
    }

    // Going to another screen and back starts from the current month again.
    Component.onCompleted: {
        overview.showThisMonth();
        dashboard.refresh();
        updateGridView();
    }

    // A page without the period header shows this month (M6d.8, D-475):
    // nothing on it could go back to this month otherwise.
    readonly property bool hasPeriodHeader: (dashboard.view.items ?? []).some(item => item.type === "page.period")
    onHasPeriodHeaderChanged: {
        if (!hasPeriodHeader) {
            overview.showThisMonth();
        }
    }

    readonly property int dashboardCount: (dashboard.view.dashboards ?? []).length
    // Where the file chooser starts (the book's folder).
    property url folderUrl
    // The book's ID of the dashboard shown, empty for the built-in Overview.
    readonly property string shownDashboardId: ((dashboard.view.dashboards ?? []).find(shown => shown.shown) ?? {}).id ?? ""
    readonly property string dashboardName: dashboard.view.name ?? i18nContext.i18nc("@title", "Overview")

    title: layoutEditing ? i18nContext.i18nc("@title %1 is the name of the dashboard shown", "Arrange %1", dashboardName) : dashboardName

    header: FormError {
        id: layoutError
    }

    actions: [
        // With one dashboard the page looks as before, with this one entry
        // more; with two or more it is in the dashboard menu (D-438).
        Kirigami.Action {
            visible: !page.layoutEditing && page.dashboardCount < 2
            displayHint: Kirigami.DisplayHint.AlwaysHide
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:inmenu", "New dashboard…")
            onTriggered: {
                ActionLog.taken("OverviewPage_newDashboard")
                newDashboardDialog.openNew()
            }
        },
        Kirigami.Action {
            visible: !page.layoutEditing && page.dashboardCount < 2
            displayHint: Kirigami.DisplayHint.AlwaysHide
            icon.name: "document-open"
            text: page.i18nContext.i18nc("@action:inmenu add a dashboard that someone saved to a file", "Add from a file…")
            onTriggered: {
                ActionLog.taken("OverviewPage_addFromFile")
                dashboardFileDialog.open()
            }
        },
        DashboardMenu {
            visible: !page.layoutEditing && page.dashboardCount > 1
            layout: page.dashboard
            onNewRequested: newDashboardDialog.openNew()
            onRenameRequested: newDashboardDialog.openRename()
            onDeleteRequested: deleteDashboardDialog.open()
            onSaveRequested: saveDashboardDialog.open()
            onAddFromFileRequested: dashboardFileDialog.open()
            onFailed: result => layoutError.result = result
        },
        // With one dashboard, saving it is here (D-443).
        Kirigami.Action {
            visible: !page.layoutEditing && page.dashboardCount < 2
            displayHint: Kirigami.DisplayHint.AlwaysHide
            icon.name: "document-save-as"
            text: page.i18nContext.i18nc("@action:inmenu save the dashboard shown as a file", "Save to a file…")
            onTriggered: {
                ActionLog.taken("OverviewPage_saveToFile")
                saveDashboardDialog.open()
            }
        },
        Kirigami.Action {
            visible: !page.layoutEditing
            icon.name: "view-grid"
            text: page.i18nContext.i18nc("@action:button change where things are on this page", "Edit layout")
            onTriggered: {
                ActionLog.taken("OverviewPage_editLayout")
                page.dashboard.edit()
            }
        },
        Kirigami.Action {
            visible: !page.layoutEditing
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add transaction")
            onTriggered: {
                ActionLog.taken("OverviewPage_addTransaction")
                page.addTransactionClicked()
            }
        },
        Kirigami.Action {
            visible: !page.layoutEditing
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add account")
            onTriggered: {
                ActionLog.taken("OverviewPage_addAccount")
                page.addAccountClicked()
            }
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "dialog-ok-apply"
            text: page.i18nContext.i18nc("@action:button keep the changes to the layout", "Done")
            onTriggered: {
                ActionLog.taken("OverviewPage_done")
                page.finishLayout()
            }
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "dialog-cancel"
            text: page.i18nContext.i18nc("@action:button throw away the changes to the layout", "Cancel")
            onTriggered: {
                ActionLog.taken("OverviewPage_cancel")
                page.cancelLayout()
            }
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button add a chart, table or figure to the layout", "Add to the page")
            onTriggered: {
                ActionLog.taken("OverviewPage_addPage")
                addMenu.popup()
            }
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "edit-undo"
            enabled: page.canUndoLayout
            text: enabled
                ? page.i18nContext.i18nc("@action:button %1 is what was done to the layout", "Undo: %1", names.editName(page.undoLayoutWord))
                : page.i18nContext.i18nc("@action:button there is nothing to undo", "Undo")
            tooltip: text
            onTriggered: {
                ActionLog.taken("OverviewPage_undoLayout")
                page.undoLayout()
            }
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "edit-redo"
            enabled: page.canRedoLayout
            text: enabled
                ? page.i18nContext.i18nc("@action:button %1 is what was done to the layout", "Redo: %1", names.editName(page.redoLayoutWord))
                : page.i18nContext.i18nc("@action:button there is nothing to make again", "Redo")
            tooltip: text
            onTriggered: {
                ActionLog.taken("OverviewPage_redoLayout")
                page.redoLayout()
            }
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "view-preview"
            checkable: true
            checked: grid.preview
            text: page.i18nContext.i18nc("@action:button hide the grid lines and the toolbars", "Preview")
            onTriggered: {
                ActionLog.taken("OverviewPage_preview")
                grid.preview = checked
            }
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "document-save-as"
            text: page.i18nContext.i18nc("@action:button keep what is on the page now as another dashboard", "Save as a new dashboard…")
            onTriggered: {
                ActionLog.taken("OverviewPage_saveNewDashboard")
                layoutName.text = "";
                saveLayoutDialog.open()
            }
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "document-revert"
            enabled: !(page.dashboard.view.builtIn ?? true)
            text: page.i18nContext.i18nc("@action:button put the layout KassenSturz comes with back", "Back to the built-in layout")
            onTriggered: {
                ActionLog.taken("OverviewPage_backBuiltLayout")
                resetDialog.open()
            }
        }
    ]

    Kirigami.PromptDialog {
        id: discardDialog

        title: page.i18nContext.i18nc("@title:window", "Throw away your changes to the layout?")
        subtitle: page.i18nContext.i18nc("@info", "The Overview goes back to how it was before you started. Nothing else changes.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Throw away the changes")
                icon.name: "edit-delete"
                onTriggered: {
                    ActionLog.taken("OverviewPage_throwAwayChanges")
                    discardDialog.close();
                    page.dashboard.cancel();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button keep arranging the layout", "Keep arranging")
                icon.name: "go-previous"
                onTriggered: {
                    ActionLog.taken("OverviewPage_keepArranging")
                    discardDialog.close()
                }
            }
        ]
    }

    Kirigami.PromptDialog {
        id: saveLayoutDialog

        title: page.i18nContext.i18nc("@title:window", "Save as a new dashboard")
        subtitle: page.i18nContext.i18nc("@info", "What is on the page now becomes a new dashboard with this name, at the end of the tabs. You go on arranging this one.")
        standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
        onAccepted: {
            ActionLog.taken("OverviewPage_accepted")
            const result = page.dashboard.saveLayoutAs(layoutName.text);
            if (result.code !== undefined) {
                layoutError.result = result;
            }
        }

        Controls.TextField {
            id: layoutName

            maximumLength: 200
            Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name of the dashboard")
            onAccepted: {
                ActionLog.taken("OverviewPage_nameLayout")
                saveLayoutDialog.accept()
            }
        }
    }

    // A dashboard file is recognised by what it holds, not by its name, so
    // the chooser also offers every file (section 4.1).
    Dialogs.FileDialog {
        id: dashboardFileDialog

        title: page.i18nContext.i18nc("@title:window", "Add a dashboard from a file")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [
            page.i18nContext.i18nc("@item:inlistbox a file type", "Dashboard files (*.json)"),
            page.i18nContext.i18nc("@item:inlistbox a file type", "All files (*)")
        ]
        onAccepted: {
            ActionLog.taken("OverviewPage_dashboardFileChosen")
            page.dashboardFileChosen(selectedFile)
        }
    }

    NewDashboardDialog {
        id: newDashboardDialog

        layout: page.dashboard
        // An empty dashboard opens with the Add menu, so the first widget
        // is one click away (section 2, `empty`).
        onMade: Qt.callLater(() => addMenu.popup())
    }

    // Saving a dashboard to a file (M6d.3, D-443, T-52): one line says what
    // the file holds before the chooser opens; nothing in the book changes.
    DataExport {
        id: dashboardExport
    }

    Kirigami.PromptDialog {
        id: saveDashboardDialog

        title: page.i18nContext.i18nc("@title:window", "Save this dashboard to a file")
        subtitle: page.i18nContext.i18nc("@info", "The file holds this dashboard's layout and the names of the accounts, categories and groups it uses. It holds no amounts or transactions.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Choose where to save…")
                icon.name: "document-save-as"
                onTriggered: {
                    ActionLog.taken("OverviewPage_chooseDashboardFile")
                    saveDashboardDialog.close();
                    if (page.folderUrl.toString() !== "") {
                        saveDashboardFileDialog.currentFolder = page.folderUrl;
                        saveDashboardFileDialog.selectedFile = page.folderUrl + "/" + encodeURIComponent(dashboardExport.dashboardFileName(page.shownDashboardId));
                    }
                    saveDashboardFileDialog.open();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("OverviewPage_cancelSaveDashboard")
                    saveDashboardDialog.close()
                }
            }
        ]
    }

    Dialogs.FileDialog {
        id: saveDashboardFileDialog

        title: page.i18nContext.i18nc("@title:window", "Save the dashboard to a file")
        fileMode: Dialogs.FileDialog.SaveFile
        defaultSuffix: "json"
        nameFilters: [page.i18nContext.i18nc("@item:inlistbox a file type", "Dashboard files (*.json)")]
        onAccepted: {
            ActionLog.taken("OverviewPage_saveDashboardFile")
            const result = dashboardExport.saveDashboard(selectedFile, page.shownDashboardId);
            if (result.code !== undefined) {
                layoutError.result = result;
                return;
            }
            const saved = page.i18nContext.i18nc("@info %1 is a file name", "Saved in %1", result.fileName);
            applicationWindow().showPassiveNotification(result.leftOut > 0
                ? page.i18nContext.i18ncp("@info %1 is how many were left out, %2 a sentence saying where the file was saved", "%2. %1 account, category or group this book no longer has was left out.", "%2. %1 accounts, categories or groups this book no longer has were left out.", result.leftOut, saved)
                : saved, 8000);
        }
    }

    // Deleting says which dashboard opens instead when it is the default
    // one (D-448), and that Undo brings it back.
    Kirigami.PromptDialog {
        id: deleteDashboardDialog

        title: page.i18nContext.i18nc("@title:window", "Delete this dashboard?")
        subtitle: Plain.plain(page.dashboard.view.isDefault ?? false
            ? page.i18nContext.i18nc("@info %1 is the name of the dashboard shown, %2 the one the book opens at instead", "“%1” and its widgets are removed. Your book opens at “%2” from now on. Nothing in your accounts changes, and Undo in the menu brings the dashboard back.", page.dashboardName, page.dashboard.view.instead ?? "")
            : page.i18nContext.i18nc("@info %1 is the name of the dashboard shown", "“%1” and its widgets are removed. Nothing in your accounts changes, and Undo in the menu brings the dashboard back.", page.dashboardName))
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Delete the dashboard")
                icon.name: "edit-delete"
                onTriggered: {
                    ActionLog.taken("OverviewPage_deleteDashboard")
                    deleteDashboardDialog.close();
                    const result = page.dashboard.deleteDashboard();
                    if (result.code !== undefined) {
                        layoutError.result = result;
                    }
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button keep a dashboard", "Keep it")
                icon.name: "go-previous"
                onTriggered: {
                    ActionLog.taken("OverviewPage_keep")
                    deleteDashboardDialog.close()
                }
            }
        ]
    }

    // Ctrl+Page Down and Ctrl+Page Up go to the next and the previous
    // dashboard (D-438).
    Shortcut {
        sequence: "Ctrl+PgDown"
        enabled: page.isCurrentPage && page.dashboardCount > 1 && !page.layoutEditing
        onActivated: {
            ActionLog.taken("OverviewPage_nextDashboard")
            dashboardTabs.step(1)
        }
    }

    Shortcut {
        sequence: "Ctrl+PgUp"
        enabled: page.isCurrentPage && page.dashboardCount > 1 && !page.layoutEditing
        onActivated: {
            ActionLog.taken("OverviewPage_previousDashboard")
            dashboardTabs.step(-1)
        }
    }

    Kirigami.PromptDialog {
        id: resetDialog

        title: page.i18nContext.i18nc("@title:window", "Go back to the built-in layout?")
        subtitle: page.i18nContext.i18nc("@info", "Your arrangement is replaced by the one KassenSturz comes with. You can take that back with Undo until you press Done, and with Undo in the menu after that.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Go back to the built-in layout")
                icon.name: "document-revert"
                onTriggered: {
                    ActionLog.taken("OverviewPage_goBackBuilt")
                    resetDialog.close();
                    const result = page.dashboard.reset();
                    if (result.code !== undefined) {
                        layoutError.result = result;
                    }
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep my layout")
                icon.name: "go-previous"
                onTriggered: {
                    ActionLog.taken("OverviewPage_keepLayout")
                    resetDialog.close()
                }
            }
        ]
    }

    // The properties of the selected component (the panel of M6a.6), with
    // the figures it shows now. A change is part of the session's undo.
    //
    // A Kirigami.Dialog, not an OverlaySheet: the sheet of KF 6.x prints
    // "binding loop" for its implicitHeight on every opening, whatever it
    // holds (M6-B10).
    Kirigami.Dialog {
        id: propertiesSheet

        title: page.i18nContext.i18nc("@title:window", "Properties")
        padding: Kirigami.Units.largeSpacing
        preferredWidth: Kirigami.Units.gridUnit * 32
        standardButtons: Kirigami.Dialog.Close
        onClosed: page.editedId = ""

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Loader {
                Layout.fillWidth: true
                active: propertiesSheet.opened && panelModel.view?.type !== undefined
                sourceComponent: PropertiesPanel {
                    model: panelModel
                    withPeriod: true
                    period: grid.periodOf(panelModel.view.period?.word, grid.byId[page.editedId])
                    choices: page.groupsSource
                }
            }

            // The preview area has a fixed height, so the dialog does not
            // shrink when a choice leaves it with only a short message
            // (M6-B12); what does not fit scrolls.
            Controls.ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 22
                // Headings, notices and boxes have no figures to show.
                visible: ["chart", "table", "card", "progress"].includes(panelModel.view?.preview?.shows)
                contentWidth: availableWidth
                clip: true

                Loader {
                    width: parent.width
                    active: propertiesSheet.opened && panelModel.view?.preview !== undefined
                    sourceComponent: ComponentPreview {
                        subject: panelModel.view
                        period: grid.periodOf(panelModel.view.period?.word, grid.byId[page.editedId])
                    }
                }
            }
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        // One tab per dashboard once there are two (M6d.1, D-438).
        DashboardTabs {
            id: dashboardTabs

            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.largeSpacing
            layout: page.dashboard
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.layoutEditing && !grid.preview
            type: Kirigami.MessageType.Information
            text: page.i18nContext.i18nc("@info", "You are arranging this page. Drag a widget to move it, drag its corner to make it larger or smaller, and use the buttons that appear on the one you pick. Press Done to keep the changes, or Cancel to leave everything as it was.")
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.layoutEditing && (page.dashboard.view.damaged ?? false)
            type: Kirigami.MessageType.Warning
            text: page.i18nContext.i18nc("@info", "The layout saved in this book couldn't be read, so the built-in layout is shown. If you change it and press Done, the saved one is replaced.")
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.layoutEditing && grid.narrow
            type: Kirigami.MessageType.Information
            text: page.i18nContext.i18nc("@info", "This window is too narrow to arrange columns, so the widgets are listed one under the other. You can move them earlier or later. Make the window wider to arrange them side by side.")
        }

        // One message for every component with a problem; the component says
        // what its own problem is.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: grid.problemCount > 0
            type: Kirigami.MessageType.Warning
            text: page.i18nContext.i18ncp("@info %1 is how many widgets", "%1 widget on this page has a problem. It says what is wrong.", "%1 widgets on this page have a problem. Each says what is wrong.", grid.problemCount)
            actions: [
                Kirigami.Action {
                    text: page.i18nContext.i18nc("@action:button go to the first component with a problem", "Show")
                    onTriggered: {
                        ActionLog.taken("OverviewPage_show")
                        grid.showProblem()
                    }
                }
            ]
        }

        // The month's key figures and where its money went, from the
        // report components (M6a.8) on the user's grid (M6b.3).
        DashboardGrid {
            id: grid

            Layout.fillWidth: true
            layout: page.dashboard
            monthKey: page.view.monthKey ?? ""
            pageView: page.view
            overview: page.overview
            choices: page.groupsSource
            bank: page.bank
            bankReady: page.bankReady
            onGetTransactionsClicked: accountId => page.getTransactionsClicked(accountId)
            onTransactionClicked: (accountId, transactionId) => page.transactionClicked(accountId, transactionId)
            onSetBudgetClicked: page.setBudgetClicked()
            onPropertiesRequested: id => page.openProperties(id)
            onFailed: result => layoutError.result = result
            onYChanged: page.updateGridView()
            onHeightChanged: page.updateGridView()
        }
    }
}
