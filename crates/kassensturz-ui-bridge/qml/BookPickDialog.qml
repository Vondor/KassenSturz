// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Lets the user pick one of several books in the folder they chose (D-733):
// in the Flatpak "Open another book…" asks for a folder first. `picked`
// carries the book's file name. With one book, the page opens it at once
// and this isn't shown.
//
//     dialog.ask(["Household.ksz", "Trip.ksz"]);

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.PromptDialog {
    id: dialog

    // The books' file names.
    property var names: []

    signal picked(string name)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function ask(books: var) {
        names = books;
        choice.currentIndex = 0;
        open();
    }

    title: i18nContext.i18nc("@title:window", "Which book?")
    subtitle: i18nContext.i18nc("@info", "There is more than one book in this folder.")
    standardButtons: Kirigami.Dialog.NoButton
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Open")
            icon.name: "document-open"
            enabled: dialog.names.length > 0
            onTriggered: {
                ActionLog.taken("BookPickDialog_open")
                const name = dialog.names[choice.currentIndex];
                dialog.close();
                dialog.picked(name);
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Cancel")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("BookPickDialog_cancel")
                dialog.close()
            }
        }
    ]

    Controls.ComboBox {
        id: choice

        Layout.fillWidth: true
        // File names come from the user's disk: never read as markup.
        model: dialog.names.map(name => Plain.plain(name))
        Accessible.name: dialog.i18nContext.i18nc("@label:listbox", "Book")
    }
}
