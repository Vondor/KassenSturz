// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One help page in plain words on the two things a person types that look
// like a small language: the patterns of a Rule's conditions and the
// formulas of charts and key figures (US3.13, #649, D-700), and the
// descriptions of custom charts (US4.11, #700 part A, D-720). Every example
// uses invented names (D-699). The rules it states are the ones the code
// keeps: a pattern is a Rust `regex` pattern, capitals are ignored unless
// "Capital letters must match" is ticked, and lookaround and
// back-references are not available (D-015); formulas follow LibreOffice
// Calc (D-510, D-292, D-318, D-649).

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Help: patterns, formulas and charts")

    // The ready chart descriptions, written once in Rust, and their names.
    readonly property ChartDescription examples: ChartDescription {}
    readonly property ChartExamples exampleNames: ChartExamples {}

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Patterns in Rules")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A Rule looks at the name or the purpose of a transaction. Most of the time \"contains\" is all you need: it finds the words you type anywhere in the text, and capital letters don't matter. A pattern is for the cases where that is not enough. Switch it on with \"Use a pattern (advanced)\" in a condition.")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "Either of two names")
            description: "fresho|brightmart"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "The text starts with a word")
            description: "^FRESHO"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "The text ends with four digits")
            description: "\\d{4}$"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "A word, then anything, then another word")
            description: "^FRESHO .* THANK YOU$"
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A pattern can match a part of the text, so you don't need to cover the whole text. ^ means the start of the text and $ means its end. A dot stands for any one character and .* for any amount of any characters. \\d is a digit, and a vertical bar means \"or\". To look for one of these characters itself, put a backslash before it, for example \\. for a dot.")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Patterns use the syntax of the Rust \"regex\" library. Capital letters are ignored unless \"Capital letters must match\" is ticked. Lookahead and lookbehind, such as (?=…) or (?<!…), and back-references such as \\1 don't work. If a pattern can't be read, KassenSturz says so and changes nothing.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Formulas")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A formula works like a formula in a spreadsheet such as LibreOffice Calc. A category or a group stands for its total in the chart's period, as a positive amount. You can start with = or leave it out. KassenSturz works out the formula from the exact amounts of your book and shows what it counted next to the result. A formula only reads your book; it never changes a transaction or a balance.")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "What is left after one category")
            description: "=Groceries - Pets"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "Two categories added up")
            description: "=SUM(Groceries; Pets)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "A name with spaces goes in single quotes")
            description: "='Eating out' + Travel"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "One category in a period of its own")
            description: "=Groceries@\"last month\""
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "An average per month over the last three whole months")
            description: "=Groceries@\"last 3 complete months\" / 3"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "Count Pets only when it is above 100")
            description: "=IF(Pets > 100; Pets; 0)"
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "You can use + - * / and brackets, % for a percentage, and the comparisons = <> < > <= >=. Between the parts of a function, ; works everywhere, as in LibreOffice Calc. If your number format uses a decimal point, a comma works as well. A period after @ gives a name its own period; the next part lists every period you can write. The formula field lists the functions with a short line each and completes the names of your categories while you type. A mistake is described in words, with the place marked.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Periods in formulas")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Without @, a name counts the chart's period. With @ after it, the name counts a period of its own. There are two ways to write one: a period word in double quotes, or dates without quotes.")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "The month before the chart's month")
            description: "=Groceries@\"last month\""
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "The chart's quarter")
            description: "=Groceries@\"this quarter\""
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "The chart's month and the 5 months before it")
            description: "=Groceries@\"last 6 months\""
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "The 3 whole months before the chart's month")
            description: "=Groceries@\"last 3 complete months\""
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "One year")
            description: "=Groceries@2026"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "One month")
            description: "=Groceries@2026-09"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "One day, here 12 November 2026")
            description: "=Groceries@2026-11-12"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "From one day to another, both included")
            description: "=Groceries@2026-11-01..2026-11-12"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info an example of a period in a formula", "From one month to another")
            description: "=Groceries@2026-01..2026-03"
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "These are all the period words: \"this month\", \"last month\", \"this quarter\", \"last quarter\", \"this year\", \"last year\", \"last N months\" and \"last N complete months\", with N from 1 to 120. They stay in English, like the function names, and capital letters don't matter. A word counts from the chart's period: in a chart for November, \"last month\" is October. \"last N months\" includes the chart's month, \"last N complete months\" doesn't.")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Dates are written year first, with a minus between year, month and day: 2026 is a year, 2026-11 a month and 2026-11-12 a day. Two of them joined by .. are a range from the first day of the first to the last day of the second, up to 100 years long. Other ways of writing a date, such as 12.11.26, and words for days or weeks don't work yet; a period like that is written with dates, for example 2026-09 for the month before last in November.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "All functions")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "These are all the functions a formula can use. Their names stay in English, as in the formula field. Where a function takes several figures, you can give as many as you need, each separated by ;. A test is a comparison such as Pets > 100.")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "SUM: adds the figures up.")
            description: "=SUM(Groceries; Pets; Travel)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "AVERAGE: the sum divided by how many figures there are.")
            description: "=AVERAGE(Groceries; Pets; Travel)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "MIN: the smallest of the figures.")
            description: "=MIN(Groceries; Pets)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "MAX: the largest of the figures.")
            description: "=MAX(Groceries; Pets)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "COUNT: how many transactions the categories or groups have in the period. A transaction in more than one of them counts once.")
            description: "=COUNT(Groceries; Pets)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "ABS: the figure without its minus sign.")
            description: "=ABS(Salary - Groceries)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "SIGN: -1 for a figure below zero, 0 for zero and 1 for a figure above zero.")
            description: "=SIGN(Salary - Groceries)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "ROUND: rounds to that many decimal places, 5 and up away from zero. Without places it rounds to a whole number; -1 rounds to tens, -2 to hundreds.")
            description: "=ROUND(Groceries / 3; 2)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "ROUNDUP: rounds away from zero to that many decimal places.")
            description: "=ROUNDUP(Groceries / 3; 0)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "ROUNDDOWN: rounds toward zero to that many decimal places.")
            description: "=ROUNDDOWN(Groceries / 3; 1)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "INT: rounds down to a whole number. Below zero that is away from zero, as in LibreOffice Calc: -2.5 becomes -3.")
            description: "=INT(Groceries / 7)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "IF: gives the first figure when the test is true, the second when it is not.")
            description: "=IF(Pets > 100; Pets; 0)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "AND: true when every test is true.")
            description: "=IF(AND(Pets > 50; Travel > 50); Pets + Travel; 0)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "OR: true when at least one test is true.")
            description: "=IF(OR(Pets > 100; Travel > 100); 1; 0)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "NOT: turns true into false and false into true.")
            description: "=IF(NOT(Pets > 100); Pets; 100)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "IFERROR: gives the fallback when the first part has no value, for example when it divides by zero.")
            description: "=IFERROR(Groceries / Travel; 0)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "PERCENT_OF: the part as a share of the whole, shown as a percent.")
            description: "=PERCENT_OF(Groceries; Salary)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info a function of formulas, its English name first", "CHANGE: how far the new figure is from the old one, as a percent of the old one.")
            description: "=CHANGE(Groceries; Groceries@\"last month\")"
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Custom charts")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A custom chart is drawn from a short description in the Vega-Lite language: which data to use, the kind of mark (bar, line, area, arc, point, text or rule) and which field goes where. In the properties of a custom chart, \"Insert an example\" puts one of the examples below into the field, where you can change it. Nothing is kept until you choose \"Use this description\".")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Data names and their fields")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Write the data name as \"data\": { \"name\": … }. Each name has the fields listed after it. Amounts are exact; a chart never adds up different currencies.")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            textItem.wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info what a data name of a chart description holds", "What you spent in the period, one row per category.")
            description: "spending_by_category: category, top_level, total, currency"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            textItem.wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info what a data name of a chart description holds", "Money in and money out of each month, and what is left.")
            description: "income_expense_by_month: month, income, expenses, net, currency"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            textItem.wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info what a data name of a chart description holds", "What each account held at the end of the period, and whether it is a debt or closed.")
            description: "account_balances: account, owed, closed, balance, currency"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            textItem.wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info what a data name of a chart description holds", "What you have, owe and are worth at the end of each month.")
            description: "net_worth_over_time: month, assets, liabilities, net_worth, currency"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            textItem.wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info what a data name of a chart description holds", "For the chosen accounts together, each month's start, money in, money out, net change and end.")
            description: "cash_flow_by_month: month, start, money_in, money_out, net_change, end, currency"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            textItem.wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info what a data name of a chart description holds", "Each month's total of the categories and groups you pick in the properties, one series each.")
            description: "category_over_time: month, series, total, currency"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            textItem.wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info what a data name of a chart description holds", "Each budget's period with what was planned, carried in, available, spent and left, and the share used.")
            description: "budget_utilization: category, every, first_day, last_day, planned, carried_in, available, spent, left, used, carries_over, currency"
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Examples of chart descriptions")
    }

    FormCard.FormCard {
        Repeater {
            model: page.examples.examples()

            delegate: FormCard.FormTextDelegate {
                required property var modelData

                text: page.exampleNames.name(modelData.key)
                textItem.textFormat: Text.PlainText
                textItem.wrapMode: Text.Wrap
                description: modelData.text
                descriptionItem.textFormat: Text.PlainText
                descriptionItem.font: Kirigami.Theme.fixedWidthFont
            }
        }
    }

}
