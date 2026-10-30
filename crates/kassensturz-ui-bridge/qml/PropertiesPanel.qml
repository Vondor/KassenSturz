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
    // The period a formula's result is worked out for: a fixed period's name
    // or a month as `YYYY-MM` (M6b.7). The host passes what the component
    // shows.
    property string period: "last-12-months"
    // A `CategoryGroups`: where the groups and categories a chart can show
    // are listed from. Without it, no list can be picked from.
    property var choices: null
    readonly property var periodChoice: view.period

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function datasetName(id: string): string {
        switch (id) {
        case "spending_by_category":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "What you spent, by category");
        case "category_over_time":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "Categories and groups you pick, month by month");
        case "income_expense_by_month":
            return i18nContext.i18nc("@item:inlistbox what a chart shows", "Money in and money out, by month");
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
        }
        return "";
    }

    function fieldLabel(key: string): string {
        switch (key) {
        case "title":
            return i18nContext.i18nc("@label:textbox a title above a chart", "Title");
        case "text":
            return i18nContext.i18nc("@label:textbox the words of a heading in the layout", "Text");
        case "groups":
            return i18nContext.i18nc("@label which of My groups a chart shows", "Groups to show");
        case "categories":
            return i18nContext.i18nc("@label which categories and groups a chart or table is limited to", "Only show these categories");
        case "series":
            return i18nContext.i18nc("@label which categories and groups a chart over time draws a line for", "Lines to draw");
        case "legend":
            return i18nContext.i18nc("@option:check", "Show the legend");
        case "grouping":
            return i18nContext.i18nc("@label:listbox how the figures are grouped", "Group by");
        case "show_hidden":
            return i18nContext.i18nc("@option:check", "Show hidden categories");
        case "compare":
            return i18nContext.i18nc("@label:listbox", "Compare with");
        case "measure":
            return i18nContext.i18nc("@label:listbox which figure a card shows", "Show");
        case "formula":
            return i18nContext.i18nc("@label:textbox a formula that works out a figure", "Formula");
        case "target":
            return i18nContext.i18nc("@label:textbox a formula for what a full progress bar stands for", "Target");
        }
        return "";
    }

    function wordName(word: string): string {
        switch (word) {
        case "category":
            return i18nContext.i18nc("@item:inlistbox", "Each category");
        case "top_level_category":
            return i18nContext.i18nc("@item:inlistbox", "Main categories, with their subcategories");
        case "groups":
            return i18nContext.i18nc("@item:inlistbox", "My groups, with the other categories on their own");
        case "month":
            return i18nContext.i18nc("@item:inlistbox", "Month");
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
        case "previous_period":
            return i18nContext.i18nc("@item:inlistbox", "The period before");
        case "same_last_year":
            return i18nContext.i18nc("@item:inlistbox", "The same days a year earlier");
        case "income":
            return i18nContext.i18nc("@item:inlistbox", "Money in");
        case "expenses":
            return i18nContext.i18nc("@item:inlistbox", "Money out");
        case "net":
            return i18nContext.i18nc("@item:inlistbox", "What is left");
        case "savings-rate":
            return i18nContext.i18nc("@item:inlistbox", "Savings rate");
        case "formula":
            return i18nContext.i18nc("@item:inlistbox a key figure the user works out with a formula", "A formula I write");
        }
        return "";
    }

    // What can be ticked for a list: the groups of money spent (`groups`), or
    // the categories and groups of both kinds (`series`).
    function idOptions(key: string): var {
        const book = choices?.view ?? ({});
        const groups = (book.groups ?? [])
            .filter(group => (key !== "groups" && key !== "categories") || !group.income)
            .map(group => ({id: group.id, name: i18nContext.i18nc("@option:check %1 is the name of a group", "%1 (group)", group.name)}));
        if (key === "groups") {
            return groups;
        }
        const categories = (book.categories ?? [])
            .filter(category => key !== "categories" || !category.income)
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
            visible: panel.datasets.length > 1
            currentIndex: panel.datasets.indexOf(panel.view.dataset)
            onActivated: index => panel.model.setDataset(panel.datasets[index])
        }

        Controls.ComboBox {
            Kirigami.FormData.label: i18nContext.i18nc("@label:listbox which period a component shows", "Period")
            Accessible.name: Kirigami.FormData.label
            model: panel.periodChoice !== undefined ? panel.wordOptions(panel.periodChoice.words) : []
            textRole: "name"
            valueRole: "word"
            visible: panel.withPeriod && panel.periodChoice !== undefined
            currentIndex: panel.periodChoice !== undefined ? panel.periodChoice.words.indexOf(panel.periodChoice.word) : -1
            onActivated: index => panel.model.setWord("period", panel.periodChoice.words[index])
        }

        Controls.Label {
            Kirigami.FormData.label: ""
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: panel.datasetHelp(panel.view.dataset)
            textFormat: Text.PlainText
            visible: text !== ""
        }

        Repeater {
            model: panel.fields

            delegate: Loader {
                id: row

                required property var modelData

                Kirigami.FormData.label: modelData.kind === "flag" ? "" : panel.fieldLabel(modelData.key)
                Kirigami.FormData.labelAlignment: modelData.kind === "ids" || modelData.kind === "formula" ? Qt.AlignTop : Qt.AlignVCenter
                Layout.fillWidth: true
                sourceComponent: modelData.kind === "text" ? textField
                    : modelData.kind === "flag" ? flagBox
                    : modelData.kind === "formula" ? formulaBox
                    : modelData.kind === "ids" ? idsBox : choiceBox

                Component {
                    id: textField

                    Controls.TextField {
                        text: row.modelData.text
                        maximumLength: 200
                        Accessible.name: panel.fieldLabel(row.modelData.key)
                        onEditingFinished: panel.model.setText(row.modelData.key, text)
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
                        text: panel.fieldLabel(row.modelData.key)
                        checked: row.modelData.flag
                        onToggled: panel.model.setFlag(row.modelData.key, checked)
                    }
                }

                Component {
                    id: idsBox

                    ColumnLayout {
                        id: picker

                        readonly property var options: panel.idOptions(row.modelData.key)

                        spacing: 0

                        Controls.Label {
                            Layout.fillWidth: true
                            visible: picker.options.length === 0
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: row.modelData.key === "categories"
                                ? panel.i18nContext.i18nc("@info", "There are no categories to pick yet.")
                                : panel.i18nContext.i18nc("@info", "There is nothing to pick yet. Make groups on the page My groups and hidden categories first.")
                        }

                        Controls.ScrollView {
                            Layout.fillWidth: true
                            Layout.preferredHeight: Math.min(picker.options.length, 6) * Kirigami.Units.gridUnit * 1.3
                            visible: picker.options.length > 0
                            Accessible.name: panel.fieldLabel(row.modelData.key)

                            ListView {
                                model: picker.options
                                clip: true
                                delegate: Controls.CheckBox {
                                    required property var modelData

                                    width: ListView.view.width
                                    text: modelData.name
                                    checked: row.modelData.ids.includes(modelData.id)
                                    onToggled: {
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
                        model: panel.wordOptions(row.modelData.words)
                        textRole: "name"
                        valueRole: "word"
                        currentIndex: row.modelData.words.indexOf(row.modelData.word)
                        Accessible.name: panel.fieldLabel(row.modelData.key)
                        onActivated: index => panel.model.setWord(row.modelData.key, row.modelData.words[index])
                    }
                }
            }
        }
    }
}
