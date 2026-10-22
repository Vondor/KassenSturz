// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A file with statements for several accounts (M3.10, D-091): the user
// says which of them is the account's. Every text comes from Rust
// (D-065); the sentences are written here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property StatementImport statementImport
    readonly property var statements: statementImport.view ?? []

    // The user picked a statement; the preview comes next.
    signal chosen()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Which statement is for this account?")
    header: FormError {
        id: error
    }

    function choose(index: int) {
        error.result = statementImport.chooseStatement(index);
        if (error.result.code === undefined) {
            chosen();
        }
    }

    function describe(statement: var): string {
        const count = i18nContext.i18ncp("@info", "%1 transaction", "%1 transactions", statement.entries);
        let text = statement.firstDate === "" ? count : i18nContext.i18nc("@info %1 is a number of entries, %2 and %3 dates", "%1, %2 to %3", count, statement.firstDate, statement.lastDate);
        if (statement.closing !== "") {
            text = i18nContext.i18nc("@info %1 describes the entries, %2 is an amount, %3 a date", "%1 · ends with %2 on %3", text, statement.closing, statement.closingDate);
        }
        return text;
    }

    ColumnLayout {
        spacing: 0

        // A file name could contain anything, so this isn't FormSectionText,
        // which would read it as markup.
        Controls.Label {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            text: page.i18nContext.i18nc("@info %1 is a file name, %2 the number of statements in it", "%1 holds %2 statements for different accounts. Choose the one that belongs to the account you are importing into; the others stay in the file.", page.statementImport.fileName, String(page.statements.length))
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        FormCard.FormCard {
            Repeater {
                model: page.statements

                delegate: ColumnLayout {
                    id: row

                    required property int index
                    required property var modelData

                    Layout.fillWidth: true
                    spacing: 0

                    FormCard.FormDelegateSeparator {
                        visible: row.index > 0
                    }

                    // A text delegate, not a button one: only this one lets
                    // the account number from the file be shown as plain text.
                    FormCard.FormTextDelegate {
                        icon.name: "go-next"
                        text: row.modelData.account === ""
                            ? page.i18nContext.i18nc("@item a statement whose file names no account", "Statement %1, without an account number", row.index + 1)
                            : row.modelData.account
                        description: page.describe(row.modelData)
                        // An account number from a file could contain anything.
                        textItem.textFormat: Text.PlainText
                        descriptionItem.textFormat: Text.PlainText
                        onClicked: page.choose(row.modelData.index)
                    }
                }
            }
        }
    }
}
