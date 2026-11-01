// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Says why a book didn't open, and what the user can do (AGENTS.md §6: no
// dead ends): try again, open it anyway if it's locked (D-038, D-080), open
// another book, start a new one or start one from a file, or go back to the
// book that is open.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Page {
    id: page

    required property BookSession session
    readonly property var problem: session.problem || ({})
    readonly property string code: problem.code || ""
    // Main.qml checks this, so a second problem replaces the page's text
    // instead of stacking another page.
    readonly property bool showsBookProblem: true

    signal openAnotherClicked()
    signal newBookClicked()
    signal startFromFileClicked()
    signal backClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Where the file is, e.g. "Household.ksz in /home/alex/Documents".
    readonly property string where: problem.folder
        ? i18nContext.i18nc("@info %1 is a file name, %2 the folder it is in", "%1 in %2", problem.file || "", problem.folder)
        : (problem.file || "")

    function heading(): string {
        switch (code) {
        case "store.locked":
            return problem.here
                ? i18nContext.i18nc("@title", "This book is still marked as open on this computer")
                : i18nContext.i18nc("@title %1 is the name of a computer", "This book is open on %1", problem.computer || i18nContext.i18nc("@item instead of a computer's name", "another computer"));
        case "book.missing":
            return i18nContext.i18nc("@title", "KassenSturz can't find your book");
        case "book.sandboxed":
            return i18nContext.i18nc("@title", "KassenSturz can't open a book from there");
        case "book.no-app-folder":
            return i18nContext.i18nc("@title", "KassenSturz has no folder for your book");
        case "store.already-exists":
            return i18nContext.i18nc("@title", "There is a file with this name already");
        default:
            return i18nContext.i18nc("@title", "KassenSturz couldn't open this book");
        }
    }

    function explanation(): string {
        switch (code) {
        case "store.locked": {
            const since = problem.since
                ? i18nContext.i18nc("@info %1 is a date and time", "It has been open since %1.", problem.since)
                : "";
            const what = problem.here
                ? i18nContext.i18nc("@info", "KassenSturz may be open in another window, or it didn't close properly last time. If it isn't open anymore, you can open the book anyway.")
                : i18nContext.i18nc("@info", "Close it there first, then try again. If KassenSturz isn't running there anymore, for example because that computer crashed, you can open the book here anyway.");
            return since ? since + " " + what : what;
        }
        case "book.missing":
            return i18nContext.i18nc("@info %1 is where the book was", "It was %1. It may have been moved or renamed, or it is on a drive that isn't connected. Connect the drive and try again, or open the book where it is now. Nothing was changed.", page.where);
        case "book.sandboxed":
            return i18nContext.i18nc("@info", "While a book is open, KassenSturz keeps a small file next to it, and this version of KassenSturz isn't allowed to make files in that folder. Choose a book in KassenSturz's own folder instead.");
        case "book.no-app-folder":
            return i18nContext.i18nc("@info", "Your system doesn't tell KassenSturz where apps keep their data. Open a book or start a new one in a folder you choose.");
        case "store.already-exists":
            return i18nContext.i18nc("@info %1 is a file name", "KassenSturz never replaces a file with a new book, so %1 wasn't changed. Choose another name or another folder.", page.where);
        case "store.not-a-book":
            return i18nContext.i18nc("@info %1 is a file name", "%1 isn't a KassenSturz book. It wasn't changed. Choose another file.", page.where);
        case "store.unsupported-layout":
            return i18nContext.i18nc("@info", "This book was made by a newer KassenSturz. Update KassenSturz to open it. The book wasn't changed.");
        case "store.development-layout":
            return i18nContext.i18nc("@info", "This book was made by a test version of KassenSturz from before 0.1. Open it with that version, choose \"Save the whole book\" there, and start a new book from the saved file. The book wasn't changed.");
        case "store.backup-failed":
            return i18nContext.i18nc("@info", "Before it updates a book to work with this version, KassenSturz saves a copy, and that didn't work, so the book was left as it was. Free up space on the drive, then try again.");
        case "store.journal-mode-refused":
            return i18nContext.i18nc("@info", "The book's file couldn't be set up safely, so KassenSturz didn't open it. Copy the book to a folder on this computer and open it from there.");
        case "store.io":
            switch (problem.kind) {
            case "permission-denied":
            case "read-only":
                return i18nContext.i18nc("@info", "KassenSturz isn't allowed to change files in this folder. Choose a folder you can write to.");
            case "storage-full":
            case "quota-exceeded":
                return i18nContext.i18nc("@info", "The drive is full. Free up some space, then try again.");
            default:
                return i18nContext.i18nc("@info", "The file couldn't be read or written. Check that the drive is connected, then try again.");
            }
        default:
            if (code.startsWith("store.corrupt")) {
                return i18nContext.i18nc("@info", "The book is damaged, so KassenSturz didn't open it. Your backups are in KassenSturz's backup folder; open the newest one instead.");
            }
            return i18nContext.i18nc("@info", "Something went wrong reading the book. Try again. If it keeps happening, open a backup from KassenSturz's backup folder.");
        }
    }

    title: i18nContext.i18nc("@title", "Opening a book")

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 30)
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Icon {
            Layout.alignment: Qt.AlignHCenter
            implicitWidth: Kirigami.Units.iconSizes.huge
            implicitHeight: Kirigami.Units.iconSizes.huge
            source: page.code === "store.locked" ? "object-locked" : "dialog-warning"
        }

        Kirigami.Heading {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: page.heading()
            Accessible.role: Accessible.Heading
        }

        Controls.Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            // The computer's name and the file's come from files and could
            // contain anything: never shown as markup (docs/ERROR_CODES.md).
            textFormat: Text.PlainText
            text: page.explanation()
        }

        Controls.Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            visible: page.code === "store.locked" && page.where !== ""
            opacity: 0.7
            text: page.where
        }

        Flow {
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: parent.width
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                visible: page.code === "store.locked"
                icon.name: "unlock"
                text: page.i18nContext.i18nc("@action:button", "Open anyway")
                onClicked: {
                    ActionLog.taken("BookProblemPage_openAnyway")
                    page.session.openAnyway()
                }
            }
            Controls.Button {
                visible: ["store.locked", "book.missing", "store.io", "store.backup-failed", "store.database"].includes(page.code)
                icon.name: "view-refresh"
                text: page.i18nContext.i18nc("@action:button", "Try again")
                onClicked: {
                    ActionLog.taken("BookProblemPage_tryAgain")
                    page.session.retry()
                }
            }
            Controls.Button {
                icon.name: "document-open"
                text: page.i18nContext.i18nc("@action:button", "Open another book…")
                onClicked: {
                    ActionLog.taken("BookProblemPage_openAnotherBook")
                    page.openAnotherClicked()
                }
            }
            Controls.Button {
                icon.name: "document-new"
                text: page.i18nContext.i18nc("@action:button", "Start a new book…")
                onClicked: {
                    ActionLog.taken("BookProblemPage_startNewBook")
                    page.newBookClicked()
                }
            }
            Controls.Button {
                icon.name: "document-import"
                text: page.i18nContext.i18nc("@action:button", "Start a book from a file…")
                onClicked: {
                    ActionLog.taken("BookProblemPage_startBookFile")
                    page.startFromFileClicked()
                }
            }
            Controls.Button {
                visible: page.session.open
                icon.name: "go-previous"
                text: page.i18nContext.i18nc("@action:button %1 is a file name", "Back to %1", page.session.fileName)
                onClicked: {
                    ActionLog.taken("BookProblemPage_back")
                    page.backClicked()
                }
            }
        }
    }
}
