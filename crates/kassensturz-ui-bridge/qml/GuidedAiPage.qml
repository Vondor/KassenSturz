// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Step 7 of the guided setup (#1031, docs/GUIDED_SETUP.md): does the person
// have an AI on this computer? Besides the toggle in Settings, this page and
// its entry in the guided setup's Help are the only places where AI shows
// while it is off (D-911, D-923, AGENTS.md §8), and only in a build that has
// AI (D-039): Main.qml leaves the step out otherwise.
//
// "No, or I don't know" is what holds until the person says otherwise, and
// Skip, "End the setup" and "Continue" change nothing. "Yes" turns on the
// one AI switch (D-013) and opens the existing provider page, where the
// address is typed and tested; nothing else changes: every permission stays
// off, and an address on another computer still needs the consent with its
// statement of what is sent (D-035).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AiSwitch ai
    // This page's step and how many there are; 0 when opened from Help.
    property int step: 0
    property int count: 0

    // The switch is on and the person wants to set up their AI: the provider
    // page.
    signal providerClicked()
    signal nextClicked()
    signal skipClicked()
    signal endClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The switch couldn't be saved.
    property bool notSaved: false

    title: i18nContext.i18nc("@title a step of the guided setup", "Do you have an AI on this computer?")
    header: GuidedBar {
        step: page.step
        count: page.count
        onEndClicked: page.endClicked()
    }
    footer: GuidedFooter {
        nextText: Plain.plain(page.step > 0
            ? page.i18nContext.i18nc("@action:button", "Continue")
            : page.i18nContext.i18nc("@action:button closes a step opened from Help", "Done"))
        skipVisible: page.step > 0
        onSkipClicked: page.skipClicked()
        onNextClicked: page.nextClicked()
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.gridUnit
            Layout.maximumWidth: Kirigami.Units.gridUnit * 36
            Layout.preferredWidth: Kirigami.Units.gridUnit * 36
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@info", "KassenSturz can let an AI answer questions about your book, one that runs on your own computer, for example with Ollama or LM Studio. You don't need this: KassenSturz is complete without AI.")
                wrapMode: Text.Wrap
            }

            GuidedChoice {
                iconName: "dialog-cancel"
                heading: page.i18nContext.i18nc("@action:button answers whether there is an AI on this computer", "No, or I don't know")
                selected: !page.ai.enabled
                badge: Plain.plain(page.ai.enabled ? "" : page.i18nContext.i18nc("@info a few words next to the choice that holds now", "This is how it stays if you do nothing"))
                line: page.i18nContext.i18nc("@info", "The AI features are off, and nothing about AI is shown or runs. You can turn them on later in Settings.")
                onClicked: {
                    ActionLog.taken("GuidedAiPage_no")
                    page.notSaved = page.ai.enabled && !page.ai.switchTo(false);
                }
            }

            GuidedChoice {
                iconName: "computer"
                heading: page.i18nContext.i18nc("@action:button answers whether there is an AI on this computer", "Yes, I have an AI on this computer")
                selected: page.ai.enabled
                badge: Plain.plain(page.ai.enabled ? page.i18nContext.i18nc("@info a few words next to the choice that holds now", "Turned on") : "")
                line: page.i18nContext.i18nc("@info", "Turns on the AI features and opens the page where you type in the address of your AI and test it. Every permission starts switched off: the AI reads and changes nothing in your book until you allow it, one thing at a time.")
                onClicked: {
                    ActionLog.taken("GuidedAiPage_yes")
                    page.notSaved = !page.ai.enabled && !page.ai.switchTo(true);
                    if (page.ai.enabled) {
                        page.providerClicked();
                    }
                }
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: page.notSaved
                type: Kirigami.MessageType.Error
                text: page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then try again.")
            }

            Controls.Label {
                Layout.fillWidth: true
                textFormat: Text.PlainText
                opacity: 0.8
                text: page.i18nContext.i18nc("@info", "As long as the address is on this computer, nothing leaves your computer. If it is somewhere else, KassenSturz first shows exactly what would be sent and asks whether you agree.")
                wrapMode: Text.Wrap
            }
        }
    }
}
