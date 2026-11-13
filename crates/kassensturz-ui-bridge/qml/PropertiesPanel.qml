// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The properties of one chart, card or table (M6a.6, D-278): a field for every
// property the property schema gives its type, and none for anything else.
// The panel decides nothing itself. It shows `model.view.fields` and passes
// every change to the model, which refuses what the schema doesn't allow and
// then shows the component as it was, so the panel can never hold a value the
// document could not (D-281). Words are looked up by their fixed key (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: panel

    // A `ComponentProperties` (or anything with its `view` and `set...`).
    required property var model
    readonly property var view: model.view
    readonly property var fields: view.fields !== undefined ? view.fields : []
    readonly property var datasets: view.datasets !== undefined ? view.datasets : []
    // The grid editor's panel offers the period of the component; the
    // Statistics workbench has a period of its own (D-313).
    property bool withPeriod: false
    // Only the fields with these keys, when there are some: the steps of the
    // Statistic Builder's wizard show the panel a part at a time (D-335).
    property var only: []
    // Whether the box for the data (what a chart shows) is offered.
    property bool showDataset: true
    // The period a formula's result is worked out for: a fixed period's name
    // or a month as `YYYY-MM` (M6b.7). The host passes what the component
    // shows.
    property string period: "last-12-months"
    // A `CategoryGroups`: where the groups and categories a chart can show
    // are listed from. Without it, no list can be picked from.
    property var choices: null
    readonly property var periodChoice: view.period
    // Once the statistic has columns, they decide what it shows: the older
    // lists of categories would be ignored, so they are not offered.
    readonly property bool hasColumns: {
        const text = view.columns !== undefined ? String(view.columns) : "";
        return text.trim() !== "" && text.trim() !== "[]";
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property TitleWords titleWords: TitleWords {}
    readonly property PeriodRange periodRange: PeriodRange {}
    // The lengths a candle chart's *One candle per* offers for the period
    // shown (D-747): Automatic, and each length that gives two candles.
    readonly property var candleWords: periodRange.candleWords(period)
    // How far each list of ticks was scrolled, by field key.
    property var scrolledTo: ({})

    // The fields by their key, for the rows below.
    readonly property var fieldByKey: {
        // `fields` is undefined while the panel is still being made.
        const list = fields ?? [];
        const byKey = {};
        for (let i = 0; i < list.length; ++i) {
            byKey[list[i].key] = list[i];
        }
        return byKey;
    }

    // One row for each field key the panel has shown, kept while the panel
    // lives. Every change gives `fields` as a new list, and a Repeater on it
    // rebuilt every row; Kirigami's FormLayout logs errors for each row that
    // is taken away ("Cannot read property 'isSection' of null", M6-B99). So
    // a row is only ever added, in the order of the fields, and a key that is
    // no longer among the fields is hidden.
    ListModel {
        id: fieldRows
    }

    function placeFieldRows(): void {
        const list = fields ?? [];
        let at = 0;
        for (let n = 0; n < list.length; ++n) {
            const field = list[n];
            let found = -1;
            for (let i = 0; i < fieldRows.count; ++i) {
                if (fieldRows.get(i).key === field.key) {
                    found = i;
                    break;
                }
            }
            if (found === -1) {
                fieldRows.insert(at, {key: field.key});
                at += 1;
            } else if (found >= at) {
                at = found + 1;
            } else {
                fieldRows.move(found, at - 1, 1);
            }
        }
    }

    onFieldsChanged: placeFieldRows()
    Component.onCompleted: placeFieldRows()

    function datasetName(id: string): string {
        switch (id) {
        case "spending_by_category":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "What you spent, by category");
        case "category_over_time":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "Categories and groups you pick, month by month");
        case "income_expense_by_month":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "Money in and money out, by month");
        case "budget_utilization":
            return i18nContext.i18nc("@item:inlistbox what a component shows", "How much of each budget is spent and left");
        case "account_balances":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "What your accounts hold");
        case "net_worth_over_time":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "What you are worth, month by month");
        case "cash_flow_by_month":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "Money in and out of your accounts, by month");
        case "money_flow":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "Where your money came from and where it went");
        }
        return "";
    }

    function datasetHelp(id: string): string {
        switch (id) {
        case "spending_by_category":
            return i18nContext.i18nc("@info", "Shows what went out in the period. A refund reduces its category. Money moved between your own accounts is not counted.");
        case "category_over_time":
            return i18nContext.i18nc("@info", "Shows what you spent or received in each month for the categories and groups you pick, one line each. A group counts all its categories, hidden ones too.");
        case "income_expense_by_month":
            return i18nContext.i18nc("@info", "Shows what came in and what went out in each month. Money moved between your own accounts is not counted.");
        case "budget_utilization":
            return i18nContext.i18nc("@info", "Shows, for each budget, what is planned, what is spent and what is left in its period. Leave the list empty to show every budget.");
        case "account_balances":
            return i18nContext.i18nc("@info", "Shows what each account held at the end of the period, and what you have, what you owe and what you are worth. Each currency is shown on its own; they are never added together.");
        case "net_worth_over_time":
            return i18nContext.i18nc("@info", "Shows what you have, what you owe and what you are worth at the end of each month. Each currency is shown on its own; they are never added together.");
        case "money_flow":
            return i18nContext.i18nc("@info", "Shows where the money of the period came from and where it went, as bands as wide as their amount. If you earned more than you spent, the rest is shown as left over; if you spent more, as taken from your reserves. Money moved between your own accounts is not counted. One currency is shown, with no conversion.");
        case "cash_flow_by_month":
            return i18nContext.i18nc("@info", "Shows the money that came into and went out of your checking, savings and cash accounts in each month, and what they held at the start and at the end. Moving money between two of them counts as neither, and starting balances are listed on their own.");
        }
        return "";
    }

    function fieldLabel(key: string): string {
        switch (key) {
        case "title":
            return i18nContext.i18nc("@label:textbox a title above a chart", "Title");
        case "text":
            return i18nContext.i18nc("@label:textbox the words of a heading in the layout", "Text");
        case "spec":
            return i18nContext.i18nc("@label:textbox the text that describes a custom chart", "Chart description");
        case "groups":
            return i18nContext.i18nc("@label which of My groups a chart shows", "Groups to show");
        case "categories":
            return i18nContext.i18nc("@label which categories and groups a chart or table is limited to", "Only show these categories");
        case "accounts":
            return i18nContext.i18nc("@label which accounts a component counts, whatever the filters of the page say", "Only count these accounts");
        case "series":
            return i18nContext.i18nc("@label which categories and groups a chart over time draws a line for", "Lines to draw");
        case "income_categories":
            return i18nContext.i18nc("@label which categories and groups the money in of a chart counts, for money in only", "Money in: only these categories");
        case "income_accounts":
            return i18nContext.i18nc("@label which accounts the money in of a chart counts, for money in only", "Money in: only these accounts");
        case "expenses_categories":
            return i18nContext.i18nc("@label which categories and groups the money out of a chart counts, for money out only", "Money out: only these categories");
        case "expenses_accounts":
            return i18nContext.i18nc("@label which accounts the money out of a chart counts, for money out only", "Money out: only these accounts");
        case "budgets":
            return i18nContext.i18nc("@label which budgets the budget bars show", "Which budgets");
        case "tags":
            return i18nContext.i18nc("@label which tags the money of a chart, table or card is limited to", "Only count these tags");
        case "alarms":
            return i18nContext.i18nc("@label the rules of the warnings tile", "Warnings");
        case "legend":
            return i18nContext.i18nc("@option:check", "Show the legend");
        case "grouping":
            return i18nContext.i18nc("@label:listbox how the figures are grouped", "Group by");
        case "average":
            return i18nContext.i18nc("@label:listbox whether a chart shows the total spending or an average per month, quarter or year", "Show as");
        case "orientation":
            return i18nContext.i18nc("@label:listbox which way the bars of a bar chart go", "Bars");
        case "candle":
            return i18nContext.i18nc("@label:listbox how many days one candle of a candle chart covers", "One candle per");
        case "candle_fill":
            return i18nContext.i18nc("@label:listbox which boxes of a candle chart are filled with color and which are hollow", "Candle boxes");
        case "show_hidden":
            return i18nContext.i18nc("@option:check", "Show hidden categories");
        // The money-flow chart (US7, #821, D-758).
        case "sub_categories":
            return i18nContext.i18nc("@option:check a money-flow chart: sub-categories get a column of their own", "Show sub-categories");
        case "payees":
            return i18nContext.i18nc("@option:check a money-flow chart: who paid and who was paid get a column at each edge", "Show who paid and who was paid");
        case "currency":
            return i18nContext.i18nc("@label:textbox a money-flow chart: the currency it shows, as a code like EUR; empty for the one with the most money", "Currency (a code like EUR; empty: the one with most money)");
        case "hide_note":
            return i18nContext.i18nc("@option:check hides the grey lines on a tile that say what it is filtered to", "Hide the grey filter notes");
        case "compare":
            return i18nContext.i18nc("@label:listbox", "Compare with");
        // US6, #789 (D-760): small changes against the comparison stay
        // without green or red.
        case "neutral":
            return i18nContext.i18nc("@label:listbox which changes against the comparison stay without green or red", "No colour for");
        case "measure":
            return i18nContext.i18nc("@label:listbox which figure a card shows", "Show");
        case "formula":
            return i18nContext.i18nc("@label:textbox a formula that works out a figure", "Formula");
        case "target":
            return i18nContext.i18nc("@label:textbox a formula for what a full progress bar stands for", "Target");
        case "month_start":
            return i18nContext.i18nc("@label:listbox the day the month of the Left this month tile starts on", "Month starts on");
        }
        return "";
    }

    // The label on the left of a field's row. A switch carries its own
    // words, so its row has none, except the switch for the grey filter
    // notes: under a list of accounts it was easy to miss, so it has a row
    // of its own, *Filter notes* (US2.4, #605, D-680).
    function rowLabel(field: var): string {
        if (field.key === "hide_note") {
            return i18nContext.i18nc("@label the row with the switch for the grey lines on a tile that say what it is filtered to", "Filter notes");
        }
        return field.kind === "flag" ? "" : fieldLabel(field.key);
    }

    function wordName(word: string): string {
        // Left this month's "Month starts on" (#921): a day of the month.
        if (word.startsWith("day-")) {
            const day = Number(word.slice(4));
            return day === 1
                ? i18nContext.i18nc("@item:inlistbox the month starts on day 1", "Day 1 (calendar month)")
                : i18nContext.i18nc("@item:inlistbox %1 is a day of the month the month starts on, e.g. the salary day", "Day %1", day);
        }
        switch (word) {
        case "like-budgets":
            return i18nContext.i18nc("@item:inlistbox the month starts on the day the user's salary-month budgets start on", "Like my budgets");
        case "category":
            return i18nContext.i18nc("@item:inlistbox", "Each category");
        case "top_level_category":
            return i18nContext.i18nc("@item:inlistbox", "Main categories, with their subcategories");
        case "groups":
            return i18nContext.i18nc("@item:inlistbox", "My groups, with the other categories on their own");
        case "month":
            return i18nContext.i18nc("@item:inlistbox", "Month");
        case "day":
            return i18nContext.i18nc("@item:inlistbox one candle of a candle chart covers a day", "Day");
        case "week":
            return i18nContext.i18nc("@item:inlistbox one candle of a candle chart covers a week, Monday to Sunday", "Week (Monday to Sunday)");
        case "up-hollow":
            return i18nContext.i18nc("@item:inlistbox candle boxes: a candle that ended higher is hollow, one that ended lower filled", "Rising hollow, falling filled");
        case "down-hollow":
            return i18nContext.i18nc("@item:inlistbox candle boxes: a candle that ended higher is filled, one that ended lower hollow", "Rising filled, falling hollow");
        case "all-filled":
            return i18nContext.i18nc("@item:inlistbox candle boxes: every box filled, rising and falling differ by color only", "All filled");
        case "all-hollow":
            return i18nContext.i18nc("@item:inlistbox candle boxes: every box hollow, rising and falling differ by color only", "All hollow");
        case "flow":
            return i18nContext.i18nc("@item:inlistbox the one way the money flow is shown", "Bands from where it came to where it went");
        case "budget":
            return i18nContext.i18nc("@item:inlistbox", "Each budget");
        case "period":
            return i18nContext.i18nc("@item:inlistbox", "Each period of a budget");
        case "account":
            return i18nContext.i18nc("@item:inlistbox", "Each account");
        case "account_kind":
            return i18nContext.i18nc("@item:inlistbox", "What you have, what you owe and what you are worth");
        case "series":
            return i18nContext.i18nc("@item:inlistbox", "The categories and groups you picked");
        case "page":
            return i18nContext.i18nc("@item:inlistbox a period: the one the page shows", "The period of the page");
        case "this-month":
            return i18nContext.i18nc("@item:inlistbox a period", "This month");
        case "last-month":
            return i18nContext.i18nc("@item:inlistbox a period", "Last month");
        case "last-12-months":
            return i18nContext.i18nc("@item:inlistbox a period", "The last 12 months");
        case "this-year":
            return i18nContext.i18nc("@item:inlistbox a period", "This year");
        case "last-year":
            return i18nContext.i18nc("@item:inlistbox a period", "Last year");
        case "total":
            return i18nContext.i18nc("@item:inlistbox show spending as what it adds up to over the period", "Total");
        case "per-month":
            return i18nContext.i18nc("@item:inlistbox show spending as an average", "Average per month");
        case "per-quarter":
            return i18nContext.i18nc("@item:inlistbox show spending as an average", "Average per quarter");
        case "per-year":
            return i18nContext.i18nc("@item:inlistbox show spending as an average", "Average per year");
        case "auto":
            return i18nContext.i18nc("@item:inlistbox the bars of a chart go the way that fits the names best", "Automatic");
        case "horizontal":
            return i18nContext.i18nc("@item:inlistbox the bars of a chart go across, one row per name", "Across (one row per name)");
        case "vertical":
            return i18nContext.i18nc("@item:inlistbox the bars of a chart stand upright, the names below them", "Upright (names below)");
        case "previous_period":
            return i18nContext.i18nc("@item:inlistbox", "The period before");
        case "same_last_year":
            return i18nContext.i18nc("@item:inlistbox", "The same days a year earlier");
        // Which changes of a comparison stay uncoloured (D-760).
        case "off":
            return i18nContext.i18nc("@item:inlistbox no colour for: every change is green or red", "Nothing, colour every change");
        case "2":
        case "5":
        case "10":
        case "20":
            return i18nContext.i18nc("@item:inlistbox no colour for: %1 is a whole number of percent", "Changes smaller than %1 %", word);
        // What a spending chart compares each category with (US3.1a).
        case "none":
            return i18nContext.i18nc("@item:inlistbox a chart compares its spending with nothing", "None");
        case "average-3":
            return i18nContext.i18nc("@item:inlistbox a chart compares its spending with the average of earlier periods", "The average of the 3 periods before");
        case "average-6":
            return i18nContext.i18nc("@item:inlistbox a chart compares its spending with the average of earlier periods", "The average of the 6 periods before");
        case "average-12":
            return i18nContext.i18nc("@item:inlistbox a chart compares its spending with the average of earlier periods", "The average of the 12 periods before");
        case "income":
        case "money_in":
            return i18nContext.i18nc("@item:inlistbox", "Money in");
        case "expenses":
        case "money_out":
            return i18nContext.i18nc("@item:inlistbox", "Money out");
        case "net_change":
            return i18nContext.i18nc("@item:inlistbox a key figure: the money that came into the accounts less the money that went out", "Net change");
        case "net":
            return i18nContext.i18nc("@item:inlistbox", "What is left");
        case "net_worth":
            return i18nContext.i18nc("@item:inlistbox a key figure", "What you are worth");
        case "assets":
            return i18nContext.i18nc("@item:inlistbox a key figure", "What you have");
        case "liabilities":
            return i18nContext.i18nc("@item:inlistbox a key figure", "What you owe");
        case "savings-rate":
            return i18nContext.i18nc("@item:inlistbox", "Savings rate");
        case "formula":
            return i18nContext.i18nc("@item:inlistbox a key figure the user works out with a formula", "A formula I write");
        }
        return "";
    }

    // What can be ticked for a list: the groups of money spent (`groups`), the
    // categories and groups of both kinds (`series`), or those of money in or
    // money out only for a series' own list (D-646).
    function idOptions(key: string): var {
        const book = choices?.view ?? ({});
        if (key === "accounts" || key === "income_accounts" || key === "expenses_accounts") {
            return (book.accounts ?? []).map(account => ({id: account.id, name: account.closed ? i18nContext.i18nc("@option:check %1 is the name of a closed account", "%1 (closed)", account.name) : account.name}));
        }
        // The budget bars' Which budgets (D-642).
        if (key === "budgets") {
            return (book.budgets ?? []).map(budget => ({id: budget.id, name: budget.name}));
        }
        // Only count these tags (US5.2, D-714).
        if (key === "tags") {
            return (book.tags ?? []).map(tag => ({id: tag.id, name: tag.name}));
        }
        // A budget belongs to a category, not to a group (D-357).
        const forBudgets = view.preview?.shows === "budget-progress" || view.dataset === "budget_utilization";
        const groups = (forBudgets ? [] : book.groups ?? [])
            .filter(group => key === "income_categories" ? group.income
                : (key !== "groups" && key !== "categories" && key !== "expenses_categories") || !group.income)
            .map(group => ({id: group.id, name: i18nContext.i18nc("@option:check %1 is the name of a group", "%1 (group)", group.name)}));
        if (key === "groups") {
            return groups;
        }
        const categories = (book.categories ?? [])
            .filter(category => key === "income_categories" ? category.income
                : (key !== "categories" && key !== "expenses_categories") || !category.income)
            .map(category => ({id: category.id, name: category.path ?? category.name}));
        return groups.concat(categories);
    }

    function wordOptions(words): var {
        return words.map(word => ({word: word, name: wordName(word)}));
    }

    spacing: Kirigami.Units.largeSpacing

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: panel.view.error !== undefined
        type: Kirigami.MessageType.Warning
        text: i18nContext.i18nc("@info", "That setting can't be used here, so it was left as it was. Choose one of the offered values.")
    }

    Kirigami.FormLayout {
        Layout.fillWidth: true

        Controls.ComboBox {
            id: datasetBox

            Kirigami.FormData.label: i18nContext.i18nc("@label:listbox what a chart shows", "Data")
            Accessible.name: Kirigami.FormData.label
            // Only the datasets this type can show.
            model: panel.datasets.map(id => ({word: id, name: panel.datasetName(id)}))
            textRole: "name"
            valueRole: "word"
            visible: panel.showDataset && panel.datasets.length > 1
            currentIndex: panel.datasets.indexOf(panel.view.dataset)
            onActivated: index => {
                ActionLog.taken("PropertiesPanel_setDataset")
                panel.model.setDataset(panel.datasets[index])
            }
        }

        PeriodChoice {
            Kirigami.FormData.label: i18nContext.i18nc("@label:listbox which period a component shows", "Period")
            options: panel.periodChoice !== undefined ? panel.wordOptions(panel.periodChoice.words) : []
            word: panel.periodChoice?.word ?? ""
            visible: panel.withPeriod && panel.periodChoice !== undefined
            onChosen: word => {
                ActionLog.taken("PropertiesPanel_setWord")
                panel.model.setWord("period", word)
            }
        }

        // A form is as wide as its widest row, and a label that may wrap is
        // as wide as its whole text, so without a limit the text ran past the
        // window's edge instead of wrapping (M6-B153).
        Controls.Label {
            Kirigami.FormData.label: ""
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
            wrapMode: Text.Wrap
            text: panel.datasetHelp(panel.view.dataset)
            textFormat: Text.PlainText
            visible: panel.showDataset && text !== ""
        }

        Repeater {
            model: fieldRows

            delegate: Loader {
                id: row

                required property string key
                readonly property bool shown: panel.fieldByKey[key] !== undefined
                readonly property var modelData: panel.fieldByKey[key] ?? ({key: key, kind: ""})

                visible: shown && (panel.only.length === 0 || panel.only.includes(modelData.key))
                    // Uncoloured changes matter only while the chart compares (D-760).
                    && !(modelData.key === "neutral" && (panel.fieldByKey["compare"]?.word ?? "none") === "none")
                    && !(panel.hasColumns && ["categories", "series", "groups", "average", "compare", "neutral", "income_categories", "income_accounts", "expenses_categories", "expenses_accounts"].includes(modelData.key))
                active: visible
                Kirigami.FormData.label: panel.rowLabel(modelData)
                Kirigami.FormData.labelAlignment: modelData.kind === "ids" || modelData.kind === "formula" || modelData.kind === "spec" || modelData.kind === "alarms" ? Qt.AlignTop : Qt.AlignVCenter
                Layout.fillWidth: true
                // Space between the switch for the grey notes and a list
                // above it, so it doesn't read as the list's last line (D-680).
                Layout.topMargin: modelData.key === "hide_note" ? Kirigami.Units.largeSpacing : 0
                sourceComponent: modelData.kind === "text" ? textField
                    : modelData.kind === "flag" ? flagBox
                    : modelData.kind === "formula" ? formulaBox
                    : modelData.kind === "spec" ? specBox
                    : modelData.kind === "ids" ? idsBox
                    : modelData.kind === "alarms" ? alarmsBox : choiceBox

                // The warnings tile's rules (US4.9, D-722).
                Component {
                    id: alarmsBox

                    AlarmRulesField {
                        rulesText: row.modelData.text || "[]"
                        accounts: panel.idOptions("accounts")
                        budgets: panel.idOptions("budgets")
                        onRulesEdited: json => panel.model.setAlarms(json)
                    }
                }

                Component {
                    id: textField

                    ColumnLayout {
                        id: titleBox

                        // Only titles and heading texts can hold the words
                        // that stand for the period.
                        readonly property bool withWords: row.modelData.key === "title" || row.modelData.key === "text"

                        spacing: Kirigami.Units.smallSpacing

                        Controls.TextField {
                            id: entry

                            Layout.fillWidth: true
                            text: row.modelData.text
                            maximumLength: 200
                            Accessible.name: panel.fieldLabel(row.modelData.key)
                            onEditingFinished: panel.model.setText(row.modelData.key, text)
                        }

                        // The words to pick; one click puts one into the title.
                        Flow {
                            Layout.fillWidth: true
                            visible: titleBox.withWords
                            spacing: Kirigami.Units.smallSpacing

                            Controls.Label {
                                text: panel.i18nContext.i18nc("@label words that can be put into a title", "Add:")
                                height: Kirigami.Units.gridUnit * 2
                                verticalAlignment: Text.AlignVCenter
                            }

                            Repeater {
                                model: panel.titleWords.tokens

                                delegate: Controls.Button {
                                    id: addWord

                                    required property var modelData

                                    text: Plain.plain(modelData.token)
                                    flat: true
                                    Accessible.description: modelData.help
                                    Controls.ToolTip.text: modelData.help
                                    Controls.ToolTip.visible: hovered
                                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                                    onClicked: {
                                        ActionLog.taken("PropertiesPanel_clicked")
                                        entry.insert(entry.cursorPosition, modelData.token);
                                        panel.model.setText(row.modelData.key, entry.text);
                                        entry.forceActiveFocus();
                                    }
                                }
                            }
                        }

                        // What the title says for the period of the component,
                        // as it is shown.
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: titleBox.withWords && panel.titleWords.hasWords(entry.text)
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            opacity: 0.75
                            text: panel.i18nContext.i18nc("@info %1 is the title as it is shown", "Shown as: %1", panel.titleWords.resolve(entry.text, panel.period))
                        }
                    }
                }

                Component {
                    id: specBox

                    SpecPage {
                        model: panel.model
                        stored: row.modelData.text
                        partial: panel.view.preview?.partial ?? false
                    }
                }

                Component {
                    id: formulaBox

                    FormulaField {
                        text: row.modelData.text
                        period: panel.period
                        forBar: panel.view.type === "progress.bar"
                        isTarget: row.modelData.key === "target"
                        hasTarget: panel.fields.some(f => f.key === "target" && f.text.trim() !== "")
                        onCommitted: value => panel.model.setFormula(row.modelData.key, value)
                    }
                }

                Component {
                    id: flagBox

                    Controls.CheckBox {
                        text: Plain.plain(panel.fieldLabel(row.modelData.key))
                        checked: row.modelData.flag
                        onToggled: {
                            ActionLog.taken("PropertiesPanel_setFlag")
                            panel.model.setFlag(row.modelData.key, checked)
                        }
                    }
                }

                Component {
                    id: idsBox

                    ColumnLayout {
                        id: picker

                        // Built again only when what is offered changes, so
                        // a tick doesn't give the list a new model and send
                        // it back to the top.
                        readonly property string optionsText: JSON.stringify(panel.idOptions(row.modelData.key))
                        readonly property var options: JSON.parse(optionsText)

                        spacing: 0

                        Controls.Label {
                            Layout.fillWidth: true
                            visible: picker.options.length === 0
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: ["categories", "income_categories", "expenses_categories"].includes(row.modelData.key)
                                ? panel.i18nContext.i18nc("@info", "There are no categories to pick yet.")
                                : ["accounts", "income_accounts", "expenses_accounts"].includes(row.modelData.key)
                                ? panel.i18nContext.i18nc("@info", "There are no accounts to pick yet.")
                                : row.modelData.key === "budgets"
                                ? panel.i18nContext.i18nc("@info", "There are no budgets to pick yet. Set one on the page Budgets first.")
                                : row.modelData.key === "tags"
                                ? panel.i18nContext.i18nc("@info", "There are no tags to pick yet. Add one to a transaction, or in Settings › Tags.")
                                : panel.i18nContext.i18nc("@info", "There is nothing to pick yet. Make groups on the page My groups and hidden categories first.")
                        }

                        // An own list wins over the filters of the page for
                        // its own dimension only (D-350).
                        Controls.Label {
                            Layout.fillWidth: true
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                            visible: panel.withPeriod && (row.modelData.key === "accounts" || row.modelData.key === "categories")
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.smallFont
                            opacity: 0.75
                            text: panel.i18nContext.i18nc("@info", "Nothing ticked: this follows the filters of the page. Tick some to count only those, whatever the page says.")
                        }

                        // A series' own list wins over the component's and
                        // the page's, for that series only (D-646).
                        Controls.Label {
                            Layout.fillWidth: true
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                            visible: ["income_categories", "income_accounts", "expenses_categories", "expenses_accounts"].includes(row.modelData.key)
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.smallFont
                            opacity: 0.75
                            text: panel.i18nContext.i18nc("@info", "Nothing ticked: this counts like the rest of the chart. Tick some to count only those for this one series, for example money in from the salary category only.")
                        }

                        // A sum by tag counts only the tagged parts of a
                        // bill, never the whole bill (D-714).
                        Controls.Label {
                            Layout.fillWidth: true
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                            visible: row.modelData.key === "tags" && picker.options.length > 0
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.smallFont
                            opacity: 0.75
                            text: panel.i18nContext.i18nc("@info", "Nothing ticked: all money counts. Tick some to count only the money with those tags. Of a bill split into parts, only the tagged parts count. Transfers between your own accounts never count.")
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                            visible: row.modelData.key === "budgets" && picker.options.length > 0
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.smallFont
                            opacity: 0.75
                            text: panel.i18nContext.i18nc("@info", "Nothing ticked: every budget is shown.")
                        }

                        Controls.ScrollView {
                            Layout.fillWidth: true
                            Layout.preferredHeight: Math.min(picker.options.length, 6) * Kirigami.Units.gridUnit * 1.3
                            visible: picker.options.length > 0
                            Accessible.name: panel.fieldLabel(row.modelData.key)

                            ListView {
                                id: optionList

                                // A row is kept between changes (M6-B99), but
                                // it is built again when its field comes back
                                // after another type: the place in the list
                                // is kept in the panel and put back (M6-B21).
                                property bool settledAt: false

                                model: picker.options
                                clip: true
                                onContentYChanged: if (settledAt) panel.scrolledTo[row.modelData.key] = contentY
                                Component.onCompleted: Qt.callLater(() => {
                                    const kept = panel.scrolledTo[row.modelData.key];
                                    if (kept !== undefined) {
                                        optionList.contentY = Math.min(kept, Math.max(0, optionList.contentHeight - optionList.height));
                                    }
                                    optionList.settledAt = true;
                                })
                                delegate: Controls.CheckBox {
                                    required property var modelData

                                    width: ListView.view.width
                                    text: Plain.plain(modelData.name)
                                    checked: row.modelData.ids.includes(modelData.id)
                                    onToggled: {
                                        ActionLog.taken("PropertiesPanel_toggled")
                                        const without = row.modelData.ids.filter(id => id !== modelData.id);
                                        panel.model.setIds(row.modelData.key, (checked ? without.concat([modelData.id]) : without).join(","));
                                    }
                                }
                            }
                        }
                    }
                }

                Component {
                    id: choiceBox

                    Controls.ComboBox {
                        id: choice

                        // A candle chart offers only the lengths that fit
                        // its period (D-747). A stored length that doesn't
                        // fit shows as Automatic, which is what the chart
                        // draws then, and is kept for a longer period.
                        readonly property bool candle: row.modelData.key === "candle"
                        readonly property var words: candle
                            ? row.modelData.words.filter(word => panel.candleWords.includes(word))
                            : row.modelData.words
                        model: panel.wordOptions(words)
                        textRole: "name"
                        valueRole: "word"
                        currentIndex: {
                            const at = words.indexOf(row.modelData.word);
                            return at < 0 && candle ? 0 : at;
                        }
                        Accessible.name: panel.fieldLabel(row.modelData.key)
                        onActivated: index => {
                            ActionLog.taken("PropertiesPanel_setWord_2")
                            panel.model.setWord(row.modelData.key, choice.words[index])
                        }
                    }
                }
            }
        }
    }
}
