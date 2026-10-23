// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's settings: which book is open, opening another one or starting a
// new one (D-033, for people who want more than one book), how amounts look
// (D-002, D-114), the language (D-127), saving a copy of the data and starting a book from a file
// (M3.12), the bank logins on this computer and their PINs (M4.6, D-057),
// how long the history of changes is kept (D-066, M3.8), and where the
// backups go.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookSession session
    required property NumberFormatSettings numberFormat
    required property LanguageSettings language
    required property ChangeHistory history
    required property BankConnection bank

    // Whether a number format choice couldn't be saved for the next start.
    property bool formatNotSaved: false
    // Whether the language choice couldn't be saved.
    property bool languageNotSaved: false

    signal openAnotherClicked()
    signal newBookClicked()
    signal saveCopyClicked()
    signal bookFromFileClicked()
    signal showHistoryClicked()
    signal connectBankClicked()

    // What bank.status() and bank.logins() returned, and why the last
    // change to a login failed.
    property var bankStatus: ({})
    property var bankLogins: []
    property var bankResult: ({})
    // The login the removal dialog asks about.
    property var pendingLogin: ({})

    // Reads the bank logins again, e.g. after one was set up or removed.
    function refreshBanks() {
        bankStatus = bank.status();
        const result = bankStatus.ready ? bank.logins() : ({ logins: [] });
        bankLogins = result.logins ?? [];
        if (result.code !== undefined) {
            bankResult = result;
        }
    }

    Component.onCompleted: refreshBanks()

    // A login set up on its page is listed here at once.
    Connections {
        target: page.bank

        function onSetupFinished() {
            page.refreshBanks();
        }
    }

    // The history setting the user chose and the dialog asks about, with how
    // many changes it would delete, and why the last change of the history
    // settings failed.
    property string pendingRetention: ""
    property int pendingCount: 0
    property var historyResult: ({})

    // How long the history can be kept, as the keys history_model.rs knows:
    // a number of days, 0 for only while the book is open, or "forever". A
    // number the screen doesn't offer, e.g. one set with the command-line
    // tool, is added.
    readonly property var keepChoices: {
        const choices = [
            { value: "0", text: i18nContext.i18nc("@item:inlistbox keep the history", "Only while the book is open") },
            { value: "7", text: i18nContext.i18nc("@item:inlistbox keep the history", "1 week") },
            { value: "30", text: i18nContext.i18nc("@item:inlistbox keep the history", "30 days") },
            { value: "90", text: i18nContext.i18nc("@item:inlistbox keep the history", "3 months") },
            { value: "365", text: i18nContext.i18nc("@item:inlistbox keep the history", "1 year") },
            { value: "forever", text: i18nContext.i18nc("@item:inlistbox keep the history", "As long as the book exists") },
        ];
        if (choices.some(choice => choice.value === history.retention)) {
            return choices;
        }
        return choices.concat([{
            value: history.retention,
            text: i18nContext.i18ncp("@item:inlistbox keep the history for this many days", "%1 day", "%1 days", parseInt(history.retention)),
        }]);
    }

    // Keeps the history for `key`, or asks first if that deletes changes.
    function chooseRetention(key: string) {
        const count = key === "0" ? history.total : history.expiring(key);
        if (count > 0) {
            pendingRetention = key;
            pendingCount = count;
            shorterDialog.open();
        } else {
            saveRetention(key);
        }
    }

    function saveRetention(key: string) {
        historyResult = history.setRetention(key);
        keepFor.showChoice();
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Settings")
    // Why the last change to the history settings failed, if it did.
    header: FormError {
        result: page.historyResult
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Your book")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Book")
            description: page.session.fileName
            // A file's name could contain anything.
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label the folder a file is in", "Folder")
            description: page.session.folder
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-open"
            text: page.i18nContext.i18nc("@action:button", "Open another book…")
            description: page.i18nContext.i18nc("@info", "KassenSturz opens the book you used last. Most households need only one.")
            onClicked: page.openAnotherClicked()
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-new"
            text: page.i18nContext.i18nc("@action:button", "Start a new book…")
            description: page.i18nContext.i18nc("@info", "An empty book in a folder you choose, for example to try something out.")
            onClicked: page.newBookClicked()
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Your data")
    }

    FormCard.FormCard {
        FormCard.FormButtonDelegate {
            icon.name: "document-save-as"
            text: page.i18nContext.i18nc("@action:button", "Save a copy of my data…")
            description: page.i18nContext.i18nc("@info", "For a spreadsheet, or the whole book as a backup.")
            onClicked: page.saveCopyClicked()
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-import"
            text: page.i18nContext.i18nc("@action:button", "Start a book from a file…")
            description: page.i18nContext.i18nc("@info", "From a KMyMoney book, or a whole book you saved with KassenSturz.")
            onClicked: page.bookFromFileClicked()
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Bank connections")
    }

    FormCard.FormCard {
        Repeater {
            model: page.bankLogins

            delegate: ColumnLayout {
                id: login

                required property var modelData
                required property int index

                Layout.fillWidth: true
                spacing: 0

                FormCard.FormDelegateSeparator {
                    visible: login.index > 0
                }

                FormCard.FormTextDelegate {
                    descriptionItem.textFormat: Text.PlainText
                    text: login.modelData.bank
                    // From the bank lists: plain text (D-143).
                    textItem.textFormat: Text.PlainText
                    description: login.modelData.kind === "chip-card"
                        ? page.i18nContext.i18nc("@info", "Login with a chip card")
                        : page.i18nContext.i18nc("@info", "Login with PIN and TAN")
                }

                FormCard.FormButtonDelegate {
                    icon.name: "edit-clear"
                    text: page.i18nContext.i18nc("@action:button", "Forget the PIN")
                    description: page.i18nContext.i18nc("@info", "Removes the PIN KassenSturz remembers or saved for this login. You type it at the next connection.")
                    onClicked: page.bankResult = page.bank.forgetPin(login.modelData.id)
                }

                FormCard.FormButtonDelegate {
                    icon.name: "edit-delete"
                    text: page.i18nContext.i18nc("@action:button", "Remove bank login and saved PIN…")
                    onClicked: {
                        page.pendingLogin = login.modelData;
                        removeLoginDialog.open();
                    }
                }
            }
        }

        FormCard.FormDelegateSeparator {
            visible: page.bankLogins.length > 0
        }

        FormCard.FormButtonDelegate {
            descriptionItem.textFormat: Text.PlainText
            icon.name: "network-connect"
            text: page.i18nContext.i18nc("@action:button", "Connect a bank…")
            description: page.bankStatus.ready
                ? page.i18nContext.i18nc("@info", "Fetch your transactions from your bank instead of typing them. KassenSturz only reads; it never sends money.")
                : ""
            enabled: page.bankStatus.ready ?? false
            onClicked: page.connectBankClicked()
        }
    }

    // Why bank connections can't be used here, or why the last change to a
    // login failed.
    FormError {
        Layout.fillWidth: true
        Layout.maximumWidth: Kirigami.Units.gridUnit * 30
        Layout.alignment: Qt.AlignHCenter
        result: page.bankResult.code !== undefined ? page.bankResult : page.bankStatus.ready === false ? page.bankStatus : ({})
    }

    Kirigami.PromptDialog {
        id: removeLoginDialog

        title: page.i18nContext.i18nc("@title:window", "Remove this bank login?")
        subtitle: page.i18nContext.i18nc("@info", "KassenSturz forgets the login and its saved PIN on this computer. Your accounts and their transactions stay as they are, and so does which bank account fills which of them: set the login up again to fetch once more.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Remove the login")
                icon.name: "edit-delete"
                onTriggered: {
                    page.bankResult = page.bank.removeLogin(page.pendingLogin.id);
                    removeLoginDialog.close();
                    page.refreshBanks();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep it")
                icon.name: "dialog-cancel"
                onTriggered: removeLoginDialog.close()
            }
        ]
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Language")
    }

    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            id: languageBox

            // Shows the saved choice. Set rather than bound: the delegate
            // sets its index itself on phones, which would end a binding.
            function showChoice() {
                currentIndex = indexOfValue(page.language.choice);
            }

            text: page.i18nContext.i18nc("@label:listbox", "Language")
            textRole: "text"
            valueRole: "value"
            // Each language is named in itself, so it is found whatever
            // language the app is in now.
            model: [
                { value: "system", text: page.i18nContext.i18nc("@item:inlistbox", "As in your system") },
                { value: "en", text: "English" },
                { value: "de", text: "Deutsch" },
            ]
            Component.onCompleted: showChoice()
            onActivated: page.languageNotSaved = !page.language.choose(currentValue)
        }
    }

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        text: page.languageNotSaved
            ? page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then choose again.")
            : page.language.restartNeeded
                ? page.i18nContext.i18nc("@info", "The new language applies the next time you start KassenSturz.")
                : page.i18nContext.i18nc("@info", "Dates and amounts keep following your regional settings; change how amounts look below.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "How amounts look")
    }

    // One number format setting: its choices as `{ text, value }`, `value`
    // being the word the settings file keeps for it (D-114). An inline
    // component doesn't see the page's ids, so it gets the settings object
    // and reports a choice it couldn't save.
    component FormatChoice: FormCard.FormComboBoxDelegate {
        id: choice

        required property string setting
        required property NumberFormatSettings numberFormat

        signal notSaved()

        // Shows the setting's choice, also after choosing one separator
        // changed the other. Set rather than bound: the delegate sets its
        // index itself on phones, which would end a binding.
        function showChoice() {
            currentIndex = indexOfValue(numberFormat.choices[setting]);
        }

        textRole: "text"
        valueRole: "value"
        Component.onCompleted: showChoice()
        onActivated: {
            if (!numberFormat.choose(setting, currentValue)) {
                notSaved();
            }
        }

        Connections {
            target: choice.numberFormat

            function onChoicesChanged() {
                choice.showChoice();
            }
        }
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            descriptionItem.textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@label an example amount", "Example")
            description: page.numberFormat.example + "    " + page.numberFormat.negativeExample
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "DecimalSeparator"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Decimal separator")
            model: [
                { value: "regional", text: page.i18nContext.i18nc("@item:inlistbox", "As in your regional settings") },
                { value: "comma", text: page.i18nContext.i18nc("@item:inlistbox", "Comma: 1,50") },
                { value: "point", text: page.i18nContext.i18nc("@item:inlistbox", "Point: 1.50") },
            ]
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "DigitGrouping"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Thousands separator")
            model: [
                { value: "regional", text: page.i18nContext.i18nc("@item:inlistbox", "As in your regional settings") },
                { value: "point", text: page.i18nContext.i18nc("@item:inlistbox", "Point: 1.000.000") },
                { value: "comma", text: page.i18nContext.i18nc("@item:inlistbox", "Comma: 1,000,000") },
                { value: "space", text: page.i18nContext.i18nc("@item:inlistbox", "Space: 1 000 000") },
                { value: "apostrophe", text: page.i18nContext.i18nc("@item:inlistbox", "Apostrophe: 1'000'000") },
                { value: "none", text: page.i18nContext.i18nc("@item:inlistbox", "None: 1000000") },
            ]
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "CurrencySymbol"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Currency symbol")
            model: [
                { value: "regional", text: page.i18nContext.i18nc("@item:inlistbox", "As in your regional settings") },
                { value: "before", text: page.i18nContext.i18nc("@item:inlistbox", "Before the amount: €5") },
                { value: "after", text: page.i18nContext.i18nc("@item:inlistbox", "After the amount: 5 €") },
            ]
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "NegativeAmounts"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Amounts below zero")
            model: [
                { value: "regional", text: page.i18nContext.i18nc("@item:inlistbox", "As in your regional settings") },
                { value: "minus", text: page.i18nContext.i18nc("@item:inlistbox", "With a minus sign: -5") },
                { value: "parentheses", text: page.i18nContext.i18nc("@item:inlistbox", "In parentheses: (5)") },
            ]
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "DecimalPlaces"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Decimal places")
            model: [
                { value: "all", text: page.i18nContext.i18nc("@item:inlistbox", "All the currency has: 5,00") },
                { value: "none", text: page.i18nContext.i18nc("@item:inlistbox", "None, rounded: 5") },
            ]
        }
    }

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        text: page.formatNotSaved
            ? page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice, so it applies only until you close KassenSturz. Check that your settings folder can be written to, then choose again.")
            : page.i18nContext.i18nc("@info", "This changes only how amounts look on this computer, not the amounts themselves. KassenSturz always calculates with the exact amounts, so rounded amounts may not add up exactly on screen: point at one to see it exactly. You can still type amounts with all their decimal places.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Undo and history")
    }

    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            id: keepFor

            // Shows the history's setting, also after the dialog turned the
            // choice down. Set rather than bound: the delegate sets its
            // index itself on phones, which would end a binding.
            function showChoice() {
                currentIndex = indexOfValue(page.history.retention);
            }

            text: page.i18nContext.i18nc("@label:listbox", "Keep the history for")
            textRole: "text"
            valueRole: "value"
            model: page.keepChoices
            Component.onCompleted: showChoice()
            onModelChanged: Qt.callLater(showChoice)
            onActivated: page.chooseRetention(currentValue)
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            descriptionItem.textFormat: Text.PlainText
            icon.name: "view-history"
            text: page.i18nContext.i18nc("@action:button", "Show history")
            description: page.i18nContext.i18ncp("@info %1 is how many changes there are", "%1 change so far.", "%1 changes so far.", page.history.total)
            onClicked: page.showHistoryClicked()
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "edit-clear-history"
            text: page.i18nContext.i18nc("@action:button", "Clean up history…")
            description: page.i18nContext.i18nc("@info", "Deletes the whole history. Your accounts and transactions stay as they are.")
            enabled: page.history.total > 0
            onClicked: cleanUpDialog.open()
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Undo takes back your changes, newest first, also after you close and open the book again. The longer the history is kept, the further back Undo goes. With \"Only while the book is open\", nothing is saved in the book: Undo works until you close it.")
    }

    Kirigami.PromptDialog {
        id: shorterDialog

        title: page.i18nContext.i18nc("@title:window", "Delete part of the history?")
        subtitle: page.pendingRetention === "0"
            ? page.i18nContext.i18ncp("@info %1 is how many changes there are", "Undo will then work only until you close the book. The %1 change made so far will be gone by then. Your accounts and transactions are not affected.", "Undo will then work only until you close the book. The %1 changes made so far will be gone by then. Your accounts and transactions are not affected.", page.pendingCount)
            : page.i18nContext.i18ncp("@info %1 is how many changes there are", "%1 change is older than this and will be deleted for good. You won't be able to undo it or make it again. Your accounts and transactions are not affected.", "%1 changes are older than this and will be deleted for good. You won't be able to undo them or make them again. Your accounts and transactions are not affected.", page.pendingCount)
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Delete them")
                icon.name: "edit-delete"
                onTriggered: {
                    page.saveRetention(page.pendingRetention);
                    shorterDialog.close();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep the history as it is")
                icon.name: "dialog-cancel"
                onTriggered: shorterDialog.close()
            }
        ]
        onClosed: keepFor.showChoice()
    }

    Kirigami.PromptDialog {
        id: cleanUpDialog

        title: page.i18nContext.i18nc("@title:window", "Clean up the history?")
        subtitle: page.i18nContext.i18nc("@info", "This deletes the whole history for good, so nothing you did so far can be undone or made again. Your accounts, transactions and everything else stay exactly as they are.\n\nCopies of the book that KassenSturz saved before an update (backups) keep their history; cleaning up does not touch them.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Delete the history")
                icon.name: "edit-clear-history"
                onTriggered: {
                    page.historyResult = page.history.cleanUp();
                    cleanUpDialog.close();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep the history")
                icon.name: "dialog-cancel"
                onTriggered: cleanUpDialog.close()
            }
        ]
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Backups")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Backup folder")
            description: page.session.backupFolder
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Before KassenSturz updates a book to work with a new version, it saves a copy of it here. It keeps the last 10 copies of each book.")
    }
}
