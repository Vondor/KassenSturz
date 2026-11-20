// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's settings, first level (D-795): one main action, saving a copy of
// the book (M3.12), then a short list of groups, each on a page of its own so
// that nobody has to read every setting at once (AGENTS.md §6, progressive
// disclosure): My book (SettingsBookPage.qml), Bank (SettingsBankPage.qml),
// Look and language (SettingsLookPage.qml), Backups and undo
// (SettingsBackupPage.qml) and Advanced (SettingsAdvancedPage.qml). The AI
// switch stays here, because it is the one AI element shown while AI is off
// (D-013). Help and the running version (D-152) close the page.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookSession session
    required property AiSwitch ai

    // Whether the AI switch couldn't be saved.
    property bool aiNotSaved: false

    signal saveCopyClicked()
    // The user opens a group of settings: "book", "bank", "look", "backups",
    // "plugins" or "advanced" (D-795).
    signal groupClicked(string group)
    // The user wants the help page on patterns and formulas.
    signal helpClicked()
    // Help on the guided setup's steps is wanted (#1031, D-902).
    signal guidedHelpClicked()
    // The user wants the testing guide for testers (#1030).
    signal testingGuideClicked()
    // The user wants the page of the provider and of what is sent; only
    // offered while the switch is on.
    signal aiSettingsClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Settings")

    // The main action: protecting the book comes first (AGENTS.md §1).
    FormCard.FormCard {
        Layout.topMargin: Kirigami.Units.gridUnit

        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                Layout.fillWidth: true
                highlighted: true
                icon.name: "document-save-as"
                text: page.i18nContext.i18nc("@action:button the main action of the settings", "Save a backup now…")
                onClicked: {
                    ActionLog.taken("SettingsPage_saveCopyData")
                    page.saveCopyClicked()
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
                text: page.i18nContext.i18nc("@info", "The whole book as a copy, for example on a USB stick, or your data for a spreadsheet.")
            }
        }
    }

    FormCard.FormCard {
        Layout.topMargin: Kirigami.Units.gridUnit

        FormCard.FormButtonDelegate {
            icon.name: "view-list-details"
            text: page.i18nContext.i18nc("@action:button a group of settings", "My book")
            description: page.i18nContext.i18nc("@info what a group of settings holds", "Which book is open, tags, an example book")
            onClicked: {
                ActionLog.taken("SettingsPage_myBook")
                page.groupClicked("book")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "network-connect"
            text: page.i18nContext.i18nc("@action:button a group of settings", "Bank")
            description: page.i18nContext.i18nc("@info what a group of settings holds", "Bank logins, reminder to get new transactions")
            onClicked: {
                ActionLog.taken("SettingsPage_bank")
                page.groupClicked("bank")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "color-management"
            text: page.i18nContext.i18nc("@action:button a group of settings", "Look and language")
            description: page.i18nContext.i18nc("@info what a group of settings holds", "Language, how amounts look, chart colors")
            onClicked: {
                ActionLog.taken("SettingsPage_lookLanguage")
                page.groupClicked("look")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-revert"
            text: page.i18nContext.i18nc("@action:button a group of settings", "Backups and undo")
            description: page.i18nContext.i18nc("@info what a group of settings holds", "Copies of your book, restoring one, the history")
            onClicked: {
                ActionLog.taken("SettingsPage_backupsUndo")
                page.groupClicked("backups")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "preferences-plugin"
            text: page.i18nContext.i18nc("@action:button a group of settings", "Plugins")
            description: page.i18nContext.i18nc("@info what a group of settings holds", "Add a plugin file you downloaded, see what it may do")
            onClicked: {
                ActionLog.taken("SettingsPage_plugins")
                page.groupClicked("plugins")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "configure"
            text: page.i18nContext.i18nc("@action:button a group of settings", "Advanced")
            description: page.i18nContext.i18nc("@info what a group of settings holds", "Where your book is, folder access, where PINs are saved")
            onClicked: {
                ActionLog.taken("SettingsPage_advanced")
                page.groupClicked("advanced")
            }
        }
    }

    // ai-toggle-begin: the one place AI shows while it is off (D-013). A
    // build without AI doesn't show it at all.
    FormCard.FormCard {
        visible: page.ai.available

        FormCard.FormSwitchDelegate {
            text: page.i18nContext.i18nc("@option:check", "AI features")
            description: page.i18nContext.i18nc("@info", "Off unless you turn it on. While it is off, nothing about AI is shown or runs.")
            checked: page.ai.enabled
            onToggled: {
                ActionLog.taken("SettingsPage_aiFeatures")
                page.aiNotSaved = !page.ai.switchTo(checked);
                checked = Qt.binding(() => page.ai.enabled);
            }
        }
    }

    FormCard.FormSectionText {
        visible: page.ai.available && page.aiNotSaved
        textFormat: Text.PlainText
        text: page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then try again.")
    }
    // ai-toggle-end

    AiSettingsEntry {
        ai: page.ai
        onOpenClicked: page.aiSettingsClicked()
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "About")
    }

    FormCard.FormCard {
        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button opens a help page", "Help: patterns, formulas and charts")
            description: page.i18nContext.i18nc("@info", "How to write a pattern in a Rule, a formula for a chart and the description of a custom chart")
            onClicked: {
                ActionLog.taken("SettingsPage_helpPatternsFormulas")
                page.helpClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button opens a help page", "Help: guided setup")
            description: page.i18nContext.i18nc("@info", "Every step of the guided setup again: get your transactions, check your balance")
            onClicked: {
                ActionLog.taken("SettingsPage_helpGuidedSetup")
                page.guidedHelpClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button opens the testing guide", "Testing KassenSturz")
            description: page.i18nContext.i18nc("@info", "For testers: check the install, try the first steps and write down a problem")
            onClicked: {
                ActionLog.taken("SettingsPage_testingGuide")
                page.testingGuideClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            descriptionItem.textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@label the version of KassenSturz that is running", "Version")
            // A bundle built in a pull request and reused on main shows the
            // pull request's last commit, not main's merge (D-427, #758).
            description: page.session.fromPullRequest
                ? page.i18nContext.i18nc("@info %1 is the version and its commit, e.g. 0.261112.15 (abc1234); the commit is the last one of a pull request", "%1, built from a pull request's last commit (on main: the merge of that commit)", page.session.version)
                : page.session.version
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button opens the project's web page in the browser", "Project page on GitHub")
            description: "https://github.com/Vondor/KassenSturz"
            descriptionItem.textFormat: Text.PlainText
            onClicked: {
                ActionLog.taken("SettingsPage_projectPageGithub")
                Qt.openUrlExternally("https://github.com/Vondor/KassenSturz")
            }
        }
    }
}
