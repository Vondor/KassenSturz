// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A period: the quick choices, and a range of dates the user types (M6b.R6,
// D-332). The text is read in Rust, in the regional date format; this file
// only shows it and passes the word on.

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

    // The period was chosen: a word from the quick choices, or a range.
    signal chosen(string word)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function indexOfWord(): int {
        if (isRange) {
            return options.length;
        }
        for (let index = 0; index < options.length; ++index) {
            if (options[index].word === word) {
                return index;
            }
        }
        return -1;
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
        model: choice.options.concat([{word: "", name: choice.i18nContext.i18nc("@item:inlistbox a period the user types as two dates", "A range of dates…")}])
        textRole: "name"
        currentIndex: choice.indexOfWord()
        onActivated: index => {
            choice.problem = ({});
            if (index < choice.options.length) {
                choice.typing = false;
                choice.chosen(choice.options[index].word);
            } else {
                choice.typing = true;
            }
        }
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
