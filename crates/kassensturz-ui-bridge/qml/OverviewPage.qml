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
// the history. The parts that are not components (the net worth card, the
// fetch-gap warnings, the month switcher and the note about transfers) stay
// fixed around the grid.

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
    // A `CategoryGroups`, for picking My groups and categories in the panel.
    property var groupsSource: null
    readonly property var view: overview.view ?? ({})
    readonly property var sections: view.sections ?? []
    readonly property var totals: view.totals ?? []
    readonly property var fetchGaps: view.fetchGaps ?? []

    signal addAccountClicked()
    signal addTransactionClicked()
    signal getTransactionsClicked(string accountId)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // `amount` as the figure of the net worth card: a debt reads "You owe",
    // not a minus sign (D-067).
    function owedText(amount: string, owes: bool): string {
        return owes ? i18nContext.i18nc("@label %1 is an amount of money", "You owe %1", amount) : amount;
    }

    function categoryName(total: var): string {
        return total.unsorted ? i18nContext.i18nc("@label money with no category yet", "Not sorted yet") : total.name;
    }

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

            LibraryAction { kind: "chart.donut"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "chart.bar"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "chart.line"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "chart.area"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "chart.stacked_bar"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "chart.pie"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "table.rows"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "kpi.card"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "progress.bar"; onChosen: kind => grid.add(kind) }
            LibraryAction { kind: "text.heading"; onChosen: kind => grid.add(kind) }
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
    Kirigami.OverlaySheet {
        id: propertiesSheet

        title: page.i18nContext.i18nc("@title:window", "Properties")
        onClosed: page.editedId = ""

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Loader {
                Layout.fillWidth: true
                active: propertiesSheet.opened && panelModel.view.type !== undefined
                sourceComponent: PropertiesPanel {
                    model: panelModel
                    withPeriod: true
                    choices: page.groupsSource
                }
            }

            Loader {
                Layout.fillWidth: true
                active: propertiesSheet.opened && panelModel.view.preview !== undefined
                sourceComponent: ComponentPreview {
                    subject: panelModel.view
                    period: grid.periodOf(panelModel.view.period?.word)
                }
            }
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        // An account linked to the bank that was last updated from it long
        // ago (M5.U, D-180).
        Repeater {
            model: page.fetchGaps

            delegate: Kirigami.InlineMessage {
                id: gap

                required property var modelData

                Layout.fillWidth: true
                type: Kirigami.MessageType.Warning
                visible: true
                text: Plain.plain(page.i18nContext.i18nc("@info %1 is how many days, %2 an account's name, %3 a date", "It has been %1 days since you last got new transactions for %2 from your bank (%3). Your bank only lists its recent transactions, so some may be missing if you wait longer.", gap.modelData.days, gap.modelData.account, gap.modelData.lastFetched))
                actions: [
                    Kirigami.Action {
                        icon.name: "view-refresh"
                        text: page.i18nContext.i18nc("@action:button", "Get new transactions")
                        onTriggered: page.getTransactionsClicked(gap.modelData.accountId)
                    }
                ]
            }
        }

        // What all the accounts hold together less what is owed, per
        // currency: the net worth card (Q-215). It is today's figure, so it
        // stays when the month below changes.
        Repeater {
            model: page.sections

            delegate: ColumnLayout {
                id: section

                required property var modelData

                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                // Only a book with several currencies needs to say which.
                Kirigami.Heading {
                    textFormat: Text.PlainText
                    visible: page.sections.length > 1
                    text: section.modelData.currency
                    level: 3
                }

                KpiGrid {
                    cards: [{
                        measure: "net-worth",
                        currency: section.modelData.currency,
                        value: page.owedText(section.modelData.together, section.modelData.togetherOwes),
                        exactValue: page.owedText(section.modelData.exactTogether, section.modelData.togetherOwes),
                        negative: section.modelData.togetherOwes,
                        previous: "",
                        change: "",
                        changePercent: "",
                        direction: "none"
                    }]
                }
            }
        }

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
            text: page.i18nContext.i18nc("@info", "You are arranging this page. Drag a component to move it, drag its corner to make it larger or smaller, and use the buttons that appear on the one you pick. Press Done to keep the changes, or Cancel to leave everything as it was.")
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
            text: page.i18nContext.i18nc("@info", "This window is too narrow to arrange columns, so the components are listed one under the other. You can move them earlier or later. Make the window wider to arrange them side by side.")
        }

        // One message for every component with a problem; the component says
        // what its own problem is.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: grid.problemCount > 0
            type: Kirigami.MessageType.Warning
            text: page.i18nContext.i18ncp("@info %1 is how many components", "%1 component on this page has a problem. It says what is wrong.", "%1 components on this page have a problem. Each says what is wrong.", grid.problemCount)
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
            onPropertiesRequested: id => page.openProperties(id)
            onFailed: result => layoutError.result = result
        }

        Repeater {
            model: page.totals

            delegate: ColumnLayout {
                id: monthTotals

                required property var modelData

                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing
                visible: !(page.view.empty ?? false)

                Kirigami.Heading {
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    visible: monthTotals.modelData.income.length > 0
                    text: page.i18nContext.i18nc("@title", "Where your money came from")
                    level: 3
                }

                Repeater {
                    model: monthTotals.modelData.income

                    delegate: AmountLine {
                        Layout.fillWidth: true
                        name: page.categoryName(modelData)
                    }
                }
            }
        }

        // Transfers are left out of the totals (D-021); the page says so
        // only when the month has one.
        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: (page.view.transfers ?? 0) > 0
            text: page.i18nContext.i18ncp("@info", "Money you moved between your accounts (once this month) isn't counted as money in or out.", "Money you moved between your accounts (%1 times this month) isn't counted as money in or out.", page.view.transfers ?? 0)
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
