// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's settings: which book is open, opening another one or starting a
// new one (D-033, for people who want more than one book), how amounts look
// (D-002, D-114), the colors of the charts (D-589), the language (D-127), saving a copy of the data and starting a book from a file
// (M3.12), the bank logins on this computer and their PINs (M4.6, D-057),
// how long the history of changes is kept (D-066, M3.8), where the
// backups go, copies in a folder the user chose and restoring one (#806),
// and which version of the app is running (D-152).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookSession session
    required property NumberFormatSettings numberFormat
    required property ChartPaletteSettings chartPalette
    required property LanguageSettings language
    required property AiSwitch ai
    required property ChangeHistory history
    required property BankConnection bank
    required property FetchGapSettings fetchGap
    // Where the book is kept (D-731).
    required property BookMove bookMove

    // Whether a number format choice couldn't be saved for the next start.
    property bool formatNotSaved: false
    // Whether the choice of when to remind about fetching couldn't be saved.
    property bool fetchGapNotSaved: false
    // Whether the language choice couldn't be saved.
    property bool languageNotSaved: false
    // Whether the AI switch couldn't be saved.
    property bool aiNotSaved: false
    // Why the folder for backup copies wasn't taken, or "" (#806).
    property string copiesChoiceProblem: ""

    signal openAnotherClicked()
    signal exampleBookClicked()
    signal newBookClicked()
    signal saveCopyClicked()
    // "Where your book is kept" (D-731).
    signal bookPlaceClicked()
    signal grantedFoldersClicked()
    signal bookFromFileClicked()
    signal showHistoryClicked()
    // The user wants the list of backups to restore one (#806).
    signal restoreBackupClicked()
    // The user wants the page of the book's tags (US4.8).
    signal tagsClicked()
    // The user wants the help page on patterns and formulas.
    signal helpClicked()
    // The user wants the page of the provider and of what is sent; only
    // offered while the switch is on.
    signal aiSettingsClicked()
    signal connectBankClicked()
    // The user wants to change this login (an object from bank.logins()).
    signal changeBankClicked(var login)
    // The user wants to link another account of this login (D-617).
    signal linkAccountClicked(var login)

    // What bank.status() and bank.logins() returned, and why the last
    // change to a login failed.
    property var bankStatus: ({})
    property var bankLogins: []
    property var bankResult: ({})
    // The login the removal dialog asks about.
    property var pendingLogin: ({})

    // Where saved PINs go (D-163): the places this build offers here, what
    // choosing one did, and whether the advanced ones are shown. The place
    // being got ready, e.g. KeePassXC waiting for the user (D-167).
    property var pinStorageChoices: []
    property var pinStorageResult: ({})
    property bool showAdvancedStorage: false
    property string pinStorageWaiting: ""

    // Gets the place `key` ready and saves PINs there from now on; the
    // chosen place again connects to it once more (D-167).
    function choosePinStorage(key: string) {
        pinStorageResult = ({});
        const started = key === bank.pinStorage ? bank.openPinStorage() : bank.choosePinStorage(key);
        if (started.code !== undefined)
            pinStorageResult = started;
        else
            pinStorageWaiting = key;
    }

    // Reads the bank logins again, e.g. after one was set up or removed.
    function refreshBanks() {
        bankStatus = bank.status();
        const result = bankStatus.ready ? bank.logins() : ({ logins: [] });
        bankLogins = result.logins ?? [];
        if (result.code !== undefined) {
            bankResult = result;
        }
    }

    Component.onCompleted: {
        bookMove.refresh();
        refreshBanks();
        pinStorageChoices = Array.from(bank.pinStorageChoices().choices ?? []);
    }

    // A login set up on its page is listed here at once.
    Connections {
        target: page.bank

        function onSetupFinished() {
            page.refreshBanks();
        }

        function onPinStorageFinished(result) {
            page.pinStorageWaiting = "";
            page.pinStorageResult = result;
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

    // Where the book is, in the user's words, and where it was before the
    // last move.
    function placeText(place: var): string {
        if (!place || !place.open)
            return "";
        const folder = (shown) => shown && shown.own
            ? i18nContext.i18nc("@info the folder KassenSturz keeps its own book in", "KassenSturz's own folder")
            : (shown?.text ?? "");
        const here = folder(place.folder);
        if (!place.moved)
            return here;
        return here + "\n" + i18nContext.i18nc("@info %1 is a date and time, %2 a folder", "Moved on %1 from %2", place.movedOn, folder(place.movedFrom));
    }

    title: i18nContext.i18nc("@title", "Settings")
    header: ColumnLayout {
        spacing: 0

        // A new language applies at the next start (D-127): said at the top
        // of the page, where the other information bars are (D-498, M6-B4),
        // so it is seen wherever the page is scrolled to.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            position: Kirigami.InlineMessage.Position.Header
            type: Kirigami.MessageType.Information
            visible: page.language.restartNeeded && !page.languageNotSaved
            text: page.i18nContext.i18nc("@info", "Close KassenSturz and open it again to see it in the new language.")
        }

        // Why the last change to the history settings failed, if it did.
        FormError {
            Layout.fillWidth: true
            result: page.historyResult
        }
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

        FormCard.FormButtonDelegate {
            icon.name: "tag"
            text: page.i18nContext.i18nc("@action:button", "Tags")
            description: page.i18nContext.i18nc("@info", "Rename, merge or remove your tags.")
            onClicked: {
                ActionLog.taken("SettingsPage_tags")
                page.tagsClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        // Opens the book's folder in the file manager (M5-F2). In the
        // Flatpak this goes through the desktop portal, so it works from the
        // sandbox.
        FormCard.FormButtonDelegate {
            icon.name: "folder-open"
            text: page.i18nContext.i18nc("@label the folder a file is in", "Folder")
            description: page.session.folder
            descriptionItem.textFormat: Text.PlainText
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: page.i18nContext.i18nc("@info:tooltip", "Open this folder in the file manager")
            onClicked: {
                ActionLog.taken("SettingsPage_folder")
                Qt.openUrlExternally(page.session.folderUrl)
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-open"
            text: page.i18nContext.i18nc("@action:button", "Open another book…")
            description: page.i18nContext.i18nc("@info", "KassenSturz opens the book you used last. Most households need only one.")
            onClicked: {
                ActionLog.taken("SettingsPage_openAnotherBook")
                page.openAnotherClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "folder-open"
            text: page.i18nContext.i18nc("@action:button", "Try an example book…")
            description: page.i18nContext.i18nc("@info", "Look around in a made-up household. KassenSturz makes a new book for it, so your own books are never changed.")
            onClicked: {
                ActionLog.taken("SettingsPage_tryExampleBook")
                page.exampleBookClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-new"
            text: page.i18nContext.i18nc("@action:button", "Start a new book…")
            description: page.i18nContext.i18nc("@info", "An empty book in a folder you choose, for example to try something out.")
            onClicked: {
                ActionLog.taken("SettingsPage_startNewBook")
                page.newBookClicked()
            }
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Your data")
    }

    FormCard.FormCard {
        // Where the book is kept, and moving it there (D-731). Only where a
        // folder can be chosen (D-730).
        FormCard.FormButtonDelegate {
            visible: page.bookMove.available
            icon.name: "folder-move"
            text: page.i18nContext.i18nc("@action:button", "Where your book is kept")
            description: page.placeText(page.bookMove.place)
            descriptionItem.textFormat: Text.PlainText
            onClicked: {
                ActionLog.taken("SettingsPage_bookPlace")
                page.bookPlaceClicked()
            }
        }

        FormCard.FormDelegateSeparator {
            visible: page.bookMove.available
        }

        // The folders the user gave access to, to take back (#873). Only in
        // the Flatpak, where the access stays until it is taken back.
        FormCard.FormButtonDelegate {
            visible: page.bookMove.grantsAvailable
            icon.name: "folder-locked"
            text: page.i18nContext.i18nc("@action:button", "Folders KassenSturz may open")
            description: page.i18nContext.i18nc("@info", "See them and take the access back.")
            onClicked: {
                ActionLog.taken("SettingsPage_grantedFolders")
                page.grantedFoldersClicked()
            }
        }

        FormCard.FormDelegateSeparator {
            visible: page.bookMove.grantsAvailable
        }

        FormCard.FormButtonDelegate {
            icon.name: "document-save-as"
            text: page.i18nContext.i18nc("@action:button", "Save a copy of my data…")
            description: page.i18nContext.i18nc("@info", "For a spreadsheet, or the whole book as a backup.")
            onClicked: {
                ActionLog.taken("SettingsPage_saveCopyData")
                page.saveCopyClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-import"
            text: page.i18nContext.i18nc("@action:button", "Start a book from a file…")
            description: page.i18nContext.i18nc("@info", "From a KMyMoney book, or a whole book you saved with KassenSturz.")
            onClicked: {
                ActionLog.taken("SettingsPage_startBookFile")
                page.bookFromFileClicked()
            }
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
                    description: Plain.plain(login.modelData.kind === "chip-card"
                        ? page.i18nContext.i18nc("@info", "Login with a chip card")
                        : login.modelData.tanMethodName !== ""
                        ? page.i18nContext.i18nc("@info %1 is the name of an approval method", "Login with PIN and TAN, approval by: %1", login.modelData.tanMethodName)
                        : page.i18nContext.i18nc("@info", "Login with PIN and TAN"))
                }

                FormCard.FormButtonDelegate {
                    icon.name: "configure"
                    text: page.i18nContext.i18nc("@action:button", "Change this bank login…")
                    description: page.i18nContext.i18nc("@info", "Change the approval (TAN) method or the FinTS version of this login. Nothing is set up again and your accounts stay linked. The bank's address and the login name can be changed there too, by setting the login up again.")
                    enabled: page.bankStatus.ready ?? false
                    onClicked: {
                        ActionLog.taken("SettingsPage_changeBankLogin")
                        page.changeBankClicked(login.modelData)
                    }
                }

                FormCard.FormButtonDelegate {
                    icon.name: "list-add"
                    text: page.i18nContext.i18nc("@action:button", "Link another account…")
                    description: page.i18nContext.i18nc("@info", "Asks your bank again which accounts this login reaches, so you can link a new one without setting the login up again. You type your PIN if KassenSturz doesn't have it.")
                    enabled: page.bankStatus.ready ?? false
                    onClicked: {
                        ActionLog.taken("SettingsPage_linkAnotherAccount")
                        page.linkAccountClicked(login.modelData)
                    }
                }

                FormCard.FormButtonDelegate {
                    icon.name: "edit-clear"
                    text: page.i18nContext.i18nc("@action:button", "Forget the PIN")
                    description: page.i18nContext.i18nc("@info", "Removes the PIN KassenSturz remembers or saved for this login. You type it at the next connection.")
                    onClicked: {
                        ActionLog.taken("SettingsPage_forgetPin")
                        page.bankResult = page.bank.forgetPin(login.modelData.id)
                    }
                }

                FormCard.FormButtonDelegate {
                    icon.name: "edit-delete"
                    text: page.i18nContext.i18nc("@action:button", "Remove bank login and saved PIN…")
                    onClicked: {
                        ActionLog.taken("SettingsPage_removeBankLogin")
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
            onClicked: {
                ActionLog.taken("SettingsPage_connectBank")
                page.connectBankClicked()
            }
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

    // Where KassenSturz saves a PIN the user chooses to save (D-057,
    // D-163): the system keychain first, the other places one step away.
    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Password storage")
    }

    FormCard.FormCard {
        Repeater {
            model: page.pinStorageChoices

            delegate: FormCard.FormRadioDelegate {
                id: storage

                required property var modelData

                visible: !modelData.advanced || page.showAdvancedStorage || page.bank.pinStorage === modelData.key
                enabled: page.pinStorageWaiting === ""
                // The choice follows the setting, not the click: a choice
                // that couldn't be made stays unticked.
                checkable: false
                checked: page.bank.pinStorage === modelData.key
                text: {
                    switch (modelData.key) {
                    case "keepassxc":
                        return page.i18nContext.i18nc("@option:radio where PINs are saved", "A KeePassXC database");
                    case "nowhere":
                        return page.i18nContext.i18nc("@option:radio where PINs are saved", "Don't save PINs");
                    default:
                        return page.i18nContext.i18nc("@option:radio where PINs are saved", "System keychain");
                    }
                }
                description: {
                    switch (modelData.key) {
                    case "keepassxc":
                        return page.i18nContext.i18nc("@info", "For a KeePassXC database you open only when needed, apart from your desktop's keychain. KassenSturz never sees the database's password.");
                    case "nowhere":
                        return page.i18nContext.i18nc("@info", "KassenSturz never saves a PIN. You type it at each connection, or it remembers it until you close KassenSturz.");
                    default:
                        switch (Qt.platform.os) {
                        case "windows":
                            return page.i18nContext.i18nc("@info", "Recommended. The Windows Credential Manager keeps your PINs safe in the background.");
                        case "osx":
                            return page.i18nContext.i18nc("@info", "Recommended. The macOS Keychain keeps your PINs safe in the background.");
                        default:
                            return page.i18nContext.i18nc("@info", "Recommended. Your desktop's keychain keeps your PINs safe in the background: KWallet, GNOME Keyring, or KeePassXC if you set it up to take their place.");
                        }
                    }
                }
                onClicked: {
                    ActionLog.taken("SettingsPage_clicked")
                    if (modelData.key !== page.bank.pinStorage)
                        page.choosePinStorage(modelData.key);
                }
            }
        }

        // KeePassXC keeps the file; KassenSturz uses the database open in
        // front there and says which one (D-167).
        FormCard.FormDelegateSeparator {
            visible: keepassxcHelp.visible
        }

        FormCard.FormTextDelegate {
            id: keepassxcHelp

            // The database's and group's names are the user's own text.
            textItem.textFormat: Text.PlainText
            textItem.wrapMode: Text.Wrap
            descriptionItem.textFormat: Text.PlainText
            visible: page.bank.pinStorage === "keepassxc" || page.pinStorageWaiting === "keepassxc"
            text: page.pinStorageWaiting === "keepassxc"
                ? page.i18nContext.i18nc("@info", "Waiting for KeePassXC…")
                : page.pinStorageResult.place !== undefined
                    ? page.i18nContext.i18nc("@info %1 a KeePassXC database's name, %2 a group in it", "PINs are saved in the KeePassXC database “%1”, in its group “%2”.", page.pinStorageResult.place.name, page.pinStorageResult.place.group)
                    : page.i18nContext.i18nc("@info", "PINs are saved in the KeePassXC database open in front, in its group “KassenSturz”.")
            description: page.pinStorageWaiting === "keepassxc"
                ? page.i18nContext.i18nc("@info", "Switch to KeePassXC: unlock the database there, and allow KassenSturz when KeePassXC asks. If KeePassXC isn't running, start it first.")
                : page.i18nContext.i18nc("@info", "KeePassXC keeps the database file: you choose there where it is saved and how it is encrypted. To keep your PINs in a file of their own, create a new database in KeePassXC (Database → New Database), keep it open in front, and connect again. KassenSturz reaches KeePassXC through its Browser Integration, which you turn on in KeePassXC's settings.")
            trailing: Controls.BusyIndicator {
                visible: page.pinStorageWaiting !== ""
                running: visible
            }
        }

        FormCard.FormButtonDelegate {
            icon.name: "view-refresh"
            text: page.i18nContext.i18nc("@action:button", "Connect to KeePassXC again")
            visible: page.bank.pinStorage === "keepassxc" && page.pinStorageWaiting === ""
            onClicked: {
                ActionLog.taken("SettingsPage_connectKeepassxcAgain")
                page.choosePinStorage("keepassxc")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: page.showAdvancedStorage ? "arrow-up" : "arrow-down"
            text: Plain.plain(page.showAdvancedStorage
                ? page.i18nContext.i18nc("@action:button", "Hide advanced choices")
                : page.i18nContext.i18nc("@action:button", "Show advanced choices"))
            visible: page.pinStorageChoices.some(choice => choice.advanced && choice.key !== page.bank.pinStorage)
            onClicked: {
                ActionLog.taken("SettingsPage_showAdvancedStorage")
                page.showAdvancedStorage = !page.showAdvancedStorage
            }
        }
    }

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        text: page.pinStorageResult.saved === false
            ? page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then choose again.")
            : page.i18nContext.i18nc("@info", "KassenSturz saves a PIN only when you choose so as you type it. When you choose another place here, the PINs saved in the old one are removed, and you type each PIN again at the next connection.")
    }

    // The new place is in use, but the old one couldn't be emptied, e.g. it
    // can't be reached: a saved PIN or the AI key may still be there
    // (D-605).
    FormCard.FormSectionText {
        visible: page.pinStorageResult.leftBehind === true
        textFormat: Text.PlainText
        text: page.i18nContext.i18nc("@info", "The new place is in use, but KassenSturz could not remove everything from the old one, so a saved PIN or another saved secret may still be there. Nothing was copied to the new place. To remove them, open the old place yourself, for example your system's keychain manager or KeePassXC, and delete the entries named KassenSturz.")
    }

    // Why the choice couldn't be made, e.g. the new place can't be reached,
    // so the old one stays chosen (D-167).
    FormError {
        Layout.fillWidth: true
        Layout.maximumWidth: Kirigami.Units.gridUnit * 30
        Layout.alignment: Qt.AlignHCenter
        result: page.pinStorageResult
    }

    // When the overview reminds the user to fetch (M5.U, D-180): kept for
    // this computer, not in the book.
    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Reminder to get new transactions")
    }

    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            id: fetchGapChoice

            // Shows the saved choice, also after a choice couldn't be saved.
            function showChoice() {
                currentIndex = indexOfValue(page.fetchGap.choice);
            }

            text: page.i18nContext.i18nc("@label:listbox", "Remind me after")
            textRole: "text"
            valueRole: "value"
            model: [
                { value: "30", text: page.i18nContext.i18nc("@item:inlistbox remind about fetching", "30 days") },
                { value: "60", text: page.i18nContext.i18nc("@item:inlistbox remind about fetching", "60 days") },
                { value: "90", text: page.i18nContext.i18nc("@item:inlistbox remind about fetching", "90 days") },
                { value: "180", text: page.i18nContext.i18nc("@item:inlistbox remind about fetching", "180 days") },
                { value: "off", text: page.i18nContext.i18nc("@item:inlistbox remind about fetching", "Never remind me") },
            ]
            Component.onCompleted: showChoice()
            onActivated: {
                ActionLog.taken("SettingsPage_remindMeAfter")
                page.fetchGapNotSaved = !page.fetchGap.choose(currentValue)
            }
        }
    }

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        text: page.fetchGapNotSaved
            ? page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice, so it applies only until you close KassenSturz. Check that your settings folder can be written to, then choose again.")
            : page.i18nContext.i18nc("@info", "The overview reminds you when an account linked to your bank was last updated from it longer ago than this. Banks only list their recent transactions, so a long wait can leave some out.")
    }

    Kirigami.PromptDialog {
        id: removeLoginDialog

        title: page.i18nContext.i18nc("@title:window", "Remove this bank login?")
        subtitle: page.i18nContext.i18nc("@info", "KassenSturz forgets the login and its saved PIN on this computer. Your accounts and their transactions stay as they are, and so does which bank account fills which of them: set the login up again to fetch once more.")
            + (page.bank.pinStorage === "keepassxc"
                ? "\n\n" + page.i18nContext.i18nc("@info", "KeePassXC cannot delete an entry for another program, so the PIN's entry stays there with an empty password. You can delete it in KeePassXC.")
                : "")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Remove the login")
                icon.name: "edit-delete"
                onTriggered: {
                    ActionLog.taken("SettingsPage_removeLogin")
                    page.bankResult = page.bank.removeLogin(page.pendingLogin.id);
                    removeLoginDialog.close();
                    page.refreshBanks();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep it")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("SettingsPage_keep")
                    removeLoginDialog.close()
                }
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
            onActivated: {
                ActionLog.taken("SettingsPage_language")
                page.languageNotSaved = !page.language.choose(currentValue)
            }
        }

        // A choice that couldn't be saved says so under the box; the
        // restart note is the bar at the top of the page (D-498).
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            visible: page.languageNotSaved
            type: Kirigami.MessageType.Error
            text: page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then choose again.")
        }
    }

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        text: page.i18nContext.i18nc("@info", "Dates and amounts keep following your regional settings; change how amounts look below.")
    }

    // ai-toggle-begin: the one place AI shows while it is off (D-013). A
    // build without AI doesn't show it at all.
    FormCard.FormCard {
        visible: page.ai.available

        FormCard.FormSwitchDelegate {
            text: page.i18nContext.i18nc("@option:check", "AI features")
            description: page.i18nContext.i18nc("@info", "Off unless you turn it on. While it is off, nothing about AI is shown or runs.")
            checked: page.ai.enabled
            onToggled: {
                ActionLog.taken("SettingsPage_aiFeatures")
                page.aiNotSaved = !page.ai.switchTo(checked);
                checked = Qt.binding(() => page.ai.enabled);
            }
        }
    }

    FormCard.FormSectionText {
        visible: page.ai.available && page.aiNotSaved
        textFormat: Text.PlainText
        text: page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then try again.")
    }
    // ai-toggle-end

    AiSettingsEntry {
        ai: page.ai
        onOpenClicked: page.aiSettingsClicked()
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
            ActionLog.taken("SettingsPage_activated")
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
        title: page.i18nContext.i18nc("@title:group", "Chart colors")
    }

    // The colors of the charts (M7a.10, D-589).
    ChartColorsCard {
        Layout.fillWidth: true
        chartColors: page.chartPalette
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
            onActivated: {
                ActionLog.taken("SettingsPage_keepHistory")
                page.chooseRetention(currentValue)
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            descriptionItem.textFormat: Text.PlainText
            icon.name: "view-history"
            text: page.i18nContext.i18nc("@action:button", "Show history")
            description: page.i18nContext.i18ncp("@info %1 is how many changes there are", "%1 change so far.", "%1 changes so far.", page.history.total)
            onClicked: {
                ActionLog.taken("SettingsPage_showHistory")
                page.showHistoryClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "edit-clear-history"
            text: page.i18nContext.i18nc("@action:button", "Clean up history…")
            description: page.i18nContext.i18nc("@info", "Deletes the whole history. Your accounts and transactions stay as they are.")
            enabled: page.history.total > 0
            onClicked: {
                ActionLog.taken("SettingsPage_cleanUpHistory")
                cleanUpDialog.open()
            }
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
                    ActionLog.taken("SettingsPage_deleteThem")
                    page.saveRetention(page.pendingRetention);
                    shorterDialog.close();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep the history as it is")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("SettingsPage_keepHistory_2")
                    shorterDialog.close()
                }
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
                    ActionLog.taken("SettingsPage_deleteHistory")
                    page.historyResult = page.history.cleanUp();
                    cleanUpDialog.close();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep the history")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("SettingsPage_keepHistory_3")
                    cleanUpDialog.close()
                }
            }
        ]
    }

    // What the folder for copies says, from `backupCopies.status` (#806).
    function copiesStatusText(status: var): string {
        switch (status.state) {
        case "kept":
            return i18nContext.i18nc("@info %1 is when the copy was saved, e.g. 21.10.2026 14:02", "Newest copy there: %1", status.when);
        case "problem":
            return copiesChoiceText(status.code, i18nContext.i18nc("@info", "Your book and the backups on this computer are fine."));
        default:
            return i18nContext.i18nc("@info", "KassenSturz saves a copy there when it opens your book.");
        }
    }

    // The words for a code of the folder for copies; `fine` is added where
    // the book itself is not affected.
    function copiesChoiceText(code: string, fine: string): string {
        const sentences = (first, next) => fine === "" ? first + " " + next : first + " " + fine + " " + next;
        switch (code) {
        case "backup-copy.missing":
            return sentences(i18nContext.i18nc("@info", "The folder isn't there right now, so no copy was saved there."), i18nContext.i18nc("@info", "If it is on a USB stick or another drive, plug it in and choose Try again, or choose another folder."));
        case "backup-copy.full":
            return sentences(i18nContext.i18nc("@info", "The disk with that folder is full, so no copy was saved there."), i18nContext.i18nc("@info", "Make room there and choose Try again, or choose another folder."));
        case "backup-copy.read-only":
            return sentences(i18nContext.i18nc("@info", "KassenSturz may not write to that folder, so no copy was saved there."), i18nContext.i18nc("@info", "Please choose another folder."));
        case "backup-copy.same-folder":
            return i18nContext.i18nc("@info", "That is KassenSturz's own backup folder. Please choose a folder on another drive, for example a USB stick.");
        case "backup-copy.not-saved":
            return i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then choose again.");
        default:
            return sentences(i18nContext.i18nc("@info", "KassenSturz couldn't save a copy there."), i18nContext.i18nc("@info", "Choose Try again, or choose another folder."));
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Backups")
    }

    // Copies in a folder the user chose, e.g. on a USB stick, and restoring
    // a backup (#806). The folder belongs to this computer, not to the book.
    BackupCopies {
        id: backupCopies
    }

    FolderChooser {
        id: copiesChooser

        title: page.i18nContext.i18nc("@title:window", "Choose a folder for copies of your book")
        onGranted: grant => {
            const result = backupCopies.choose(grant.path);
            page.copiesChoiceProblem = result.ok ? "" : page.copiesChoiceText(result.code, "");
        }
        onRefused: (code, reason) => {
            page.copiesChoiceProblem = reason;
        }
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Backup folder")
            description: page.session.backupFolder
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {
            visible: copiesButton.visible
        }

        FormCard.FormButtonDelegate {
            id: copiesButton

            visible: copiesChooser.available || backupCopies.chosen
            icon.name: "folder-open"
            text: Plain.plain(backupCopies.chosen
                ? page.i18nContext.i18nc("@action:button %1 is the folder", "Also keep copies in: %1", backupCopies.shownAs)
                : page.i18nContext.i18nc("@action:button", "Also keep copies in another folder…"))
            description: backupCopies.chosen
                ? page.i18nContext.i18nc("@info", "Choose another folder")
                : page.i18nContext.i18nc("@info", "For example on a USB stick, or in a folder your cloud app uploads")
            // The folder's path holds the user's own names.
            descriptionItem.textFormat: Text.PlainText
            onClicked: {
                ActionLog.taken("SettingsPage_chooseCopiesFolder")
                page.copiesChoiceProblem = "";
                copiesChooser.open();
            }
        }

        FormCard.FormDelegateSeparator {
            visible: backupCopies.chosen
        }

        FormCard.FormComboBoxDelegate {
            id: copiesKeepChoice

            // Shows the saved choice, also after a choice couldn't be saved.
            function showChoice() {
                currentIndex = indexOfValue(backupCopies.keep);
            }

            visible: backupCopies.chosen
            text: page.i18nContext.i18nc("@label:listbox", "Copies of each book to keep there")
            textRole: "text"
            valueRole: "value"
            model: [
                { value: "3", text: page.i18nContext.i18nc("@item:inlistbox how many copies", "3") },
                { value: "5", text: page.i18nContext.i18nc("@item:inlistbox how many copies", "5") },
                { value: "10", text: page.i18nContext.i18nc("@item:inlistbox how many copies", "10") },
                { value: "20", text: page.i18nContext.i18nc("@item:inlistbox how many copies", "20") },
            ]
            Component.onCompleted: showChoice()
            onActivated: {
                ActionLog.taken("SettingsPage_copiesToKeep")
                page.copiesChoiceProblem = backupCopies.chooseKeep(currentValue)
                    ? ""
                    : page.copiesChoiceText("backup-copy.not-saved", "");
                showChoice();
            }
        }

        FormCard.FormDelegateSeparator {
            visible: backupCopies.chosen
        }

        FormCard.FormTextDelegate {
            visible: backupCopies.chosen
            text: page.copiesStatusText(backupCopies.status)
            textItem.textFormat: Text.PlainText
            textItem.wrapMode: Text.Wrap
            Accessible.role: backupCopies.status.state === "problem" ? Accessible.AlertMessage : Accessible.StaticText
        }

        // A row of its own: as a trailing button of the line above it didn't
        // show in the VM, while the text tells the user to choose it
        // (M6-B241).
        FormCard.FormDelegateSeparator {
            visible: copiesTryAgain.visible
        }

        FormCard.FormButtonDelegate {
            id: copiesTryAgain

            visible: backupCopies.chosen && backupCopies.status.state === "problem"
            icon.name: "view-refresh"
            text: page.i18nContext.i18nc("@action:button make a copy again", "Try again")
            onClicked: {
                ActionLog.taken("SettingsPage_copyAgain")
                backupCopies.tryAgain()
            }
        }

        FormCard.FormDelegateSeparator {
            visible: backupCopies.chosen
        }

        FormCard.FormButtonDelegate {
            visible: backupCopies.chosen
            icon.name: "edit-delete-remove"
            text: page.i18nContext.i18nc("@action:button", "Stop keeping copies there")
            description: page.i18nContext.i18nc("@info", "The copies already there stay")
            onClicked: {
                ActionLog.taken("SettingsPage_stopCopies")
                page.copiesChoiceProblem = backupCopies.forget()
                    ? ""
                    : page.copiesChoiceText("backup-copy.not-saved", "");
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-revert"
            text: page.i18nContext.i18nc("@action:button", "Restore from backup…")
            description: page.i18nContext.i18nc("@info", "Go back to an earlier copy of your book")
            onClicked: {
                ActionLog.taken("SettingsPage_restoreFromBackup")
                page.restoreBackupClicked()
            }
        }
    }

    FormCard.FormSectionText {
        visible: page.copiesChoiceProblem !== ""
        textFormat: Text.PlainText
        text: page.copiesChoiceProblem
        Accessible.role: Accessible.AlertMessage
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Before KassenSturz updates a book to work with a new version, it saves a copy of it here. It keeps the last 10 copies of each book.")
    }

    FormCard.FormSectionText {
        visible: copiesButton.visible
        text: page.i18nContext.i18nc("@info", "These copies are on the same disk as your book. If you choose another folder, for example on a USB stick or in a folder your cloud app uploads, KassenSturz also keeps a copy there once a day when you open your book, and a copy of every backup.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "About")
    }

    FormCard.FormCard {
        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button opens a help page", "Help: patterns, formulas and charts")
            description: page.i18nContext.i18nc("@info", "How to write a pattern in a Rule, a formula for a chart and the description of a custom chart")
            onClicked: {
                ActionLog.taken("SettingsPage_helpPatternsFormulas")
                page.helpClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            descriptionItem.textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@label the version of KassenSturz that is running", "Version")
            // A bundle built in a pull request and reused on main shows the
            // pull request's last commit, not main's merge (D-427, #758).
            description: page.session.fromPullRequest
                ? page.i18nContext.i18nc("@info %1 is the version and its commit, e.g. 0.261112.15 (abc1234); the commit is the last one of a pull request", "%1, built from a pull request's last commit (on main: the merge of that commit)", page.session.version)
                : page.session.version
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button opens the project's web page in the browser", "Project page on GitHub")
            description: "https://github.com/Vondor/KassenSturz"
            descriptionItem.textFormat: Text.PlainText
            onClicked: {
                ActionLog.taken("SettingsPage_projectPageGithub")
                Qt.openUrlExternally("https://github.com/Vondor/KassenSturz")
            }
        }
    }
}
