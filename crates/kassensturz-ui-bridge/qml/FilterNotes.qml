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
    // The title names the accounts the component counts, so the marks about
    // the accounts are left out (US1.9, #523, D-636). Every other mark and
    // every note stays.
    property bool accountsInTitle: false
    // The tile's *Hide the grey notes* switch (`props.hide_note`, M6-B202,
    // #581, D-663): the marks that only say what it is filtered to are left
    // out. The notes about a problem (a filter that chooses nothing, one that
    // names something gone) stay, so a tile is never empty without a reason.
    // The note that an average counts months before the first transaction
    // (M6-B207, #590, D-668) is left out too: the figure is right as it is,
    // and so is the one about a comparison's early periods (US3.1a).
    property bool hideMarks: false

    readonly property var own: hideMarks ? [] : (filter?.own ?? []).filter(word => !(accountsInTitle && word === "accounts"))
    readonly property var filtered: hideMarks ? [] : (filter?.filtered ?? []).filter(word => word !== "period" && !(accountsInTitle && word === "accounts"))
    readonly property var list: (filter?.notes ?? []).filter(note => !(hideMarks && (note.kind === "not-applied" || note.kind === "average-early" || note.kind === "compare-early")))
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
        case "tags":
            return i18nContext.i18nc("@item what a filter narrows: the tags", "tags");
        }
        return "";
    }

    function names(kept: var, more: int): string {
        const shown = kept.join(", ");
        return more > 0 ? i18nContext.i18nc("@info %1 is a list of names, %2 how many more there are", "%1 and %2 more", shown, more) : shown;
    }

    function noteText(note: var): string {
        if (note.kind === "not-applied") {
            if (note.dimension === "tags") {
                return i18nContext.i18nc("@info", "Not filtered by tag: this counts whole accounts or budgets, not tagged money.");
            }
            return note.dimension === "accounts"
                ? i18nContext.i18nc("@info", "Not filtered by account: this counts every account.")
                : i18nContext.i18nc("@info", "Not filtered by category: this counts whole accounts.");
        }
        if (note.kind === "average-early") {
            // %1 of the %2 months (quarters, years) the average divides by
            // lie before the first transaction it could count.
            switch (note.id) {
            case "per-quarter":
                return i18nContext.i18ncp("@info %1 is how many quarters of an average are before the first transaction, %2 how many it counts", "%1 of the %2 quarters this average counts is before the first transaction and counts as zero.", "%1 of the %2 quarters this average counts are before the first transaction and count as zero.", note.before, note.counted);
            case "per-year":
                return i18nContext.i18ncp("@info %1 is how many years of an average are before the first transaction, %2 how many it counts", "%1 of the %2 years this average counts is before the first transaction and counts as zero.", "%1 of the %2 years this average counts are before the first transaction and count as zero.", note.before, note.counted);
            }
            return i18nContext.i18ncp("@info %1 is how many months of an average are before the first transaction, %2 how many it counts", "%1 of the %2 months this average counts is before the first transaction and counts as zero.", "%1 of the %2 months this average counts are before the first transaction and count as zero.", note.before, note.counted);
        }
        if (note.kind === "compare-early") {
            // A comparison whose periods end before the first transaction it
            // could count: they count as zero (US3.1a).
            if (note.counted <= 1) {
                return i18nContext.i18nc("@info", "The period compared is before the first transaction.");
            }
            return i18nContext.i18ncp("@info %1 is how many periods of a comparison are before the first transaction, %2 how many it averages", "%1 of the %2 periods this comparison averages is before the first transaction and counts as zero.", "%1 of the %2 periods this comparison averages are before the first transaction and count as zero.", note.before, note.counted);
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
            if (notes.filtered.includes("tags")) {
                parts.push(notes.i18nContext.i18nc("@info %1 is a list of tag names", "tags: %1", notes.names(notes.filter.tags ?? [], notes.filter.tagsMore ?? 0)));
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
