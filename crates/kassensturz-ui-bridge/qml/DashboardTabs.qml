// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The row of tabs of a dashboard (M6d.1, D-438), the widget `page.tabs`
// since M6d.9 (D-529): one tab per dashboard in the book's order, its name as
// plain text, a small home icon on the one the book opens at (D-448). A
// narrow window scrolls the row. With one dashboard there is no row to show.
// While a dashboard is arranged the tabs can't be used: Done or Cancel comes
// first. The list and the switch are Rust's (`DashboardLayout`); this file
// only shows them. The keys that go to the next and the previous dashboard
// are the page's, so they work on a dashboard without the widget too.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Controls.TabBar {
    id: tabs

    // A `DashboardLayout`.
    required property var layout

    readonly property var dashboards: layout.view.dashboards ?? []
    readonly property int shownIndex: dashboards.findIndex(dashboard => dashboard.shown)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    visible: dashboards.length > 1
    enabled: !(layout.view.editing ?? false)
    currentIndex: shownIndex

    Repeater {
        model: tabs.dashboards

        delegate: Controls.TabButton {
            required property var modelData
            required property int index

            width: implicitWidth
            text: modelData.name
            icon.name: modelData.isDefault ? "go-home" : ""
            checked: modelData.shown
            Accessible.name: modelData.isDefault
                ? tabs.i18nContext.i18nc("@info:tab %1 is a dashboard's name, %2 its place in the list, %3 how many there are", "Dashboard %1, default, %2 of %3", modelData.name, index + 1, tabs.dashboards.length)
                : tabs.i18nContext.i18nc("@info:tab %1 is a dashboard's name, %2 its place in the list, %3 how many there are", "Dashboard %1, %2 of %3", modelData.name, index + 1, tabs.dashboards.length)
            onClicked: {
                ActionLog.taken("DashboardTabs_clicked")
                tabs.layout.showDashboard(modelData.id)
            }

            Controls.ToolTip.text: modelData.isDefault ? tabs.i18nContext.i18nc("@info:tooltip", "This book opens at this dashboard") : ""
            Controls.ToolTip.visible: modelData.isDefault && hovered
            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }
}
