// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The dashboard menu of the Overview (M6d.1, D-438), shown once a book has
// two or more dashboards; with one, the page offers only *New dashboard…*.
// Each entry works on the dashboard shown and is one change in the history
// (D-436). The page asks the questions (new name, new name for Rename,
// Delete): this menu tells it with a signal. Later steps add one entry each
// below the line before Delete: *Save to a file…* (M6d.3), *Add from a
// file…* (M6d.4), *Back to the template…* (M6d.5); M6d.2 adds the templates
// to `NewDashboardDialog`.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Action {
    id: menu

    // A `DashboardLayout`.
    required property var layout

    readonly property var view: layout.view ?? ({})
    readonly property var dashboards: view.dashboards ?? []
    readonly property int shownIndex: dashboards.findIndex(dashboard => dashboard.shown)
    readonly property bool stored: view.stored ?? false

    signal newRequested()
    signal renameRequested()
    signal deleteRequested()
    // An action was refused: the error object.
    signal failed(var result)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function answered(result: var) {
        if (result.code !== undefined) {
            failed(result);
        }
    }

    icon.name: "view-list-icons"
    text: i18nContext.i18nc("@action:button the menu of the dashboard shown", "Dashboard")

    Kirigami.Action {
        icon.name: "list-add"
        text: menu.i18nContext.i18nc("@action:inmenu", "New dashboard…")
        onTriggered: {
            ActionLog.taken("DashboardMenu_newDashboard")
            menu.newRequested()
        }
    }
    Kirigami.Action {
        separator: true
    }
    Kirigami.Action {
        icon.name: "edit-rename"
        text: menu.i18nContext.i18nc("@action:inmenu give the dashboard shown another name", "Rename…")
        onTriggered: {
            ActionLog.taken("DashboardMenu_rename")
            menu.renameRequested()
        }
    }
    Kirigami.Action {
        icon.name: "edit-copy"
        text: menu.i18nContext.i18nc("@action:inmenu make a copy of the dashboard shown", "Duplicate")
        onTriggered: {
            ActionLog.taken("DashboardMenu_duplicate")
            const name = menu.view.name ?? "";
            menu.answered(menu.layout.duplicateDashboard(
                menu.i18nContext.i18nc("@item the name of a copy of a dashboard, %1 is its name", "%1 (copy)", name),
                // U+0001 marks where Rust puts the number of the copy.
                menu.i18nContext.i18nc("@item the name of a further copy of a dashboard, %1 is its name, %2 a number", "%1 (copy %2)", name, "\u0001")))
        }
    }
    Kirigami.Action {
        icon.name: "go-previous"
        enabled: menu.stored && menu.shownIndex > 0
        text: menu.i18nContext.i18nc("@action:inmenu move the dashboard shown one place towards the start of the list", "Move earlier")
        onTriggered: {
            ActionLog.taken("DashboardMenu_moveEarlier")
            menu.answered(menu.layout.moveDashboard(-1))
        }
    }
    Kirigami.Action {
        icon.name: "go-next"
        enabled: menu.stored && menu.shownIndex >= 0 && menu.shownIndex < menu.dashboards.length - 1
        text: menu.i18nContext.i18nc("@action:inmenu move the dashboard shown one place towards the end of the list", "Move later")
        onTriggered: {
            ActionLog.taken("DashboardMenu_moveLater")
            menu.answered(menu.layout.moveDashboard(1))
        }
    }
    Kirigami.Action {
        icon.name: "go-home"
        enabled: !(menu.view.isDefault ?? true)
        text: menu.i18nContext.i18nc("@action:inmenu the book opens at the dashboard shown from now on", "Make this the default")
        onTriggered: {
            ActionLog.taken("DashboardMenu_makeDefault")
            menu.answered(menu.layout.makeDefault())
        }
    }
    Kirigami.Action {
        separator: true
    }
    Kirigami.Action {
        icon.name: "edit-delete"
        enabled: menu.stored
        text: menu.i18nContext.i18nc("@action:inmenu", "Delete…")
        onTriggered: {
            ActionLog.taken("DashboardMenu_delete")
            menu.deleteRequested()
        }
    }
}
