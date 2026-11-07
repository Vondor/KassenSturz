// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A period: the quick choices, the last few months and a salary month that
// move with today (US1.12, D-644, D-645), and a range of dates the user types
// (M6b.R6, D-332). The text is read and the words are made in Rust, in the
// regional date format; this file only shows them and passes the word on.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: choice

    // The quick choices, as {word, name}.
    property var options: []
    // The period now: one of their words or a range.
    property string word: ""

    // The user picked "A range of dates" and hasn't typed one yet.
    property bool typing: false
    property var problem: ({})
    readonly property var described: range.describe(word)
    readonly property bool isRange: typing || described.custom === true
    // The word is one of the quick choices (`last-12-months` is one, and a
    // rolling word too).
    readonly property bool quick: options.some(option => option.word === word)
    // A rolling period that isn't a quick choice: `months` or `salary`, else
    // empty.
    readonly property string rolling: typing || quick ? "" : (described.rolling ?? "")

    // The period was chosen: a word from the quick choices, or a range.
    signal chosen(string word)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function indexOfWord(): int {
        if (isRange) {
            return options.length + 2;
        }
        if (quick) {
            return options.findIndex(option => option.word === word);
        }
        if (rolling === "months") {
            return options.length;
        }
        if (rolling === "salary") {
            return options.length + 1;
        }
        return -1;
    }

    // The spin boxes and the check box lose their bindings once the user
    // changes them, so a new word sets them again.
    onDescribedChanged: {
        if (described.rolling === "months") {
            monthCount.value = described.count;
            completeMonths.checked = described.complete === true;
        } else if (described.rolling === "salary") {
            salaryDay.value = described.day;
            whichSalaryMonth.currentIndex = described.before === true ? 1 : 0;
        }
    }

    function apply(input: string) {
        if (input.trim() === "") {
            problem = ({});
            return;
        }
        const result = range.parse(input);
        if (result.code !== undefined) {
            problem = result;
            return;
        }
        problem = ({});
        typing = false;
        choice.chosen(result.word);
    }

    spacing: Kirigami.Units.smallSpacing

    PeriodRange {
        id: range
    }

    Controls.ComboBox {
        Layout.fillWidth: true
        Accessible.name: Kirigami.FormData.label
        model: choice.options.concat([
            {word: "", name: choice.i18nContext.i18nc("@item:inlistbox a period of a number of months the user chooses", "The last months…")},
            {word: "", name: choice.i18nContext.i18nc("@item:inlistbox a period from a day of the month the user chooses, such as payday, to the day before it in the next month", "A salary month…")},
            {word: "", name: choice.i18nContext.i18nc("@item:inlistbox a period the user types as two dates", "A range of dates…")}
        ])
        textRole: "name"
        currentIndex: choice.indexOfWord()
        onActivated: index => {
            ActionLog.taken("PeriodChoice_activated")
            choice.problem = ({});
            choice.typing = false;
            if (index < choice.options.length) {
                choice.chosen(choice.options[index].word);
            } else if (index === choice.options.length) {
                // Complete months by default: a month that isn't over yet
                // makes its figures look small (D-644).
                choice.chosen(range.months(3, true));
            } else if (index === choice.options.length + 1) {
                choice.chosen(range.salaryMonth(1, false));
            } else {
                choice.typing = true;
            }
        }
    }

    // The last months: how many, and whether this month counts.
    RowLayout {
        Layout.fillWidth: true
        visible: choice.rolling === "months"
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            text: choice.i18nContext.i18nc("@label:spinbox how many months a period counts back", "Months:")
            textFormat: Text.PlainText
        }

        Controls.SpinBox {
            id: monthCount

            from: 1
            to: 120
            editable: true
            value: choice.described.count ?? 3
            Accessible.name: choice.i18nContext.i18nc("@label:spinbox", "How many months")
            // A spin box is no action of its own for the debug log, like
            // the other spin boxes; the word it makes is what counts.
            onValueModified: choice.chosen(range.months(value, completeMonths.checked))
        }
    }

    Controls.CheckBox {
        id: completeMonths

        Layout.fillWidth: true
        visible: choice.rolling === "months"
        checked: choice.described.complete === true
        text: choice.i18nContext.i18nc("@option:check a period of months", "Only complete months (leave out this month)")
        onToggled: {
            ActionLog.taken("PeriodChoice_complete")
            choice.chosen(range.months(monthCount.value, checked));
        }
    }

    // A salary month: the day it starts on, and which one.
    RowLayout {
        Layout.fillWidth: true
        visible: choice.rolling === "salary"
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            text: choice.i18nContext.i18nc("@label:spinbox the day of the month a salary month starts on", "Starts on day:")
            textFormat: Text.PlainText
        }

        Controls.SpinBox {
            id: salaryDay

            from: 1
            to: 31
            editable: true
            value: choice.described.day ?? 1
            Accessible.name: choice.i18nContext.i18nc("@label:spinbox", "The day of the month the salary month starts on")
            onValueModified: choice.chosen(range.salaryMonth(value, whichSalaryMonth.currentIndex === 1))
        }
    }

    Controls.ComboBox {
        id: whichSalaryMonth

        Layout.fillWidth: true
        visible: choice.rolling === "salary"
        model: [
            choice.i18nContext.i18nc("@item:inlistbox which salary month", "The one running now"),
            choice.i18nContext.i18nc("@item:inlistbox which salary month", "The one before")
        ]
        currentIndex: choice.described.before === true ? 1 : 0
        Accessible.name: choice.i18nContext.i18nc("@label:listbox", "Which salary month")
        onActivated: index => {
            ActionLog.taken("PeriodChoice_whichSalaryMonth")
            choice.chosen(range.salaryMonth(salaryDay.value, index === 1));
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: choice.rolling === "salary"
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        opacity: 0.7
        text: choice.i18nContext.i18nc("@info", "From this day to the day before it in the next month, for example from your payday. In a month without this day, its last day counts.")
    }

    Controls.TextField {
        id: rangeField

        Layout.fillWidth: true
        visible: choice.isRange
        text: choice.described.text ?? ""
        placeholderText: range.example()
        Accessible.name: choice.i18nContext.i18nc("@label:textbox two dates, or one with < or >", "Dates of the period")
        onEditingFinished: choice.apply(text)
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: choice.isRange && choice.problem.code === undefined
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        opacity: 0.7
        text: choice.i18nContext.i18nc("@info %1 is two dates written as the user types dates", "Two dates, like %1. Write < or > before one date for everything before or after it; <= and >= include that day.", range.example())
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: choice.problem.code !== undefined
        type: Kirigami.MessageType.Warning
        text: choice.i18nContext.i18nc("@info", "That isn't a period KassenSturz can use. Write two dates, for example the first and the last day, with the first one earlier than the last and not more than 100 years apart.")
    }
}
