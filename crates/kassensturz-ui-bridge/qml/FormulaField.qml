// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The field for a formula of a key figure or a progress bar (M6b.7, D-322):
// the formula in the user's number format, a list of functions with a plain
// line each, the names of the book's categories and groups completed while
// the user types, and the result next to the formula with what it counted, so
// nobody has to trust a formula blindly. A mistake is told in words with the
// place marked. The work is Rust's (`BookFormula`); the words are here
// (D-034). Names are the user's text, shown as plain text.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: field

    // The formula as it was kept, in the user's number format.
    property string text
    // The period the result is worked out for: `this-month`, ..., or a
    // month as `YYYY-MM`.
    property string period: "last-12-months"
    // Whether the figure fills a progress bar, whether this field is the
    // target the bar's formula is measured against, and whether the bar has
    // one.
    property bool forBar: false
    property bool isTarget: false
    property bool hasTarget: false

    // The formula was typed and the field was left (or Enter was pressed).
    signal committed(string text)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property var shown: book.view ?? ({})
    readonly property var result: shown.result
    readonly property var mistake: shown.mistake
    readonly property var writing: book.functions()
    // What can be completed at the cursor, from `BookFormula.complete`.
    property var completion: ({start: 0, end: 0, names: []})
    // The suggestion the arrow keys are on, or -1.
    property int chosen: -1

    spacing: Kirigami.Units.smallSpacing

    function help(name: string): string {
        switch (name) {
        case "SUM":
            return i18nContext.i18nc("@info a function of formulas", "Adds the figures up.");
        case "AVERAGE":
            return i18nContext.i18nc("@info a function of formulas", "The sum divided by how many figures there are.");
        case "MIN":
            return i18nContext.i18nc("@info a function of formulas", "The smallest of the figures.");
        case "MAX":
            return i18nContext.i18nc("@info a function of formulas", "The largest of the figures.");
        case "COUNT":
            return i18nContext.i18nc("@info a function of formulas", "How many transactions the categories or groups have in the period.");
        case "ABS":
            return i18nContext.i18nc("@info a function of formulas", "The figure without its minus sign.");
        case "SIGN":
            return i18nContext.i18nc("@info a function of formulas", "-1 for a figure below zero, 0 for zero and 1 for a figure above zero.");
        case "ROUND":
            return i18nContext.i18nc("@info a function of formulas", "Rounds to that many decimal places, 5 and up away from zero.");
        case "ROUNDUP":
            return i18nContext.i18nc("@info a function of formulas", "Rounds away from zero to that many decimal places.");
        case "ROUNDDOWN":
            return i18nContext.i18nc("@info a function of formulas", "Rounds toward zero to that many decimal places.");
        case "INT":
            return i18nContext.i18nc("@info a function of formulas", "Rounds down to a whole number.");
        case "IF":
            return i18nContext.i18nc("@info a function of formulas", "Gives the first figure when the test is true, the second when it is not. A test compares, like Pets > 100.");
        case "AND":
            return i18nContext.i18nc("@info a function of formulas", "True when every test is true.");
        case "OR":
            return i18nContext.i18nc("@info a function of formulas", "True when at least one test is true.");
        case "NOT":
            return i18nContext.i18nc("@info a function of formulas", "Turns true into false and false into true.");
        case "IFERROR":
            return i18nContext.i18nc("@info a function of formulas", "Gives the fallback when the first part has no value, for example when it divides by zero.");
        case "PERCENT_OF":
            return i18nContext.i18nc("@info a function of formulas", "The part as a share of the whole, shown as a percent.");
        case "CHANGE":
            return i18nContext.i18nc("@info a function of formulas", "How far the new figure is from the old one, as a percent of the old one.");
        }
        return "";
    }

    // `SUM(x; ...)`, written with the separator of the user's number format.
    function signature(entry: var): string {
        return entry.name + "(" + entry.arguments.join(writing.separator + " ") + ")";
    }

    // The words of "counts Pets (with Vet), Travel".
    function counts(lines: var): string {
        const parts = lines.map(line => {
            let name = line.name;
            if (line.inside.length > 0) {
                const more = line.insideMore > 0
                    ? i18nContext.i18ncp("@info the end of a list of names", "and %1 more", "and %1 more", line.insideMore) : "";
                name = i18nContext.i18nc("@info %1 is a category, %2 the names inside it", "%1 (with %2)", name, [line.inside.join(", "), more].filter(part => part !== "").join(" "));
            }
            if (!line.ownPeriod) {
                name = line.month !== ""
                    ? i18nContext.i18nc("@info %1 is a category, %2 a month", "%1 in %2", name, line.month)
                    : i18nContext.i18nc("@info %1 is a category, %2 and %3 are dates", "%1 from %2 to %3", name, line.firstDay, line.lastDay);
            }
            return name;
        });
        return parts.join(", ");
    }

    function check() {
        book.show(period, input.text, true);
    }

    function look() {
        completion = book.complete(input.text, input.cursorPosition);
        chosen = -1;
    }

    // Puts a suggested name where the word was typed.
    function insertName(name: var) {
        const at = completion.start;
        input.text = input.text.slice(0, at) + name.insert + input.text.slice(completion.end);
        input.cursorPosition = at + name.insert.length;
        completion = {start: 0, end: 0, names: []};
        chosen = -1;
        input.forceActiveFocus();
        checkTimer.restart();
    }

    // Puts a function's name and brackets at the cursor, inside them.
    function insertFunction(name: string) {
        const at = input.cursorPosition;
        input.text = input.text.slice(0, at) + name + "()" + input.text.slice(input.selectionEnd);
        input.cursorPosition = at + name.length + 1;
        input.forceActiveFocus();
        checkTimer.restart();
    }

    onPeriodChanged: check()
    Component.onCompleted: check()

    BookFormula {
        id: book
    }

    Timer {
        id: checkTimer

        interval: 300
        onTriggered: {
            ActionLog.taken("FormulaField_check")
            field.check()
        }
    }

    Controls.TextField {
        id: input

        Layout.fillWidth: true
        text: field.text
        maximumLength: 1000
        placeholderText: field.isTarget
            ? field.i18nContext.i18nc("@info:placeholder an example of a formula for a target", "For example =400")
            : field.i18nContext.i18nc("@info:placeholder an example of a formula", "For example =Pets - Travel")
        Accessible.name: field.i18nContext.i18nc("@label:textbox a formula that works out a figure", "Formula")
        Accessible.description: field.i18nContext.i18nc("@info", "Use the names of your categories and groups. Add a time range to a name with @, for example Pets@\"last month\" or Pets@2026-03.")
        // The text decides what is offered; a typing pause decides when the
        // result is worked out again.
        onTextEdited: {
            checkTimer.restart();
            field.look();
        }
        onCursorPositionChanged: {
            if (activeFocus) {
                field.look();
            }
        }
        onEditingFinished: field.committed(text)
        Keys.onDownPressed: event => {
            if (field.completion.names.length > 0) {
                field.chosen = (field.chosen + 1) % field.completion.names.length;
            } else {
                event.accepted = false;
            }
        }
        Keys.onUpPressed: event => {
            if (field.completion.names.length > 0) {
                field.chosen = field.chosen <= 0 ? field.completion.names.length - 1 : field.chosen - 1;
            } else {
                event.accepted = false;
            }
        }
        // Tab or Enter takes the name the arrow keys are on.
        Keys.onTabPressed: event => {
            if (field.chosen >= 0) {
                field.insertName(field.completion.names[field.chosen]);
            } else {
                event.accepted = false;
            }
        }
        Keys.onReturnPressed: event => {
            if (field.chosen >= 0) {
                field.insertName(field.completion.names[field.chosen]);
            } else {
                event.accepted = false;
            }
        }
        Keys.onEscapePressed: event => {
            if (field.completion.names.length > 0) {
                field.completion = {start: 0, end: 0, names: []};
                field.chosen = -1;
            } else {
                event.accepted = false;
            }
        }
    }

    // The names that fit what is being typed. Clicking one keeps the focus in
    // the field; the keyboard uses the arrow keys and Tab.
    ColumnLayout {
        Layout.fillWidth: true
        visible: field.completion.names.length > 0
        spacing: 0

        Controls.Label {
            Layout.fillWidth: true
            text: field.i18nContext.i18nc("@info above a list of names that fit what is typed", "Names that fit. Choose one with the arrow keys and Tab, or click it:")
            font: Kirigami.Theme.smallFont
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }

        Repeater {
            model: field.completion.names

            delegate: Controls.ItemDelegate {
                required property var modelData
                required property int index

                Layout.fillWidth: true
                focusPolicy: Qt.NoFocus
                highlighted: index === field.chosen
                text: modelData.group
                    ? field.i18nContext.i18nc("@item a group of categories the user made, %1 is its name", "%1 (group)", modelData.label)
                    : modelData.label
                onClicked: {
                    ActionLog.taken("FormulaField_insertName")
                    field.insertName(modelData)
                }
            }
        }
    }

    Controls.ComboBox {
        id: functionBox

        Layout.fillWidth: true
        visible: !field.isTarget
        model: field.writing.functions
        displayText: field.i18nContext.i18nc("@label:listbox opens a list of the functions of formulas", "Insert a function…")
        currentIndex: -1
        Accessible.name: displayText
        onActivated: index => {
            ActionLog.taken("FormulaField_activated")
            field.insertFunction(field.writing.functions[index].name);
            currentIndex = -1;
        }

        delegate: Controls.ItemDelegate {
            required property var modelData
            required property int index

            width: ListView.view ? ListView.view.width : implicitWidth
            highlighted: functionBox.highlightedIndex === index
            contentItem: ColumnLayout {
                spacing: 0

                Controls.Label {
                    text: field.signature(modelData)
                    font.bold: true
                    textFormat: Text.PlainText
                }

                Controls.Label {
                    Layout.fillWidth: true
                    text: field.help(modelData.name)
                    wrapMode: Text.Wrap
                    font: Kirigami.Theme.smallFont
                    textFormat: Text.PlainText
                }
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: field.forBar && (field.isTarget || field.text.trim() !== "")
        text: field.isTarget
            ? field.i18nContext.i18nc("@info", "What a full bar stands for, the same kind of figure as the formula: an amount in the same currency, a percent or a plain number. Leave it empty when the formula gives a percent.")
            : field.i18nContext.i18nc("@info", "The bar shows your formula instead of the share you kept. Set a target to measure it against, or clear the formula to go back.")
        font: Kirigami.Theme.smallFont
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    // The result, and what it counted.
    ColumnLayout {
        Layout.fillWidth: true
        visible: field.result !== undefined
        spacing: 0

        Controls.Label {
            Layout.fillWidth: true
            text: field.result !== undefined
                ? field.i18nContext.i18nc("@info %1 is the figure a formula gives", "= %1", field.result.value) : ""
            font.bold: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: field.result !== undefined && field.result.counted.length > 0
            text: field.result !== undefined
                ? field.i18nContext.i18nc("@info %1 is a list of categories and groups", "Counts %1", field.counts(field.result.counted)) : ""
            font: Kirigami.Theme.smallFont
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: field.forBar && !field.isTarget && !field.hasTarget && field.result !== undefined && field.result.kind !== "percent"
            text: field.i18nContext.i18nc("@info", "Without a target, a progress bar fills only for a percent. Set a target, or use PERCENT_OF(part; whole).")
            font: Kirigami.Theme.smallFont
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }
    }

    // A mistake, with the place marked in the formula.
    ColumnLayout {
        Layout.fillWidth: true
        // An empty field is not a mistake: the example in it is the hint.
        visible: field.mistake !== undefined && input.text.trim() !== ""
        spacing: Kirigami.Units.smallSpacing

        FormError {
            Layout.fillWidth: true
            result: field.mistake !== undefined ? field.mistake.error : ({})
        }

        Row {
            visible: field.mistake !== undefined && (field.mistake.before !== "" || field.mistake.after !== "")
            // The message says the character; the mark is for the eyes.
            Accessible.ignored: true

            Controls.Label {
                id: before

                text: field.mistake !== undefined ? field.mistake.before : ""
                font.family: "monospace"
                textFormat: Text.PlainText
            }

            Rectangle {
                width: 2
                height: before.implicitHeight
                color: Kirigami.Theme.negativeTextColor
            }

            Controls.Label {
                text: field.mistake !== undefined ? field.mistake.after : ""
                font.family: "monospace"
                textFormat: Text.PlainText
            }
        }
    }

    FormError {
        Layout.fillWidth: true
        result: input.text.trim() !== "" && field.shown.error !== undefined ? field.shown.error : ({})
    }
}
