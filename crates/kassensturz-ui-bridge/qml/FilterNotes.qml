// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What a component says about its filters (M6c.7, D-350): the *Own filter*
// mark naming what it chooses itself, the *Filtered* mark naming what the
// filter keeps, and the notes (a dimension the figures cannot be narrowed by,
// something that no longer exists, a filter that chooses nothing). The report
// layer decides all of it; this file only gives the words, so a filter is
// never applied without being said.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: notes

    // `view.filter` of the object that drew the component, or undefined.
    property var filter: undefined

    readonly property var own: filter?.own ?? []
    readonly property var filtered: (filter?.filtered ?? []).filter(word => word !== "period")
    readonly property var list: filter?.notes ?? []
    readonly property bool quiet: own.length === 0 && filtered.length === 0 && list.length === 0

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function dimensionName(word: string): string {
        switch (word) {
        case "period":
            return i18nContext.i18nc("@item what a filter narrows: the days", "period");
        case "accounts":
            return i18nContext.i18nc("@item what a filter narrows: the accounts", "accounts");
        case "categories":
            return i18nContext.i18nc("@item what a filter narrows: the categories", "categories");
        }
        return "";
    }

    function names(kept: var, more: int): string {
        const shown = kept.join(", ");
        return more > 0 ? i18nContext.i18nc("@info %1 is a list of names, %2 how many more there are", "%1 and %2 more", shown, more) : shown;
    }

    function noteText(note: var): string {
        if (note.kind === "not-applied") {
            return note.dimension === "accounts"
                ? i18nContext.i18nc("@info", "Not filtered by account: this counts every account.")
                : i18nContext.i18nc("@info", "Not filtered by category: this counts whole accounts.");
        }
        if (note.kind === "gone") {
            return i18nContext.i18nc("@info", "The filter names something that no longer exists. Take it out of the filter.");
        }
        return i18nContext.i18nc("@info", "The filter chooses nothing, so nothing is shown. Change the filter to see figures.");
    }

    visible: !quiet
    spacing: 0

    Controls.Label {
        Layout.fillWidth: true
        visible: notes.own.length > 0
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        color: Kirigami.Theme.disabledTextColor
        text: notes.i18nContext.i18nc("@info %1 is a list of what the component chooses itself, like period, accounts", "Own filter: %1", notes.own.map(word => notes.dimensionName(word)).join(", "))
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: notes.filtered.length > 0
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        color: Kirigami.Theme.disabledTextColor
        text: {
            const parts = [];
            if (notes.filtered.includes("accounts")) {
                parts.push(notes.i18nContext.i18nc("@info %1 is a list of account names", "accounts: %1", notes.names(notes.filter.accounts ?? [], notes.filter.accountsMore ?? 0)));
            }
            if (notes.filtered.includes("categories")) {
                parts.push(notes.i18nContext.i18nc("@info %1 is a list of category names", "categories: %1", notes.names(notes.filter.categories ?? [], notes.filter.categoriesMore ?? 0)));
            }
            return notes.i18nContext.i18nc("@info %1 is what the filter keeps", "Filtered to %1", parts.join("; "));
        }
    }

    Repeater {
        model: notes.list

        Controls.Label {
            required property var modelData

            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font: Kirigami.Theme.smallFont
            color: modelData.kind === "chooses-nothing" ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.disabledTextColor
            text: notes.noteText(modelData)
        }
    }
}
