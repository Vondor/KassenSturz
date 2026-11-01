// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The menu that adds a widget to the Overview (M6b, D-337): the kinds are in
// groups (Bank, Charts, Figures and tables, Layout, Notes, Saved views), the
// groups are in alphabetical order and so is each group's list. The order
// is made here from the words on screen, so it is right in every language.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Controls.Menu {
    // Not `menu` or `addMenu`: inside a MenuItem `menu` is the item's own
    // property (its submenu) and `addMenu` is a function of the Menu, so
    // `menu.kindChosen` was no function there and a click did nothing (M6-B9).
    id: widgetMenu

    // Rows with `id` and `name` of the usable saved views.
    property var savedList: []
    // Whether bank connections can be used (D-054): only then is there a
    // Bank group with the Update all accounts button (M6d.7).
    property bool bankReady: false
    // The types of the widgets on the page: the period header and the filter
    // bar are offered only while the page has none (M6d.8, D-474).
    property var pageKinds: []

    // `dataset` is empty for a kind that starts with its first dataset.
    signal kindChosen(string kind, string dataset)
    signal savedChosen(string id)

    readonly property GridNames names: GridNames {}

    function sorted(rows: var): var {
        return rows.slice().sort((a, b) => a.name.localeCompare(b.name));
    }

    function kindRows(kinds: var): var {
        return sorted(kinds.map(kind => ({ id: kind, kind: kind, dataset: "", name: names.typeName(kind), saved: false })));
    }

    // Kinds that start with a dataset of their own (M6c.2), by `kind` and
    // `dataset`: they have a name that says what they show.
    function dataRows(entries: var): var {
        return sorted(entries.map(entry => ({ id: entry.kind + "/" + entry.dataset, kind: entry.kind, dataset: entry.dataset, name: names.dataName(entry.dataset, entry.kind), saved: false })));
    }

    // Each group: its name and its rows, sorted; empty groups are left out.
    readonly property var groups: {
        const ctx = names.i18nContext;
        const list = [
            { name: ctx.i18nc("@title:menu a group of widgets: charts", "Charts"),
              rows: kindRows(["chart.bar", "chart.pie", "chart.donut", "chart.line", "chart.area", "chart.stacked_bar"])
                  .concat(dataRows([{ kind: "chart.line", dataset: "net_worth_over_time" }, { kind: "chart.bar", dataset: "cash_flow_by_month" }])).sort((a, b) => a.name.localeCompare(b.name)) },
            { name: ctx.i18nc("@title:menu a group of widgets: single figures and tables", "Figures and tables"),
              rows: kindRows(["kpi.card", "progress.bar", "table.rows", "table.income_sources", "card.net_worth", "budget.progress"])
                  .concat(dataRows([{ kind: "kpi.card", dataset: "account_balances" }, { kind: "table.rows", dataset: "account_balances" }, { kind: "table.rows", dataset: "cash_flow_by_month" }])).sort((a, b) => a.name.localeCompare(b.name)) },
            { name: ctx.i18nc("@title:menu a group of widgets: those that work with the user's banks", "Bank"),
              rows: widgetMenu.bankReady ? kindRows(["bank.update_all"]) : [] },
            { name: ctx.i18nc("@title:menu a group of widgets: those that arrange the page", "Layout"),
              rows: kindRows(["text.heading", "text.separator", "group.period"]
                  .concat(["page.period", "page.filters"].filter(kind => !widgetMenu.pageKinds.includes(kind)))) },
            { name: ctx.i18nc("@title:menu a group of widgets: notes and warnings", "Notes"),
              rows: kindRows(["notice.fetch_gap", "notice.transfers"]) },
            { name: ctx.i18nc("@title:menu a group of widgets: views saved in the Statistic Builder", "Saved views"),
              rows: sorted(widgetMenu.savedList.map(row => ({ id: row.id, name: row.name, saved: true }))) }
        ];
        return sorted(list.filter(group => group.rows.length > 0));
    }

    Instantiator {
        model: widgetMenu.groups
        delegate: Controls.Menu {
            id: groupMenu

            required property var modelData

            title: modelData.name

            Instantiator {
                model: groupMenu.modelData.rows
                delegate: Controls.MenuItem {
                    required property var modelData
                    text: modelData.name
                    onTriggered: {
                        ActionLog.taken("AddMenu_triggered")
                        modelData.saved ? widgetMenu.savedChosen(modelData.id) : widgetMenu.kindChosen(modelData.kind, modelData.dataset)
                    }
                }
                onObjectAdded: (index, object) => groupMenu.insertItem(index, object)
                onObjectRemoved: (index, object) => groupMenu.removeItem(object)
            }
        }
        onObjectAdded: (index, object) => widgetMenu.insertMenu(index, object)
        onObjectRemoved: (index, object) => widgetMenu.removeMenu(object)
    }
}
