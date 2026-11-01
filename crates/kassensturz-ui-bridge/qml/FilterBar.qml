// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The filter bar above the Overview's grid (M6c.7, D-350, D-351): Period,
// Accounts and Categories, each a button that opens a list. While the page is
// only looked at, a choice is for now: it changes what the page shows, is not
// saved and is not a step in the history; Reset filters goes back to the
// layout's own. While the layout is arranged, the choices are kept with the
// layout when the user presses Done. The layout (Rust) checks and keeps every
// choice; this file shows them and passes them on.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Flow {
    id: bar

    // A `DashboardLayout` (or anything with its `view.filters`, `setFilter`
    // and `resetFilters`).
    required property var layout
    // A `CategoryGroups`: where the accounts, categories and groups to pick
    // from are listed. Without it only the period can be chosen.
    property var choices: null

    readonly property var filters: layout.view.filters ?? ({period: "", accounts: [], categories: [], forNow: false})
    readonly property bool editing: layout.view.editing ?? false
    readonly property bool anyChoice: filters.period !== "" || filters.accounts.length > 0 || filters.categories.length > 0
    readonly property var book: choices?.view ?? ({})
    readonly property var periodWords: ["page", "this-month", "last-month", "last-12-months", "this-year", "last-year"]

    // A choice was refused: the error object.
    signal failed(var result)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    GridNames {
        id: names
    }

    function set(dimension: string, value: string) {
        const result = layout.setFilter(dimension, value);
        if (result.code !== undefined) {
            failed(result);
        }
    }

    function reset() {
        const result = layout.resetFilters();
        if (result.code !== undefined) {
            failed(result);
        }
    }

    spacing: Kirigami.Units.smallSpacing

    Controls.Button {
        id: periodButton

        icon.name: "view-calendar"
        checked: periodPopup.opened
        text: bar.i18nContext.i18nc("@action:button %1 is a period like This year", "Period: %1", bar.filters.period === "" ? bar.i18nContext.i18nc("@item the period the page itself shows", "as the page") : names.periodName(bar.filters.period))
        Accessible.name: text
        onClicked: {
            ActionLog.taken("FilterBar_page")
            periodPopup.opened ? periodPopup.close() : periodPopup.open()
        }

        Controls.Popup {
            id: periodPopup

            y: periodButton.height
            width: Math.max(periodButton.width, Kirigami.Units.gridUnit * 18)
            padding: Kirigami.Units.largeSpacing
            closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

            PeriodChoice {
                width: parent.width
                options: bar.periodWords.map(word => ({word: word, name: names.periodName(word)}))
                word: bar.filters.period === "" ? "page" : bar.filters.period
                onChosen: word => {
                    ActionLog.taken("FilterBar_chosen")
                    bar.set("period", word === "page" ? "" : word);
                    if (word !== "" && word !== undefined) {
                        periodPopup.close();
                    }
                }
            }
        }
    }

    FilterPicker {
        label: bar.i18nContext.i18nc("@label what a filter narrows: the user's accounts", "Accounts")
        options: (bar.book.accounts ?? []).map(account => ({id: account.id, name: account.closed ? bar.i18nContext.i18nc("@option:check %1 is the name of a closed account", "%1 (closed)", account.name) : account.name}))
        chosen: bar.filters.accounts
        emptyText: bar.i18nContext.i18nc("@info", "There are no accounts to pick yet.")
        onPicked: ids => bar.set("accounts", ids)
    }

    FilterPicker {
        label: bar.i18nContext.i18nc("@label what a filter narrows: categories", "Categories")
        options: (bar.book.groups ?? []).map(group => ({id: group.id, name: bar.i18nContext.i18nc("@option:check %1 is the name of a group", "%1 (group)", group.name)}))
            .concat((bar.book.categories ?? []).map(category => ({id: category.id, name: category.path ?? category.name})))
        chosen: bar.filters.categories
        emptyText: bar.i18nContext.i18nc("@info", "There are no categories to pick yet.")
        onPicked: ids => bar.set("categories", ids)
    }

    Controls.Button {
        visible: bar.editing ? bar.anyChoice : bar.filters.forNow
        icon.name: "edit-undo"
        text: bar.i18nContext.i18nc("@action:button go back to the filters the layout has", "Reset filters")
        onClicked: {
            ActionLog.taken("FilterBar_resetFilters")
            bar.reset()
        }
    }

    Controls.Label {
        height: periodButton.height
        verticalAlignment: Text.AlignVCenter
        visible: bar.editing || bar.filters.forNow
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        color: Kirigami.Theme.disabledTextColor
        text: bar.editing
            ? bar.i18nContext.i18nc("@info", "These choices are kept with the layout when you press Done.")
            : bar.i18nContext.i18nc("@info", "These choices are for now. They are not saved.")
    }
}
