// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The settings group "Advanced" (D-795), for what most households never
// change: where the book is kept (D-731), the folders KassenSturz may open
// (#873), and where saved PINs go (D-057, D-163, D-167).

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
    // Where the book is kept (D-731).
    required property BookMove bookMove

    // "Where your book is kept" (D-731).
    signal bookPlaceClicked()
    signal grantedFoldersClicked()

    // Where saved PINs go (D-163): the places this build offers here and
    // what choosing one did. The place being got ready, e.g. KeePassXC
    // waiting for the user (D-167). All places are shown: this whole page is
    // one deliberate step away (D-795).
    property var pinStorageChoices: []
    property var pinStorageResult: ({})
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


    Component.onCompleted: {
        bookMove.refresh();
        pinStorageChoices = Array.from(bank.pinStorageChoices().choices ?? []);
    }

    Connections {
        target: page.bank

        function onPinStorageFinished(result) {
            page.pinStorageWaiting = "";
            page.pinStorageResult = result;
        }
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

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        text: page.i18nContext.i18nc("@info", "Most households never need to change anything here.")
    }

    FormCard.FormHeader {
        visible: page.bookMove.available || page.bookMove.grantsAvailable
        title: page.i18nContext.i18nc("@title:group", "Where your book is")
    }

    FormCard.FormCard {
        visible: page.bookMove.available || page.bookMove.grantsAvailable

        // Where the book is kept, and moving it there (D-731). Only where a
        // folder can be chosen (D-730).
        FormCard.FormButtonDelegate {
            visible: page.bookMove.available
            icon.name: "folder-move"
            text: page.i18nContext.i18nc("@action:button", "Where your book is kept")
            description: page.placeText(page.bookMove.place)
            descriptionItem.textFormat: Text.PlainText
            onClicked: {
                ActionLog.taken("SettingsAdvancedPage_bookPlace")
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
                ActionLog.taken("SettingsAdvancedPage_grantedFolders")
                page.grantedFoldersClicked()
            }
        }
    }

    // Where KassenSturz saves a PIN the user chooses to save (D-057,
    // D-163): the system keychain first, then the other places.
    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Password storage")
    }

    FormCard.FormCard {
        Repeater {
            model: page.pinStorageChoices

            delegate: FormCard.FormRadioDelegate {
                id: storage

                required property var modelData

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
                    ActionLog.taken("SettingsAdvancedPage_clicked")
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
                ActionLog.taken("SettingsAdvancedPage_connectKeepassxcAgain")
                page.choosePinStorage("keepassxc")
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

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Advanced")
}
