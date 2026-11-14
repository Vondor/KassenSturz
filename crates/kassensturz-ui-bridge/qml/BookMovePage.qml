// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Keeping the book in another folder (D-731, docs/BOOK_FOLDER.md §2): the
// system's folder chooser, then this page with the folder, the book's name,
// its size and what will happen, then "Move my book", a short progress
// line and the done page with "Move it back". Only moving is offered, never
// a second working copy. Folders are shown in the user's words, as plain
// text only (D-730); every text comes from Rust, the sentences are written
// here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookMove bookMove
    // Told when the book moved, so the window shows it where it is now.
    signal moved()

    // "choose", "ready" (a folder is chosen), "moving", "done".
    property string stage: "choose"
    // The folder the chooser granted: `path` (what the app opens) and
    // `shownAs`.
    property var grant: ({})
    // What `check` answered for the chosen folder and name.
    property var checked: ({})
    // What the move answered.
    property var result: ({})
    // The last problem, as a sentence.
    property string problem: ""
    // The move to run once the progress line is shown: "to", "back", "own".
    property string pending: ""

    readonly property var place: bookMove.place ?? ({})

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // A folder as the user knows it.
    function folderText(shown: var): string {
        if (!shown)
            return "";
        return shown.own
            ? i18nContext.i18nc("@info the folder KassenSturz keeps its own book in", "KassenSturz's own folder")
            : (shown.text ?? "");
    }

    // What a code from BookMove means, and what to do next.
    function reasonFor(code: string): string {
        switch (code) {
        case "book-move.bad-name":
            return i18nContext.i18nc("@info", "Please give the book a name without / \\ : * ? \" < > | that doesn't start or end with a dot.");
        case "book-move.no-folder":
            return i18nContext.i18nc("@info", "This folder isn't there any more. Please choose another one.");
        case "book-move.same-folder":
            return i18nContext.i18nc("@info", "Your book is in this folder already. Please choose another one.");
        case "book-move.already-exists":
            return i18nContext.i18nc("@info", "There is already a book or a file with this name in that folder. KassenSturz never replaces a file: choose another name or another folder.");
        case "book-move.leftover-files":
            return i18nContext.i18nc("@info", "That folder holds files that a book with this name left behind. KassenSturz doesn't put your book next to them, so they can't be mixed up. Choose another name or another folder.");
        case "book-move.in-progress":
            return i18nContext.i18nc("@info", "An earlier move of your book hasn't ended yet. Close KassenSturz, open it again and answer its question, then try once more.");
        case "book-move.read-only":
            return i18nContext.i18nc("@info", "KassenSturz can't write to this folder. Choose a folder you may write to.");
        case "book-move.no-space":
            return i18nContext.i18nc("@info", "There isn't enough free space there: moving needs at least twice your book's size. Free some space or choose another drive.");
        case "book-move.sandboxed":
            return i18nContext.i18nc("@info", "A book can't be kept in this place. Please choose a folder.");
        case "book-move.outside-test":
            return i18nContext.i18nc("@info", "In the test profile, books stay inside its test folder. Choose a folder there.");
        case "book-move.no-way-back":
            return i18nContext.i18nc("@info", "KassenSturz doesn't know where your book was before. Choose the folder instead.");
        case "book-move.failed-checks":
            return i18nContext.i18nc("@info", "Your book wasn't moved: something in the folder changed since you chose it. Choose it again.");
        case "book-move.failed-lock":
            return i18nContext.i18nc("@info", "Your book wasn't moved: KassenSturz couldn't save its settings, or the book was busy. Try again in a moment.");
        case "book-move.failed-backup":
            return i18nContext.i18nc("@info", "Your book wasn't moved: the copy before moving couldn't be kept in the backup folder. Free some space on this computer and try again.");
        case "book-move.failed-copy":
            return i18nContext.i18nc("@info", "Your book wasn't moved: it couldn't be copied into the folder. Free some space there, or choose another folder.");
        case "book-move.failed-check":
            return i18nContext.i18nc("@info", "Your book wasn't moved: the copy in the new folder wasn't exactly the same as your book, so it was removed. The drive may have a problem; choose another folder.");
        case "book-move.failed-place":
            return i18nContext.i18nc("@info", "Your book wasn't moved: the copy couldn't be given its name in the new folder without the risk of replacing a file. Choose another folder.");
        case "book-move.failed-switch":
            return i18nContext.i18nc("@info", "Your book wasn't moved: the book in the new folder couldn't be opened or remembered, so it was removed again. Try again, or choose another folder.");
        default:
            return i18nContext.i18nc("@info", "Your book wasn't moved. Try again, or choose another folder.");
        }
    }

    function showProblem(answer: var) {
        problem = reasonFor(answer.code ?? "");
        if (answer.left) {
            problem += " " + i18nContext.i18nc("@info", "A file of the unfinished move couldn't be removed from the new folder; you may remove it yourself. Your book is open where it was.");
        }
    }

    function check() {
        checked = bookMove.check(grant.path ?? "", nameField.text);
        problem = checked.ok ? "" : reasonFor(checked.code ?? "");
    }

    // Shows the progress line first, then moves.
    function start(which: string) {
        pending = which;
        problem = "";
        stage = "moving";
        Qt.callLater(page.runPending);
    }

    function runPending() {
        let answer;
        if (pending === "back") {
            answer = bookMove.moveBack();
        } else if (pending === "own") {
            answer = bookMove.moveToOwnFolder();
        } else {
            answer = bookMove.moveTo(grant.path ?? "", nameField.text);
        }
        pending = "";
        if (answer.ok) {
            result = answer;
            stage = "done";
            page.moved();
        } else {
            stage = grant.path ? "ready" : "choose";
            showProblem(answer);
        }
    }

    title: i18nContext.i18nc("@title", "Where your book is kept")

    Component.onCompleted: bookMove.refresh()

    FolderChooser {
        id: chooser

        forBook: true
        title: page.i18nContext.i18nc("@title:window", "Choose the folder for your book")
        onGranted: chosen => {
            page.grant = chosen;
            page.stage = "ready";
            if (nameField.text === "")
                nameField.text = page.place.name ?? "";
            page.check();
        }
        onRefused: (code, reason) => {
            page.problem = reason;
        }
    }

    // Why the last attempt didn't work, and what to do next. The sentences
    // are fixed; nothing from a folder or a file is in them.
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.largeSpacing
        type: Kirigami.MessageType.Warning
        visible: page.problem !== "" && page.stage !== "moving"
        text: page.problem
    }

    // Where the book is now, and the folder chooser.
    FormCard.FormCard {
        visible: page.stage === "choose"

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Your book is now in")
            description: page.folderText(page.place.folder)
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "folder-open"
            text: page.i18nContext.i18nc("@action:button", "Choose a folder…")
            description: page.i18nContext.i18nc("@info", "For example a sync folder or a folder on another drive.")
            enabled: chooser.available
            onClicked: {
                ActionLog.taken("BookMovePage_chooseFolder")
                chooser.open()
            }
        }

        FormCard.FormDelegateSeparator {
            visible: !(page.place.folder?.own ?? true)
        }

        FormCard.FormButtonDelegate {
            visible: !(page.place.folder?.own ?? true)
            icon.name: "go-home"
            text: page.i18nContext.i18nc("@action:button", "Keep it in KassenSturz's own folder")
            description: page.i18nContext.i18nc("@info", "Where KassenSturz keeps a book unless you choose another folder.")
            onClicked: {
                ActionLog.taken("BookMovePage_ownFolder")
                page.start("own")
            }
        }
    }

    FormCard.FormSectionText {
        visible: page.stage === "choose" && !chooser.available
        text: page.i18nContext.i18nc("@info", "This computer's system doesn't offer choosing a folder for KassenSturz, so your book stays in KassenSturz's own folder, where it keeps working.")
    }

    // The chosen folder, the name and what will happen.
    FormCard.FormCard {
        visible: page.stage === "ready"

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Folder")
            description: page.folderText(page.checked.shownAs ?? ({ own: false, text: page.grant.shownAs ?? "" }))
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextFieldDelegate {
            id: nameField

            label: page.i18nContext.i18nc("@label the book's file name, without its extension", "Name of the book")
            onTextEdited: page.check()
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            visible: (page.checked.size ?? "") !== ""
            text: page.i18nContext.i18nc("@label", "Size")
            description: page.checked.size ?? ""
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        visible: page.stage === "ready"
        text: page.i18nContext.i18nc("@info", "KassenSturz moves your book there. Before that, it keeps a copy of your book as it is now with its backups on this computer, so you can always go back. If the folder is on a network drive or in a sync folder, close the book on your other computers first.")
    }

    FormCard.FormCard {
        visible: page.stage === "ready"

        FormCard.FormButtonDelegate {
            icon.name: "go-next"
            text: page.i18nContext.i18nc("@action:button", "Move my book")
            enabled: page.checked.ok ?? false
            onClicked: {
                ActionLog.taken("BookMovePage_move")
                page.start("to")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "folder-open"
            text: page.i18nContext.i18nc("@action:button", "Choose another folder…")
            onClicked: {
                ActionLog.taken("BookMovePage_chooseAgain")
                chooser.open()
            }
        }
    }

    // The progress line. The move runs at once; nothing else can change
    // the book meanwhile.
    FormCard.FormCard {
        visible: page.stage === "moving"

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.largeSpacing

            Controls.BusyIndicator {
                running: page.stage === "moving"
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: page.i18nContext.i18nc("@info", "Moving your book…")
            }
        }
    }

    // The done page.
    FormCard.FormCard {
        visible: page.stage === "done"

        FormCard.FormTextDelegate {
            icon.name: "dialog-ok"
            text: page.i18nContext.i18nc("@info", "Your book is now in")
            description: page.folderText(page.result.shownAs)
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "The copy from before moving")
            description: page.i18nContext.i18nc("@info %1 is a backup's file name", "In the backup folder on this computer: %1", page.result.backupName ?? "")
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {
            visible: page.result.oldLeft ?? false
        }

        FormCard.FormTextDelegate {
            visible: page.result.oldLeft ?? false
            text: page.i18nContext.i18nc("@label", "The old file")
            description: page.i18nContext.i18nc("@info %1 is a folder", "It couldn't be removed and is still in %1. Your book is the one in the new folder; you may remove the old file yourself.", page.folderText(page.result.oldLeftShownAs))
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        visible: page.stage === "done"
        text: page.i18nContext.i18nc("@info", "Moving isn't on the Undo list, because it doesn't change what's in your book. To go back, use \"Move it back\", or open the copy from before moving in the backup folder.")
    }

    FormCard.FormCard {
        visible: page.stage === "done"

        FormCard.FormButtonDelegate {
            icon.name: "edit-undo"
            text: page.i18nContext.i18nc("@action:button", "Move it back")
            onClicked: {
                ActionLog.taken("BookMovePage_moveBack")
                page.grant = ({});
                page.start("back")
            }
        }
    }
}
