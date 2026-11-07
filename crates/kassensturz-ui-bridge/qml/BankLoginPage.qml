// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Changes a bank login that already works, in place (D-617): the approval
// (TAN) method and the FinTS version. Nothing is set up again, no PIN is
// asked and nothing is sent to the bank; the accounts, the links and the PIN
// stay as they are. The page starts from the settings the login has now.
// The login name is shown here, as text, so it can be compared with the
// bank's own information; it is shown nowhere else (D-617, threat T-19).
// What can't be changed in place (the bank's address, the login name) is
// one step away: setting the login up again.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property BankConnection bank
    // The login being changed: an object from bank.logins().
    required property var login

    // What bank.loginDetails() returned.
    property var details: ({})
    // The approval methods to choose from: entries of `value` (the number
    // AqBanking takes) and `text`. Empty if the bank has told none.
    property var tanChoices: []

    signal done()
    // The user wants to change what can't be changed in place.
    signal setUpAgainClicked(var login)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: Plain.plain(i18nContext.i18nc("@title", "Change a bank login"))

    header: ColumnLayout {
        spacing: 0

        FormError {
            id: error
            Layout.fillWidth: true
        }
    }

    // Reads the lines of an error's `methods` (`number`, `version` and the
    // bank's name, separated by tabs) into the pick list (D-615).
    function setChoicesFromLines(lines: string) {
        const found = [];
        for (const line of lines.split("\n")) {
            const parts = line.split("\t");
            if (parts.length >= 3) {
                found.push({ number: parts[0], version: parts[1], name: parts.slice(2).join(" ") });
            }
        }
        setChoices(found);
    }

    // Builds the pick list from entries with `number`, `version` and `name`;
    // the method in use is the one it starts on.
    function setChoices(methods: var) {
        const found = [];
        let current = 0;
        for (const method of methods) {
            const function_ = Number(method.number) % 1000;
            if (method.number === (details.tanMethod ?? "")) {
                current = found.length + 1;
            }
            found.push({
                value: method.number,
                text: Plain.plain(method.name !== ""
                    ? i18nContext.i18nc("@item:inlistbox %1 is the method's number, %2 its name, %3 the FinTS version", "%1 - %2 (version %3)", function_, method.name, method.version)
                    : i18nContext.i18nc("@item:inlistbox %1 is the method's number, %2 the FinTS version", "%1 (version %2)", function_, method.version)),
            });
        }
        if (found.length === 0) {
            tanChoices = [];
            return;
        }
        found.unshift({ value: "", text: i18nContext.i18nc("@item:inlistbox", "Leave it as it is") });
        tanChoices = found;
        tanMethodBox.currentIndex = current;
    }

    function save() {
        const result = bank.changeLogin(
            login.id,
            versionBox.currentValue,
            tanChoices.length > 0 ? tanMethodBox.currentValue : tanMethodField.text);
        error.result = result;
        if (result.code === undefined) {
            done();
        } else if (result.code === "bank.tan-method-needed" && result.methods !== undefined) {
            setChoicesFromLines(result.methods);
        }
    }

    Component.onCompleted: {
        const found = bank.loginDetails(login.id);
        error.result = found.code !== undefined ? found : ({});
        if (found.code !== undefined) {
            return;
        }
        details = found;
        setChoices(found.methods ?? []);
        versionBox.currentIndex = login.fintsVersion === "220" ? 1 : 0;
        tanMethodField.text = found.tanMethod ?? "";
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                text: page.i18nContext.i18nc("@info", "You change a login that works. Nothing is set up again, KassenSturz asks no PIN and sends nothing to your bank now. Your accounts, their links and the PIN stay as they are; the new settings are used at the next connection.")
            }

            Controls.Label {
                Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Bank:")
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                // From the bank lists: plain text (D-143).
                textFormat: Text.PlainText
                text: page.login.bank
            }

            Controls.Label {
                Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Login name:")
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.WrapAnywhere
                textFormat: Text.PlainText
                visible: (page.details.loginName ?? "") !== ""
                text: page.details.loginName ?? ""
            }

            Controls.Label {
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox the bank's FinTS server address", "FinTS address:")
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.WrapAnywhere
                textFormat: Text.PlainText
                font: Kirigami.Theme.smallFont
                visible: page.login.url !== ""
                text: page.login.url
            }

            Controls.ComboBox {
                id: versionBox
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "FinTS version:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { value: "300", text: page.i18nContext.i18nc("@item:inlistbox", "3.0 (nearly all banks)") },
                    { value: "220", text: page.i18nContext.i18nc("@item:inlistbox", "2.2 (only if your bank says so)") },
                ]
            }

            // The bank's own methods with their names, when it has told
            // them (D-615).
            Controls.ComboBox {
                id: tanMethodBox
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Approval method:")
                visible: page.tanChoices.length > 0
                textRole: "text"
                valueRole: "value"
                model: page.tanChoices
            }

            Controls.TextField {
                id: tanMethodField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Approval method number:")
                visible: page.tanChoices.length === 0
                placeholderText: page.i18nContext.i18nc("@info:placeholder", "Let the bank choose")
                inputMethodHints: Qt.ImhDigitsOnly | Qt.ImhNoPredictiveText
                validator: IntValidator { bottom: 900; top: 9999 }
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Save")
                icon.name: "document-save"
                onClicked: {
                    ActionLog.taken("BankLoginPage_save")
                    page.save()
                }
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
                text: page.i18nContext.i18nc("@info", "The bank's address and the login name can't be changed here. To change them, set the login up again: you type the login details and the PIN once more, and your accounts stay linked.")
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Set this login up again…")
                icon.name: "configure"
                flat: true
                onClicked: {
                    ActionLog.taken("BankLoginPage_setUpAgain")
                    page.setUpAgainClicked(page.login)
                }
            }
        }
    }
}
