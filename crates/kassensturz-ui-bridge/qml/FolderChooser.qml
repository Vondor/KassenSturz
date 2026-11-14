// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Choosing a folder the app may keep files in (D-730): the system's folder
// chooser (in the Flatpak the file chooser portal, which grants the app this
// one folder), then FolderChoice decides whether the folder is used. Pages
// for moving the book, the backup folder and the bill folder put one of these
// in and call `open()`. `granted` carries `path` (what the app opens),
// `shownAs` (what the user is shown, as plain text only) and
// `shownIsRealPath`; `refused` carries the code and a sentence that says what
// to do next. The folder's real path is never written to a log (D-730).

import QtQuick
import QtQuick.Dialogs as Dialogs
import org.kde.ki18n
import io.github.vondor.kassensturz

Item {
    id: chooser

    // The dialog's title, in the words of the page that asks.
    property string title: i18nContext.i18nc("@title:window", "Choose a folder")
    // Whether this computer offers choosing a folder at all.
    readonly property bool available: choice.available

    // Whether a book is to live in the folder, which needs file locks.
    property bool forBook: false

    signal granted(var grant)
    signal refused(string code, string reason)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function open() {
        dialog.open();
    }

    function reasonFor(code: string): string {
        switch (code) {
        case "folder.no-locks":
            return i18nContext.i18nc("@info", "KassenSturz is not allowed to keep a book in this folder. Allow it to use your home folder or removable media in your system's app permissions, then choose the folder again.");
        case "folder.too-broad":
            return i18nContext.i18nc("@info", "Please choose a folder inside your home folder, for example one just for your book.");
        default:
            return i18nContext.i18nc("@info", "KassenSturz can't use this folder. Please choose another one.");
        }
    }

    visible: false

    FolderChoice {
        id: choice
    }

    Dialogs.FolderDialog {
        id: dialog

        title: chooser.title
        onAccepted: {
            ActionLog.taken("FolderChooser_chosen")
            const grant = choice.grant(selectedFolder, chooser.forBook);
            if (grant.ok) {
                chooser.granted(grant);
            } else {
                chooser.refused(grant.code, chooser.reasonFor(grant.code));
            }
        }
    }
}
