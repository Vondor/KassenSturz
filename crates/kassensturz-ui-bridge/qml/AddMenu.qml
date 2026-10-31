// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The menu that adds a widget to the Overview (M6b, D-337): the kinds are in
// groups (Charts, Figures and tables, Layout, Notes, Saved views), the
// groups are in alphabetical order and so is each group's list. The order
// is made here from the words on screen, so it is right in every language.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Controls.Menu {
    id: menu

    // Rows with `id` and `name` of the usable saved views.
    property var savedList: []

    signal kindChosen(string kind)
    signal savedChosen(string id)

    readonly property GridNames names: GridNames {}

    function sorted(rows: var): var {
        return rows.slice().sort((a, b) => a.name.localeCompare(b.name));
    }

    function kindRows(kinds: var): var {
        return sorted(kinds.map(kind => ({ id: kind, name: names.typeName(kind), saved: false })));
    }

    // Each group: its name and its rows, sorted; empty groups are left out.
    readonly property var groups: {
        const ctx = names.i18nContext;
        const list = [
            { name: ctx.i18nc("@title:menu a group of widgets: charts", "Charts"),
              rows: kindRows(["chart.bar", "chart.pie", "chart.donut", "chart.line", "chart.area", "chart.stacked_bar"]) },
            { name: ctx.i18nc("@title:menu a group of widgets: single figures and tables", "Figures and tables"),
              rows: kindRows(["kpi.card", "progress.bar", "table.rows", "table.income_sources", "card.net_worth"]) },
            { name: ctx.i18nc("@title:menu a group of widgets: those that arrange the page", "Layout"),
              rows: kindRows(["text.heading", "text.separator", "group.period"]) },
            { name: ctx.i18nc("@title:menu a group of widgets: notes and warnings", "Notes"),
              rows: kindRows(["notice.fetch_gap", "notice.transfers"]) },
            { name: ctx.i18nc("@title:menu a group of widgets: views saved in the Statistic Builder", "Saved views"),
              rows: sorted(menu.savedList.map(row => ({ id: row.id, name: row.name, saved: true }))) }
        ];
        return sorted(list.filter(group => group.rows.length > 0));
    }

    Instantiator {
        model: menu.groups
        delegate: Controls.Menu {
            id: groupMenu

            required property var modelData

            title: modelData.name

            Instantiator {
                model: groupMenu.modelData.rows
                delegate: Controls.MenuItem {
                    required property var modelData
                    text: modelData.name
                    onTriggered: modelData.saved ? menu.savedChosen(modelData.id) : menu.kindChosen(modelData.id)
                }
                onObjectAdded: (index, object) => groupMenu.insertItem(index, object)
                onObjectRemoved: (index, object) => groupMenu.removeItem(object)
            }
        }
        onObjectAdded: (index, object) => menu.insertMenu(index, object)
        onObjectRemoved: (index, object) => menu.removeMenu(object)
    }
}
