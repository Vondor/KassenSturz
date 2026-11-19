// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The testing guide (#1030, D-792): a short wizard for someone who tests
// KassenSturz, so they don't need to read a manual first. Step 1 shows that
// the install worked, with the version and the kind of install; step 2
// points to an example book and the first things worth trying; step 3 helps
// to write down a problem as a note without real data, with the version and
// where it happened, which the tester copies and sends themselves.
//
// It is off for everyone else: nothing opens it by itself and it stores
// nothing. It is one entry in Settings, under About. The note never leaves
// the computer on its own (AGENTS.md §2): KassenSturz only puts it on the
// clipboard when the tester presses the button, and the tester sends it
// to whoever invited them, as the tester kit's report page says
// (docs/testing/REPORT_PAGE.en.md).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    // The running version (D-152) and whether it was built from a pull
    // request, as the settings show them.
    required property string version
    property bool fromPullRequest: false
    // How the app was installed: InstallKind.kind.
    property string installKind: "other"
    // Whether developer mode writes its log (D-227).
    property bool developerOn: false

    // The step on show: 0, 1 or 2.
    property int step: 0
    readonly property int steps: 3
    // Whether the note was just put on the clipboard.
    property bool copied: false

    signal exampleBookClicked()
    signal showLogClicked()
    signal doneClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Testing KassenSturz")

    // The kind of install in words.
    function installWords(kind: string): string {
        switch (kind) {
        case "flatpak":
            return i18nContext.i18nc("@info how KassenSturz was installed", "Flatpak");
        case "snap":
            return i18nContext.i18nc("@info how KassenSturz was installed", "Snap");
        case "windows":
            return i18nContext.i18nc("@info how KassenSturz was installed", "Windows");
        case "macos":
            return i18nContext.i18nc("@info how KassenSturz was installed", "macOS");
        default:
            return i18nContext.i18nc("@info how KassenSturz was installed", "Another way, for example a package of your Linux distribution");
        }
    }

    function versionWords(): string {
        return fromPullRequest
            ? i18nContext.i18nc("@info %1 is the version and its commit, e.g. 0.261112.15 (abc1234); the commit is the last one of a pull request", "%1, built from a pull request's last commit (on main: the merge of that commit)", version)
            : version;
    }

    // The note about a problem, as plain text in the language of the app.
    function noteText(): string {
        const lines = [
            i18nContext.i18nc("@info the first line of a tester's note", "KassenSturz: a problem while testing"),
            i18nContext.i18nc("@info a line of a tester's note, %1 is what they typed", "Title: %1", titleField.text.trim()),
            i18nContext.i18nc("@info a line of a tester's note, %1 is a part of the app", "Where: %1", whereChoice.currentText),
            i18nContext.i18nc("@info a line of a tester's note, %1 is what they typed", "What I wanted to do: %1", wantedArea.text.trim()),
            i18nContext.i18nc("@info a line of a tester's note, %1 is what they typed", "What I expected: %1", expectedArea.text.trim()),
            i18nContext.i18nc("@info a line of a tester's note, %1 is what they typed", "What happened: %1", happenedArea.text.trim()),
            i18nContext.i18nc("@info a line of a tester's note, %1 is always, sometimes or once", "How often: %1", oftenChoice.currentText),
            i18nContext.i18nc("@info a line of a tester's note, %1 is how much it gets in the way", "How bad: %1", badChoice.currentText),
            realDataCheck.checked
                ? i18nContext.i18nc("@info a line of a tester's note", "Real data in this note: yes")
                : i18nContext.i18nc("@info a line of a tester's note", "Real data in this note: no"),
            i18nContext.i18nc("@info a line of a tester's note, %1 is the version", "Version: %1", versionWords()),
            i18nContext.i18nc("@info a line of a tester's note, %1 is how KassenSturz was installed", "Installed as: %1", installWords(installKind)),
            developerOn
                ? i18nContext.i18nc("@info a line of a tester's note", "Developer mode: on, a log was written")
                : i18nContext.i18nc("@info a line of a tester's note", "Developer mode: off, no log"),
        ];
        return lines.join("\n");
    }

    function goTo(next: int): void {
        step = Math.max(0, Math.min(steps - 1, next));
        copied = false;
    }

    Controls.Label {
        Layout.fillWidth: true
        Layout.topMargin: Kirigami.Units.gridUnit
        Layout.leftMargin: Kirigami.Units.largeSpacing
        Layout.rightMargin: Kirigami.Units.largeSpacing
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Kirigami.Theme.disabledTextColor
        text: page.i18nContext.i18nc("@info %1 is the step on show, %2 how many there are", "Step %1 of %2", page.step + 1, page.steps)
    }

    // Step 1: the install worked.
    ColumnLayout {
        objectName: "installStep"
        Layout.fillWidth: true
        visible: page.step === 0
        spacing: 0

        FormCard.FormHeader {
            title: page.i18nContext.i18nc("@title:group", "Is KassenSturz installed correctly?")
        }

        FormCard.FormSectionText {
            text: page.i18nContext.i18nc("@info", "If you can read this, KassenSturz has started. Compare the version below with the release you downloaded: they should be the same.")
        }

        FormCard.FormCard {
            FormCard.FormTextDelegate {
                objectName: "versionRow"
                text: page.i18nContext.i18nc("@label the version of KassenSturz that is running", "Version")
                description: page.versionWords()
                descriptionItem.textFormat: Text.PlainText
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormTextDelegate {
                objectName: "installRow"
                text: page.i18nContext.i18nc("@label how KassenSturz was installed", "Installed as")
                description: page.installWords(page.installKind)
                descriptionItem.textFormat: Text.PlainText
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormTextDelegate {
                text: page.i18nContext.i18nc("@label", "Developer mode")
                description: page.developerOn
                    ? page.i18nContext.i18nc("@info", "On: KassenSturz writes a log for the test.")
                    : page.i18nContext.i18nc("@info", "Off. If your install sheet asks for it, close KassenSturz and start it in developer mode, the way the sheet shows.")
                descriptionItem.textFormat: Text.PlainText
            }

            FormCard.FormDelegateSeparator {
                visible: page.developerOn
            }

            FormCard.FormButtonDelegate {
                objectName: "showLogButton"
                visible: page.developerOn
                icon.name: "folder-open"
                text: page.i18nContext.i18nc("@action:button opens the folder of the developer mode's log", "Show the log")
                onClicked: {
                    ActionLog.taken("TestingGuidePage_showLog")
                    page.showLogClicked()
                }
            }
        }
    }

    // Step 2: the example book and the first things to try.
    ColumnLayout {
        objectName: "tryStep"
        Layout.fillWidth: true
        visible: page.step === 1
        spacing: 0

        FormCard.FormHeader {
            title: page.i18nContext.i18nc("@title:group", "The first things to try")
        }

        FormCard.FormSectionText {
            text: page.i18nContext.i18nc("@info", "Use an example book: its people, accounts and amounts are made up, so nothing of yours is in it. Please don't type in anything from your own bank accounts.")
        }

        FormCard.FormCard {
            FormCard.FormButtonDelegate {
                objectName: "exampleBookButton"
                icon.name: "document-open"
                text: page.i18nContext.i18nc("@action:button", "Open an example book…")
                onClicked: {
                    ActionLog.taken("TestingGuidePage_exampleBook")
                    page.exampleBookClicked()
                }
            }
        }

        FormCard.FormCard {
            Layout.topMargin: Kirigami.Units.largeSpacing

            FormCard.FormTextDelegate {
                text: page.i18nContext.i18nc("@info a thing to try while testing", "1. Look at the start screen: what do you have, and where does the money go?")
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormTextDelegate {
                text: page.i18nContext.i18nc("@info a thing to try while testing", "2. Add an account with a made-up balance.")
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormTextDelegate {
                text: page.i18nContext.i18nc("@info a thing to try while testing", "3. Enter a transaction, then take it back with Undo.")
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormTextDelegate {
                text: page.i18nContext.i18nc("@info a thing to try while testing", "4. Open an account and find one of its transactions.")
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormTextDelegate {
                text: page.i18nContext.i18nc("@info a thing to try while testing", "5. In Settings, change the language or how amounts look.")
            }
        }

        FormCard.FormSectionText {
            text: page.i18nContext.i18nc("@info", "Something unclear, slow or wrong? That is exactly what we are looking for. The next step helps you write it down.")
        }
    }

    // Step 3: a note about a problem.
    ColumnLayout {
        objectName: "noteStep"
        Layout.fillWidth: true
        visible: page.step === 2
        spacing: 0

        FormCard.FormHeader {
            title: page.i18nContext.i18nc("@title:group", "Write down a problem")
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            visible: true
            type: Kirigami.MessageType.Warning
            text: page.i18nContext.i18nc("@info", "Write only about made-up data: no names, amounts, account numbers or IBANs from your own book. The note stays on this computer until you copy it and send it yourself.")
        }

        FormCard.FormCard {
            Layout.topMargin: Kirigami.Units.largeSpacing

            // A label and a field of its own, not FormTextFieldDelegate, which
            // asks for the engine-wide KI18n context that the app doesn't use
            // (D-074, as in ExportPage.qml).
            ColumnLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: page.i18nContext.i18nc("@label a short name for the problem", "Title")
                }

                Controls.TextField {
                    id: titleField

                    objectName: "titleField"
                    Layout.fillWidth: true
                    Accessible.name: page.i18nContext.i18nc("@label a short name for the problem", "Title")
                    onTextEdited: page.copied = false
                }
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormComboBoxDelegate {
                id: whereChoice

                objectName: "whereChoice"
                text: page.i18nContext.i18nc("@label:listbox", "Where in KassenSturz")
                model: [
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Installing or starting"),
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Start screen"),
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Accounts"),
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Transactions and transfers"),
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Importing a file"),
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Charts and reports"),
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Budgets and plans"),
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Settings"),
                    page.i18nContext.i18nc("@item:inlistbox a part of the app", "Somewhere else"),
                ]
            }

            FormCard.FormDelegateSeparator {}

            ColumnLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: page.i18nContext.i18nc("@label", "What did you want to do?")
                }

                Controls.TextArea {
                    id: wantedArea

                    objectName: "wantedArea"
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 3
                    wrapMode: TextEdit.Wrap
                    textFormat: TextEdit.PlainText
                    Accessible.name: page.i18nContext.i18nc("@label", "What did you want to do?")
                    onTextChanged: page.copied = false
                }

                Controls.Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: page.i18nContext.i18nc("@label", "What did you expect?")
                }

                Controls.TextArea {
                    id: expectedArea

                    objectName: "expectedArea"
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 3
                    wrapMode: TextEdit.Wrap
                    textFormat: TextEdit.PlainText
                    Accessible.name: page.i18nContext.i18nc("@label", "What did you expect?")
                    onTextChanged: page.copied = false
                }

                Controls.Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: page.i18nContext.i18nc("@label", "What happened? Copy every message word for word.")
                }

                Controls.TextArea {
                    id: happenedArea

                    objectName: "happenedArea"
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 4
                    wrapMode: TextEdit.Wrap
                    textFormat: TextEdit.PlainText
                    Accessible.name: page.i18nContext.i18nc("@label", "What happened? Copy every message word for word.")
                    onTextChanged: page.copied = false
                }
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormComboBoxDelegate {
                id: oftenChoice

                objectName: "oftenChoice"
                text: page.i18nContext.i18nc("@label:listbox", "Does it happen again?")
                model: [
                    page.i18nContext.i18nc("@item:inlistbox how often a problem happens", "Always"),
                    page.i18nContext.i18nc("@item:inlistbox how often a problem happens", "Sometimes"),
                    page.i18nContext.i18nc("@item:inlistbox how often a problem happens", "Only once"),
                ]
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormComboBoxDelegate {
                id: badChoice

                objectName: "badChoice"
                text: page.i18nContext.i18nc("@label:listbox", "How bad is it?")
                model: [
                    page.i18nContext.i18nc("@item:inlistbox how much a problem gets in the way", "It stops me"),
                    page.i18nContext.i18nc("@item:inlistbox how much a problem gets in the way", "It is annoying"),
                    page.i18nContext.i18nc("@item:inlistbox how much a problem gets in the way", "A small thing"),
                ]
            }

            FormCard.FormDelegateSeparator {}

            FormCard.FormCheckDelegate {
                id: realDataCheck

                objectName: "realDataCheck"
                text: page.i18nContext.i18nc("@option:check", "This note has data from my own book in it")
                onToggled: {
                    ActionLog.taken("TestingGuidePage_realData")
                    page.copied = false
                }
            }
        }

        FormCard.FormHeader {
            title: page.i18nContext.i18nc("@title:group", "Your note")
        }

        FormCard.FormCard {
            ColumnLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                spacing: Kirigami.Units.smallSpacing

                Controls.TextArea {
                    id: notePreview

                    objectName: "notePreview"
                    Layout.fillWidth: true
                    readOnly: true
                    wrapMode: TextEdit.Wrap
                    textFormat: TextEdit.PlainText
                    Accessible.name: page.i18nContext.i18nc("@label", "Your note, as it is copied")
                    text: page.noteText()
                }

                Controls.Button {
                    objectName: "copyButton"
                    icon.name: "edit-copy"
                    text: page.i18nContext.i18nc("@action:button", "Copy the note")
                    onClicked: {
                        ActionLog.taken("TestingGuidePage_copyNote")
                        notePreview.selectAll();
                        notePreview.copy();
                        notePreview.deselect();
                        page.copied = true;
                    }
                }

                Controls.Label {
                    Layout.fillWidth: true
                    visible: page.copied
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: page.i18nContext.i18nc("@info", "The note is on the clipboard. Paste it into your e-mail or message.")
                }
            }
        }

        FormCard.FormSectionText {
            text: page.i18nContext.i18nc("@info", "Send the note only to the person who invited you to test, the way they told you, for example by e-mail. If developer mode is on, they may ask you for the log as well. If your note has data from your own book in it, it stays with that person only and is deleted after the round.")
        }
    }

    // Back and next, the same on every step, so the keyboard finds them in
    // the same place.
    RowLayout {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.smallSpacing

        Controls.Button {
            objectName: "backButton"
            enabled: page.step > 0
            icon.name: "go-previous"
            text: page.i18nContext.i18nc("@action:button", "Back")
            onClicked: {
                ActionLog.taken("TestingGuidePage_back")
                page.goTo(page.step - 1)
            }
        }

        Item {
            Layout.fillWidth: true
        }

        Controls.Button {
            objectName: "nextButton"
            visible: page.step < page.steps - 1
            highlighted: true
            icon.name: "go-next"
            text: page.i18nContext.i18nc("@action:button", "Next")
            onClicked: {
                ActionLog.taken("TestingGuidePage_next")
                page.goTo(page.step + 1)
            }
        }

        Controls.Button {
            objectName: "doneButton"
            visible: page.step === page.steps - 1
            highlighted: true
            icon.name: "dialog-ok"
            text: page.i18nContext.i18nc("@action:button", "Done")
            onClicked: {
                ActionLog.taken("TestingGuidePage_done")
                page.doneClicked()
            }
        }
    }
}
