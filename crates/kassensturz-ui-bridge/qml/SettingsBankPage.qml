// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The settings group "Bank" (D-795): the bank logins on this computer and
// their PINs (M4.6, D-057), connecting a bank, and when the overview reminds
// the user to fetch (M5.U, D-180). Where PINs are saved is in the group
// "Advanced" (SettingsAdvancedPage.qml).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BankConnection bank
    required property FetchGapSettings fetchGap

    // Whether the choice of when to remind about fetching couldn't be saved.
    property bool fetchGapNotSaved: false

    signal connectBankClicked()
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
                        ActionLog.taken("SettingsBankPage_changeBankLogin")
                        page.changeBankClicked(login.modelData)
                    }
                }

                FormCard.FormButtonDelegate {
                    icon.name: "list-add"
                    text: page.i18nContext.i18nc("@action:button", "Link another account…")
                    description: page.i18nContext.i18nc("@info", "Asks your bank again which accounts this login reaches, so you can link a new one without setting the login up again. You type your PIN if KassenSturz doesn't have it.")
                    enabled: page.bankStatus.ready ?? false
                    onClicked: {
                        ActionLog.taken("SettingsBankPage_linkAnotherAccount")
                        page.linkAccountClicked(login.modelData)
                    }
                }

                FormCard.FormButtonDelegate {
                    icon.name: "edit-clear"
                    text: page.i18nContext.i18nc("@action:button", "Forget the PIN")
                    description: page.i18nContext.i18nc("@info", "Removes the PIN KassenSturz remembers or saved for this login. You type it at the next connection.")
                    onClicked: {
                        ActionLog.taken("SettingsBankPage_forgetPin")
                        page.bankResult = page.bank.forgetPin(login.modelData.id)
                    }
                }

                FormCard.FormButtonDelegate {
                    icon.name: "edit-delete"
                    text: page.i18nContext.i18nc("@action:button", "Remove bank login and saved PIN…")
                    onClicked: {
                        ActionLog.taken("SettingsBankPage_removeBankLogin")
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
                ActionLog.taken("SettingsBankPage_connectBank")
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
                ActionLog.taken("SettingsBankPage_remindMeAfter")
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
                    ActionLog.taken("SettingsBankPage_removeLogin")
                    page.bankResult = page.bank.removeLogin(page.pendingLogin.id);
                    removeLoginDialog.close();
                    page.refreshBanks();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep it")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("SettingsBankPage_keep")
                    removeLoginDialog.close()
                }
            }
        ]
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Bank")
}
