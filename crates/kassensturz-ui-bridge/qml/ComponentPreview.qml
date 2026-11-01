// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Shows one component of the properties panel with the real figures of the
// open book (M6a.7): a chart, a table, a key figure card or a progress bar.
// What to ask `BookCharts` and `BookCards` for comes from the component's
// `preview` object (properties.rs), so nothing is mapped here. Every figure
// is text from Rust (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: preview

    // `ComponentProperties.view`.
    required property var subject
    // One of the fixed period names (`this-month`, `last-month`,
    // `last-12-months`, `this-year`, `last-year`).
    property string period: "last-12-months"
    // The filters of the component on its page (M6c.7): the text the layout
    // gives. Undefined where there is no page around it (the properties
    // panel), where only the component's own lists narrow it.
    property var scope: undefined

    readonly property var asked: subject?.preview ?? ({})
    readonly property var charts: chartData.view?.charts ?? []
    // The categories that more than one column counts (D-339).
    readonly property var shared: chartData.view?.shared ?? []
    // What each column adds up to, for the cards of the board (D-341).
    readonly property var totals: chartData.view?.totals ?? []
    readonly property var cardSets: cardData.view?.sets ?? []
    // A card or a progress bar that shows the result of a formula instead of a
    // key figure (M6b.7).
    readonly property bool usesFormula: (asked.shows === "card" || asked.shows === "progress")
        && asked.usesFormula === true
    readonly property var formulaView: formulaData.view ?? ({})
    readonly property var formulaResult: formulaView.result
    // Headings, notices and boxes show no figures: nothing is asked of the
    // book for them, so there is nothing that can fail (M6-B19).
    // The budget bars (M6c.5) are drawn by `BudgetBars` from `BookBudgets`.
    readonly property bool shownAsBudgets: asked.shows === "budget-progress"
    readonly property bool textOnly: asked.shows !== undefined && !shownAsBudgets && !["chart", "table", "card", "progress"].includes(asked.shows)
    readonly property bool failed: usesFormula || textOnly ? false
        : shownAsBudgets ? budgetData.view?.error !== undefined
        : asked.shows === "card" || asked.shows === "progress"
        ? cardData.view?.error !== undefined : chartData.view?.error !== undefined
    // What the object that drew the component says about its filters.
    readonly property var filterView: shownAsBudgets ? budgetData.view?.filter
        : usesFormula ? formulaView.filter
        : asked.shows === "card" || asked.shows === "progress" ? cardData.view?.filter : chartData.view?.filter
    // A filter that chooses nothing says so; it is not an empty period.
    readonly property bool filterNothing: filterView?.nothing ?? false
    readonly property bool nothing: !failed && !filterNothing && !usesFormula && !textOnly && !shownAsBudgets && (asked.shows === "card" || asked.shows === "progress"
        ? cardSets.length === 0 : charts.length === 0)
    // Whether `FilterNotes` has anything to say (the same test as its own
    // `quiet`), so it is only made when it has.
    readonly property bool hasFilterNotes: (filterView?.own?.length ?? 0) > 0
        || (filterView?.filtered ?? []).some(word => word !== "period")
        || (filterView?.notes?.length ?? 0) > 0
    // What to call it when the user gave it no title (the grid editor's
    // built-in components have none, D-316).
    property string fallbackTitle: ""
    readonly property string title: {
        const field = (subject?.fields ?? []).find(f => f.key === "title");
        return words.resolve(field && field.text !== "" ? field.text : fallbackTitle, period);
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    KpiNames {
        id: names
    }

    TitleWords {
        id: words
    }

    BookCharts {
        id: chartData
    }

    BookCards {
        id: cardData
    }

    BookFormula {
        id: formulaData
    }

    BookBudgets {
        id: budgetData
    }

    // The card of a formula's result, in the shape the cards of the key
    // figures have, so `KpiCard` shows it like one of them (M6b.7).
    function formulaCard(result: var): var {
        return {
            measure: "formula",
            value: result.value,
            exactValue: result.exactValue,
            negative: result.negative,
            previous: "",
            change: "",
            changePercent: "",
            direction: "none"
        };
    }

    // Asks again, e.g. after the book changed.
    function refresh() {
        show();
    }

    function show() {
        // Read from `subject` itself: this runs from its change signal, before
        // the bindings that depend on it (`asked`) have been worked out again.
        const want = subject?.preview;
        const filters = scope ?? want?.scope ?? "";
        budgetData.setScope(filters);
        chartData.setScope(filters);
        cardData.setScope(filters);
        formulaData.setScope(filters);
        if (want?.shows === "budget-progress" && period !== "") {
            budgetData.show(period, want.only);
            return;
        }
        if (want === undefined || period === "" || !["chart", "table", "card", "progress"].includes(want.shows)) {
            return;
        }
        if (want.shows === "progress" && want.usesFormula === true) {
            formulaData.showBar(period, want.formula, want.target);
        } else if (want.shows === "card" && want.usesFormula === true) {
            formulaData.show(period, want.formula, false);
        } else if (want.shows === "card" || want.shows === "progress") {
            cardData.showDataset(subject.dataset, period, want.compare);
        } else {
            chartData.showWith(subject.dataset, want.chart, period, want.grouping, want.chosen + ";" + want.only + ";" + want.columns, want.showHidden);
        }
    }

    // The card of the measure in each currency's set.
    readonly property var shownCards: cardSets
        .map(set => set.cards.find(card => card.measure === asked.measure))
        .filter(card => card !== undefined)

    onSubjectChanged: show()
    onPeriodChanged: show()
    onScopeChanged: show()
    Component.onCompleted: show()

    spacing: Kirigami.Units.largeSpacing

    // The messages are made only when there is something to say: a message
    // that is made and hidden still costs its whole subtree every time a
    // component is made, and the Overview makes one per widget (M6-B30).
    Loader {
        Layout.fillWidth: true
        active: preview.failed
        visible: active

        sourceComponent: Kirigami.InlineMessage {
            visible: true
            type: Kirigami.MessageType.Warning
            text: preview.i18nContext.i18nc("@info", "This couldn't be shown right now. Try another period, or open the page again.")
        }
    }

    // A category that is in more than one column counts in each of them
    // (D-339): said plainly, never refused.
    Loader {
        Layout.fillWidth: true
        active: preview.shared.length > 0
        visible: active

        sourceComponent: Kirigami.InlineMessage {
            visible: true
            type: Kirigami.MessageType.Information
            text: preview.shared.length === 0 ? "" : preview.i18nContext.i18nc("@info %1 is the name of a category, %2 the names of the columns it is in", "“%1” is in more than one column (%2), so it is counted in each of them. Added together, the columns count it twice.", preview.shared[0].category, preview.shared[0].columns.join(", "))
        }
    }

    // The marks and notes about the filters, under the figures' own notes
    // (D-350). Nothing is drawn when the component is not filtered.
    Loader {
        Layout.fillWidth: true
        active: preview.hasFilterNotes
        visible: active

        sourceComponent: FilterNotes {
            filter: preview.filterView
        }
    }

    Loader {
        Layout.fillWidth: true
        active: preview.nothing
        visible: active

        sourceComponent: Kirigami.PlaceholderMessage {
            text: preview.i18nContext.i18nc("@info", "There is nothing to show for this period yet. Add transactions, or choose another period.")
        }
    }

    Repeater {
        model: preview.asked.shows === "chart" ? preview.charts : []

        ChartView {
            required property var modelData

            Layout.fillWidth: true
            chart: modelData
            title: preview.title
        }
    }

    Repeater {
        model: preview.asked.shows === "table" ? preview.charts : []

        ColumnLayout {
            id: tableBox

            required property var modelData

            Layout.fillWidth: true

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 3
                text: preview.title
                visible: text.length > 0
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
            }

            ChartTable {
                Layout.fillWidth: true
                chart: tableBox.modelData
            }
        }
    }

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 3
        text: preview.title
        visible: text.length > 0 && preview.shownAsBudgets
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    Loader {
        Layout.fillWidth: true
        active: preview.shownAsBudgets
        visible: active

        sourceComponent: BudgetBars {
            shown: budgetData.view?.bars ?? []
            budgetCount: budgetData.view?.count ?? 0
            arranging: true
        }
    }

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 3
        text: preview.title
        visible: text.length > 0 && (preview.asked.shows === "card" || preview.asked.shows === "progress")
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    Loader {
        Layout.fillWidth: true
        active: preview.usesFormula && preview.formulaView.empty === true
        visible: active

        sourceComponent: Kirigami.PlaceholderMessage {
            text: preview.i18nContext.i18nc("@info", "Write a formula in the properties to see its figure here.")
        }
    }

    // The mistake of a formula takes the place of its figure, in words.
    Loader {
        Layout.fillWidth: true
        active: preview.usesFormula && (preview.formulaView.mistake !== undefined || preview.formulaView.error !== undefined)
        visible: active

        sourceComponent: FormError {
            result: preview.formulaView.mistake !== undefined ? preview.formulaView.mistake.error : preview.formulaView.error
        }
    }

    Repeater {
        model: preview.asked.shows === "card" && preview.usesFormula && preview.formulaResult !== undefined
            ? [preview.formulaCard(preview.formulaResult)] : []

        KpiCard {
            Layout.fillWidth: true
        }
    }

    Repeater {
        model: preview.asked.shows === "progress" && preview.usesFormula && preview.formulaResult !== undefined
            ? [preview.formulaResult] : []

        ProgressMeter {
            required property var modelData

            Layout.fillWidth: true
            title: ""
            // "312,40 € of 400,00 €" when the bar has a target.
            valueText: (modelData.target ?? "") !== ""
                ? preview.i18nContext.i18nc("@info %1 is the figure a progress bar shows, %2 what a full bar stands for", "%1 of %2", modelData.value, modelData.target)
                : modelData.value
            permille: modelData.permille ?? 0
            // Only a percent, or a figure with a target, fills the bar; any
            // other figure is shown alone.
            hasValue: modelData.permille !== undefined
        }
    }

    Repeater {
        model: preview.asked.shows === "card" && !preview.usesFormula ? preview.shownCards : []

        KpiCard {
            Layout.fillWidth: true
        }
    }

    Repeater {
        model: preview.asked.shows === "progress" && !preview.usesFormula ? preview.shownCards : []

        ProgressMeter {
            required property var modelData

            Layout.fillWidth: true
            title: names.measure("savings-rate")
            hasValue: modelData.value !== ""
            valueText: modelData.value !== "" ? modelData.value : names.noValue()
            permille: modelData.permille ?? 0
        }
    }
}
