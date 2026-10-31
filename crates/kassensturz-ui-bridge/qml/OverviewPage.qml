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
// the history. The parts that are not widgets (the net worth card and the month
// switcher) stay fixed around the grid for now; the fetch-gap warnings, the note
// about transfers and the separator are widgets on it (D-327, D-328, D-334).

import QtQuick
import QtQuick.Controls as Controls
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
    readonly property var view: overview.view ?? ({})

    signal addAccountClicked()
    signal addTransactionClicked()
    signal getTransactionsClicked(string accountId)

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

    // The layouts the user saved under a name (D-330). Saving one is its own
    // step in the change history; loading one is an edit in the session that
    // Undo takes back.
    property var savedLayouts: []
    property var chosenLayout: ({})

    function openLayouts() {
        savedLayouts = dashboard.savedLayouts();
        layoutsDialog.open();
    }

    function loadLayout(id: string) {
        const result = dashboard.loadLayout(id);
        if (result.code !== undefined) {
            layoutError.result = result;
        } else {
            layoutsDialog.close();
        }
    }

    function deleteLayout(id: string) {
        const result = dashboard.deleteLayout(id);
        if (result.code !== undefined) {
            layoutError.result = result;
        }
        savedLayouts = dashboard.savedLayouts();
    }

    AddMenu {
        id: addMenu

        savedList: page.savedList
        onKindChosen: kind => grid.add(kind)
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

    // Going to another screen and back starts from the current month again.
    Component.onCompleted: {
        overview.showThisMonth();
        dashboard.refresh();
    }

    title: layoutEditing ? i18nContext.i18nc("@title", "Arrange the Overview") : i18nContext.i18nc("@title", "Overview")

    header: FormError {
        id: layoutError
    }

    actions: [
        Kirigami.Action {
            visible: !page.layoutEditing
            icon.name: "view-grid"
            text: page.i18nContext.i18nc("@action:button change where things are on this page", "Edit layout")
            onTriggered: page.dashboard.edit()
        },
        Kirigami.Action {
            visible: !page.layoutEditing
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add transaction")
            onTriggered: page.addTransactionClicked()
        },
        Kirigami.Action {
            visible: !page.layoutEditing
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add account")
            onTriggered: page.addAccountClicked()
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "dialog-ok-apply"
            text: page.i18nContext.i18nc("@action:button keep the changes to the layout", "Done")
            onTriggered: page.finishLayout()
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "dialog-cancel"
            text: page.i18nContext.i18nc("@action:button throw away the changes to the layout", "Cancel")
            onTriggered: page.cancelLayout()
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button add a chart, table or figure to the layout", "Add to the page")
            onTriggered: addMenu.popup()
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "edit-undo"
            enabled: page.canUndoLayout
            text: enabled
                ? page.i18nContext.i18nc("@action:button %1 is what was done to the layout", "Undo: %1", names.editName(page.undoLayoutWord))
                : page.i18nContext.i18nc("@action:button there is nothing to undo", "Undo")
            tooltip: text
            onTriggered: page.undoLayout()
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "edit-redo"
            enabled: page.canRedoLayout
            text: enabled
                ? page.i18nContext.i18nc("@action:button %1 is what was done to the layout", "Redo: %1", names.editName(page.redoLayoutWord))
                : page.i18nContext.i18nc("@action:button there is nothing to make again", "Redo")
            tooltip: text
            onTriggered: page.redoLayout()
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "view-preview"
            checkable: true
            checked: grid.preview
            text: page.i18nContext.i18nc("@action:button hide the grid lines and the toolbars", "Preview")
            onTriggered: grid.preview = checked
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "document-save-as"
            text: page.i18nContext.i18nc("@action:button save the arrangement under a name or load one saved earlier", "Saved layouts")
            onTriggered: page.openLayouts()
        },
        Kirigami.Action {
            visible: page.layoutEditing
            icon.name: "document-revert"
            enabled: !(page.dashboard.view.builtIn ?? true)
            text: page.i18nContext.i18nc("@action:button put the layout KassenSturz comes with back", "Back to the built-in layout")
            onTriggered: resetDialog.open()
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
                    discardDialog.close();
                    page.dashboard.cancel();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button keep arranging the layout", "Keep arranging")
                icon.name: "go-previous"
                onTriggered: discardDialog.close()
            }
        ]
    }

    Kirigami.Dialog {
        id: layoutsDialog

        title: page.i18nContext.i18nc("@title:window", "Saved layouts")
        standardButtons: Kirigami.Dialog.Close
        preferredWidth: Kirigami.Units.gridUnit * 24

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                wrapMode: Text.Wrap
                text: page.i18nContext.i18nc("@info", "Keep the way the Overview looks now under a name, and come back to it later. Loading a saved layout replaces what is on the page; you can take that back with Undo until you press Done.")
            }

            Controls.Button {
                Layout.alignment: Qt.AlignHCenter
                icon.name: "document-save-as"
                text: page.i18nContext.i18nc("@action:button", "Save this layout as…")
                onClicked: {
                    layoutName.text = "";
                    saveLayoutDialog.open();
                }
            }

            Controls.Label {
                visible: page.savedLayouts.length === 0
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                text: page.i18nContext.i18nc("@info", "No layouts saved yet.")
            }

            Repeater {
                model: page.savedLayouts

                delegate: RowLayout {
                    required property var modelData

                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing
                    Layout.rightMargin: Kirigami.Units.largeSpacing

                    Controls.Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: modelData.name
                    }

                    Controls.Button {
                        text: page.i18nContext.i18nc("@action:button put a saved layout on the page", "Load")
                        Accessible.name: page.i18nContext.i18nc("@action:button %1 is the name of a saved layout", "Load %1", modelData.name)
                        onClicked: page.loadLayout(modelData.id)
                    }

                    Controls.Button {
                        icon.name: "edit-delete"
                        display: Controls.AbstractButton.IconOnly
                        text: page.i18nContext.i18nc("@action:button %1 is the name of a saved layout", "Delete %1", modelData.name)
                        Controls.ToolTip.visible: hovered
                        Controls.ToolTip.text: text
                        onClicked: {
                            page.chosenLayout = modelData;
                            deleteLayoutDialog.open();
                        }
                    }
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: saveLayoutDialog

        title: page.i18nContext.i18nc("@title:window", "Save this layout as")
        standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
        onAccepted: {
            const result = page.dashboard.saveLayoutAs(layoutName.text);
            if (result.code !== undefined) {
                layoutError.result = result;
            }
            page.savedLayouts = page.dashboard.savedLayouts();
        }

        Controls.TextField {
            id: layoutName

            maximumLength: 200
            Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name of the layout")
            onAccepted: saveLayoutDialog.accept()
        }
    }

    Kirigami.PromptDialog {
        id: deleteLayoutDialog

        title: page.i18nContext.i18nc("@title:window", "Delete this saved layout?")
        subtitle: page.i18nContext.i18nc("@info %1 is the name of a saved layout", "“%1” is removed from your saved layouts. The Overview itself doesn't change. You can take this back with Undo in the menu.", page.chosenLayout.name ?? "")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Delete the layout")
                icon.name: "edit-delete"
                onTriggered: {
                    deleteLayoutDialog.close();
                    page.deleteLayout(page.chosenLayout.id);
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button keep a saved layout", "Keep it")
                icon.name: "go-previous"
                onTriggered: deleteLayoutDialog.close()
            }
        ]
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
                onTriggered: resetDialog.close()
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
                active: propertiesSheet.opened && panelModel.view.type !== undefined
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
                visible: ["chart", "table", "card", "progress"].includes(panelModel.view.preview?.shows)
                contentWidth: availableWidth
                clip: true

                Loader {
                    width: parent.width
                    active: propertiesSheet.opened && panelModel.view.preview !== undefined
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

        // The month, with the way to the ones before and after it.
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.smallSpacing

            Controls.ToolButton {
                id: earlierButton

                icon.name: "go-previous"
                text: page.i18nContext.i18nc("@action:button the month before", "Earlier month")
                display: Controls.AbstractButton.IconOnly
                enabled: page.view.hasEarlier ?? false
                onClicked: page.overview.showEarlierMonth()

                Controls.ToolTip.text: text
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }

            Kirigami.Heading {
                textFormat: Text.PlainText
                Layout.fillWidth: true
                text: page.view.month ?? ""
                level: 2
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }

            Controls.ToolButton {
                icon.name: "go-next"
                text: page.i18nContext.i18nc("@action:button the month after", "Later month")
                display: Controls.AbstractButton.IconOnly
                enabled: page.view.hasLater ?? false
                onClicked: page.overview.showLaterMonth()

                Controls.ToolTip.text: text
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }

        Controls.Button {
            Layout.alignment: Qt.AlignHCenter
            visible: !(page.view.thisMonth ?? true)
            icon.name: "go-jump-today"
            text: page.i18nContext.i18nc("@action:button", "Back to this month")
            onClicked: page.overview.showThisMonth()
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: page.view.empty ?? false
            text: page.i18nContext.i18nc("@info", "Nothing came in or went out this month yet. Add what you paid or received, and it shows up here.")
            wrapMode: Text.Wrap
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
                    onTriggered: grid.showProblem()
                }
            ]
        }

        // The month's key figures and where its money went, from the
        // report components (M6a.8) on the user's grid (M6b.3).
        DashboardGrid {
            id: grid

            Layout.fillWidth: true
            visible: page.layoutEditing || !(page.view.empty ?? false)
            layout: page.dashboard
            monthKey: page.view.monthKey ?? ""
            pageView: page.view
            onGetTransactionsClicked: accountId => page.getTransactionsClicked(accountId)
            onPropertiesRequested: id => page.openProperties(id)
            onFailed: result => layoutError.result = result
        }
    }
}
