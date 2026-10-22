// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The column screen for a CSV file from online banking (M3.10, D-095,
// D-111): a CSV file says neither its encoding, separator nor which column
// holds what, and every bank writes it differently, so the user says all of
// it once per bank. The settings are kept under the bank's name in the
// app's settings and offered again for the next file. The first rows show
// as the settings read them, so a wrong separator or encoding is visible
// before anything is imported. Rust reads the file; nothing here parses an
// amount or a date (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property StatementImport statementImport
    // The first rows of the file as the current choices read them.
    property var rows: []
    readonly property bool hasHeader: headerBox.checked
    readonly property var columns: columnChoices()

    // The file was read with these settings; the preview comes next.
    signal read()
    // The user gave up on the file.
    signal cancelled()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "How to read this file")
    header: FormError {
        id: error
    }

    actions: [
        Kirigami.Action {
            icon.name: "document-import"
            text: page.i18nContext.i18nc("@action:button", "Read the file")
            onTriggered: page.readFile()
        },
        Kirigami.Action {
            icon.name: "dialog-cancel"
            text: page.i18nContext.i18nc("@action:button", "Don't import")
            onTriggered: {
                page.statementImport.cancel();
                page.cancelled();
            }
        }
    ]

    Component.onCompleted: {
        const banks = statementImport.csvBanks();
        bankBox.model = banks;
        if (banks.length > 0) {
            bankBox.currentIndex = 0;
            bankBox.editText = banks[0];
            apply(statementImport.csvForm(banks[0]));
        } else {
            apply(statementImport.csvForm(""));
        }
    }

    // Fills the controls from a form the Rust side gives.
    function apply(form: var) {
        encodingBox.currentIndex = Math.max(0, encodingBox.indexOfValue(form.encoding));
        delimiterBox.currentIndex = Math.max(0, delimiterBox.indexOfValue(form.delimiter));
        skipLinesBox.value = parseInt(form.skipLines) || 0;
        skipLastLinesBox.value = parseInt(form.skipLastLines) || 0;
        headerBox.checked = form.header;
        decimalMarkBox.currentIndex = Math.max(0, decimalMarkBox.indexOfValue(form.decimalMark));
        dateOrderBox.currentIndex = Math.max(0, dateOrderBox.indexOfValue(form.dateOrder));
        signedRadio.checked = form.amountMode === "signed";
        debitCreditRadio.checked = form.amountMode === "debit-credit";
        directionRadio.checked = form.amountMode === "direction";
        bookingDateBox.value = form.bookingDate;
        valueDateBox.value = form.valueDate;
        amountBox.value = form.amount;
        debitBox.value = form.debit;
        creditBox.value = form.credit;
        directionBox.value = form.direction;
        moneyOutField.text = form.moneyOut;
        moneyInField.text = form.moneyIn;
        otherPartyNameBox.value = form.otherPartyName;
        otherPartyIbanBox.value = form.otherPartyIban;
        purposeBox.value = form.purpose;
        purpose2Box.value = form.purpose2;
        statusBox.value = form.status;
        bookedField.text = form.booked;
        currencyBox.value = form.currency;
        accountBox.value = form.account;
        refreshRows();
    }

    // The form as the Rust side reads it: every field as text.
    function form(): var {
        return {
            encoding: encodingBox.currentValue,
            delimiter: delimiterBox.currentValue,
            skipLines: String(skipLinesBox.value),
            skipLastLines: String(skipLastLinesBox.value),
            header: headerBox.checked,
            decimalMark: decimalMarkBox.currentValue,
            dateOrder: dateOrderBox.currentValue,
            bookingDate: bookingDateBox.value,
            valueDate: valueDateBox.value,
            amountMode: signedRadio.checked ? "signed" : (debitCreditRadio.checked ? "debit-credit" : "direction"),
            amount: amountBox.value,
            debit: debitBox.value,
            credit: creditBox.value,
            direction: directionBox.value,
            moneyOut: moneyOutField.text,
            moneyIn: moneyInField.text,
            otherPartyName: otherPartyNameBox.value,
            otherPartyIban: otherPartyIbanBox.value,
            purpose: purposeBox.value,
            purpose2: purpose2Box.value,
            status: statusBox.value,
            booked: bookedField.text,
            currency: currencyBox.value,
            account: accountBox.value,
        };
    }

    function refreshRows() {
        const result = statementImport.csvRows(form());
        if (result.code !== undefined) {
            rowsError.result = result;
            rows = [];
        } else {
            rowsError.result = ({});
            rows = result;
        }
    }

    // The columns to pick from: the header row's names, or numbers.
    function columnChoices(): var {
        if (rows.length === 0) {
            return [];
        }
        if (hasHeader) {
            return rows[0].map((name, index) => ({
                text: name !== "" ? name : i18nContext.i18nc("@item:inlistbox a column without a name, %1 is its number", "Column %1 (no name)", index + 1),
                value: name !== "" ? name : String(index + 1),
            }));
        }
        let widest = 0;
        for (const row of rows) {
            widest = Math.max(widest, row.length);
        }
        const choices = [];
        for (let index = 0; index < widest; index++) {
            choices.push({ text: i18nContext.i18nc("@item:inlistbox %1 is the number of a column", "Column %1", index + 1), value: String(index + 1) });
        }
        return choices;
    }

    function readFile() {
        error.result = statementImport.readCsv(form(), bankBox.editText.trim());
        if (error.result.code === undefined) {
            read();
        }
    }

    Kirigami.FormLayout {
        Controls.Label {
            Kirigami.FormData.isSection: true
            Layout.fillWidth: true
            text: page.i18nContext.i18nc("@info %1 is a file name", "%1 is a CSV file. Such a file doesn't say how it is written, and every bank does it differently, so say once how to read your bank's files. KassenSturz keeps the choices under the bank's name for the next file.", page.statementImport.fileName)
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        Controls.ComboBox {
            id: bankBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Bank:")
            editable: true
            onActivated: page.apply(page.statementImport.csvForm(currentText))
            Controls.ToolTip.text: page.i18nContext.i18nc("@info:tooltip", "Type your bank's name to keep these choices for its next file, or pick a bank you saved before.")
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
        }

        Controls.Label {
            Kirigami.FormData.isSection: true
            text: page.i18nContext.i18nc("@title:group", "How the file is written")
        }

        Controls.ComboBox {
            id: encodingBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Text encoding:")
            textRole: "text"
            valueRole: "value"
            model: [
                { text: page.i18nContext.i18nc("@item:inlistbox", "Tell from the file"), value: "" },
                { text: "UTF-8", value: "utf-8" },
                { text: page.i18nContext.i18nc("@item:inlistbox", "Windows-1252 (also ISO-8859-1)"), value: "windows-1252" },
                { text: "ISO-8859-15", value: "iso-8859-15" },
            ]
            onActivated: page.refreshRows()
        }

        Controls.ComboBox {
            id: delimiterBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Between the columns:")
            textRole: "text"
            valueRole: "value"
            model: [
                { text: page.i18nContext.i18nc("@item:inlistbox", "Semicolon (;)"), value: ";" },
                { text: page.i18nContext.i18nc("@item:inlistbox", "Comma (,)"), value: "," },
                { text: page.i18nContext.i18nc("@item:inlistbox", "Tab"), value: "tab" },
                { text: page.i18nContext.i18nc("@item:inlistbox", "Vertical bar (|)"), value: "|" },
            ]
            onActivated: page.refreshRows()
        }

        Controls.SpinBox {
            id: skipLinesBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:spinbox", "Lines to skip at the start:")
            from: 0
            to: 100
            onValueModified: page.refreshRows()
        }

        Controls.SpinBox {
            id: skipLastLinesBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:spinbox", "Lines to skip at the end:")
            from: 0
            to: 100
        }

        Controls.CheckBox {
            id: headerBox

            text: page.i18nContext.i18nc("@option:check", "The first line names the columns")
            checked: true
        }

        Controls.ComboBox {
            id: decimalMarkBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Decimal mark in amounts:")
            textRole: "text"
            valueRole: "value"
            model: [
                { text: page.i18nContext.i18nc("@item:inlistbox", "Comma (12,50)"), value: "comma" },
                { text: page.i18nContext.i18nc("@item:inlistbox", "Point (12.50)"), value: "point" },
            ]
        }

        Controls.ComboBox {
            id: dateOrderBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Dates are written as:")
            textRole: "text"
            valueRole: "value"
            model: [
                { text: page.i18nContext.i18nc("@item:inlistbox", "Day, month, year (31.12.2026)"), value: "day-month-year" },
                { text: page.i18nContext.i18nc("@item:inlistbox", "Year, month, day (2026-12-31)"), value: "year-month-day" },
                { text: page.i18nContext.i18nc("@item:inlistbox", "Month, day, year (12/31/2026)"), value: "month-day-year" },
            ]
        }

        Controls.Label {
            Kirigami.FormData.isSection: true
            text: page.i18nContext.i18nc("@title:group", "The first lines, read this way")
        }

        FormError {
            id: rowsError

            Layout.fillWidth: true
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: page.rows.length === 0 && rowsError.result.code === undefined
            text: page.i18nContext.i18nc("@info", "No lines to show. Check the lines to skip.")
            wrapMode: Text.Wrap
        }

        // One item for the form layout, which doesn't take a Repeater's
        // rows as they come and go.
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0

            Repeater {
                model: page.rows

                delegate: Controls.Label {
                    required property var modelData
                    required property int index

                    Layout.fillWidth: true
                    text: modelData.join("  │  ")
                    textFormat: Text.PlainText
                    font.family: "monospace"
                    font.bold: index === 0 && page.hasHeader
                    elide: Text.ElideRight
                    maximumLineCount: 1
                }
            }
        }

        Controls.Label {
            Kirigami.FormData.isSection: true
            text: page.i18nContext.i18nc("@title:group", "Which column holds what")
        }

        CsvColumnBox {
            id: bookingDateBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Booking date:")
            columns: page.columns
            optional: false
        }

        Controls.RadioButton {
            id: signedRadio

            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "The amount:")
            text: page.i18nContext.i18nc("@option:radio", "One column, money out with a minus sign")
            checked: true
        }

        CsvColumnBox {
            id: amountBox

            visible: signedRadio.checked || directionRadio.checked
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Amount:")
            columns: page.columns
            optional: false
        }

        Controls.RadioButton {
            id: debitCreditRadio

            text: page.i18nContext.i18nc("@option:radio", "Two columns, one for money out and one for money in")
        }

        CsvColumnBox {
            id: debitBox

            visible: debitCreditRadio.checked
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Money out:")
            columns: page.columns
            optional: false
        }

        CsvColumnBox {
            id: creditBox

            visible: debitCreditRadio.checked
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Money in:")
            columns: page.columns
            optional: false
        }

        Controls.RadioButton {
            id: directionRadio

            text: page.i18nContext.i18nc("@option:radio", "The amount without a sign, and a column saying out or in")
        }

        CsvColumnBox {
            id: directionBox

            visible: directionRadio.checked
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Out or in:")
            columns: page.columns
            optional: false
        }

        Controls.TextField {
            id: moneyOutField

            visible: directionRadio.checked
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox the value in a column that means money out", "Means money out:")
            placeholderText: "S"
        }

        Controls.TextField {
            id: moneyInField

            visible: directionRadio.checked
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox the value in a column that means money in", "Means money in:")
            placeholderText: "H"
        }

        CsvColumnBox {
            id: otherPartyNameBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Name of the other party:")
            columns: page.columns
        }

        CsvColumnBox {
            id: purposeBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Purpose:")
            columns: page.columns
        }

        CsvColumnBox {
            id: purpose2Box

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Second purpose column:")
            columns: page.columns
        }

        CsvColumnBox {
            id: otherPartyIbanBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "IBAN of the other party:")
            columns: page.columns
        }

        CsvColumnBox {
            id: valueDateBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Value date:")
            columns: page.columns
        }

        CsvColumnBox {
            id: statusBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Booked or not yet:")
            columns: page.columns
        }

        Controls.TextField {
            id: bookedField

            visible: statusBox.value !== ""
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Values that mean booked:")
            placeholderText: page.i18nContext.i18nc("@info:placeholder", "e.g. Gebucht, booked")
        }

        CsvColumnBox {
            id: currencyBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Currency:")
            columns: page.columns
        }

        CsvColumnBox {
            id: accountBox

            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Account, if the file has several:")
            columns: page.columns
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button", "Read the file")
            icon.name: "document-import"
            onClicked: page.readFile()
        }
    }
}
