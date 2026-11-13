// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Asks for the name of a new book once its folder is chosen (D-733): in the
// Flatpak a save dialog hands over one file only, so the folder is chosen
// first and the name is typed here. `named` carries what the user typed;
// the app adds ".ksz" and refuses a name that isn't a plain file name.
//
//     dialog.ask(grant.shownAs, "Household");

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.PromptDialog {
    id: dialog

    // The folder as the user knows it, plain text only.
    property string folderText: ""
    // Why the last name wasn't used, or "".
    property string refusal: ""

    signal named(string name)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function ask(folder: string, suggestion: string) {
        folderText = folder;
        refusal = "";
        nameField.text = suggestion;
        open();
        nameField.forceActiveFocus();
        nameField.selectAll();
    }

    function done() {
        const name = nameField.text.trim();
        if (name === "") {
            return;
        }
        close();
        named(name);
    }

    title: i18nContext.i18nc("@title:window", "Name your book")
    subtitle: i18nContext.i18nc("@info", "KassenSturz keeps the book in the folder you chose and adds \".ksz\" to its name.")
    standardButtons: Kirigami.Dialog.NoButton
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Make the book")
            icon.name: "document-new"
            enabled: nameField.text.trim() !== ""
            onTriggered: {
                ActionLog.taken("BookNameDialog_make")
                dialog.done()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Cancel")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("BookNameDialog_cancel")
                dialog.close()
            }
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            opacity: 0.7
            visible: dialog.folderText !== ""
            text: dialog.i18nContext.i18nc("@info %1 is a folder", "Folder: %1", dialog.folderText)
        }

        Controls.TextField {
            id: nameField

            Layout.fillWidth: true
            Accessible.name: dialog.i18nContext.i18nc("@label:textbox", "Name of the book")
            onAccepted: {
                ActionLog.taken("BookNameDialog_accept")
                dialog.done()
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            visible: dialog.refusal !== ""
            color: Kirigami.Theme.negativeTextColor
            text: dialog.refusal
        }
    }
}
