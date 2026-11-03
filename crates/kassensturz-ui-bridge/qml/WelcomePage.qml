// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Page 1 of the setup wizard (M7a.9, docs/SETUP_WIZARD.md): what
// KassenSturz does, its language and which currency the household uses
// (D-111). The book isn't written here: the currency goes along to the page
// that creates the book, so leaving the wizard halfway leaves the book as it
// was. A new language is the existing setting (D-127); on Continue
// KassenSturz starts again in it, straight into this page (Q-395).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property FirstStart firstStart
    required property LanguageSettings language

    // The household's currency, as an ISO 4217 code.
    signal continued(string currency)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Welcome")
    header: MockNotice {}

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Icon {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.gridUnit * 2
            implicitWidth: Kirigami.Units.iconSizes.huge
            implicitHeight: Kirigami.Units.iconSizes.huge
            source: "wallet-open"
        }

        Kirigami.Heading {
            Layout.alignment: Qt.AlignHCenter
            text: page.i18nContext.i18nc("@title", "Welcome to KassenSturz")
        }

        Controls.Label {
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            horizontalAlignment: Text.AlignHCenter
            text: page.i18nContext.i18nc("@info", "KassenSturz shows you how much money you have and where it goes. Everything stays on this computer.")
            wrapMode: Text.Wrap
        }

        Kirigami.FormLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.largeSpacing

            Controls.ComboBox {
                id: languageBox

                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Language:")
                textRole: "text"
                valueRole: "value"
                // Each language is named in itself, so it is found whatever
                // language the app is in now.
                model: [
                    { value: "system", text: page.i18nContext.i18nc("@item:inlistbox", "As in your system") },
                    { value: "en", text: "English" },
                    { value: "de", text: "Deutsch" },
                ]
                Component.onCompleted: currentIndex = indexOfValue(page.language.choice)
                onActivated: {
                    ActionLog.taken("WelcomePage_language")
                    restartFailed.visible = false
                    languageNotSaved.visible = !page.language.choose(currentValue)
                }
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                visible: page.language.restartNeeded
                textFormat: Text.PlainText
                text: page.language.choice === "system"
                    ? page.i18nContext.i18nc("@info", "KassenSturz starts again in your system's language. Nothing is lost.")
                    : page.i18nContext.i18nc("@info %1 is a language, named in itself", "KassenSturz starts again in %1. Nothing is lost.", languageBox.currentText)
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }

            Controls.ComboBox {
                id: currencyBox
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Which currency do you use?")
                visible: page.firstStart.needsCurrency
                // The regional settings' currency comes first (D-111).
                model: page.firstStart.currencies
                currentIndex: 0
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                visible: page.firstStart.needsCurrency
                text: page.i18nContext.i18nc("@info", "Most people only need to confirm this. You can add an account in another currency later.")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Continue")
                icon.name: "go-next"
                highlighted: true
                onClicked: {
                    ActionLog.taken("WelcomePage_continue")
                    // In a new language KassenSturz starts again and comes
                    // back to this page; nothing has been typed yet.
                    if (page.language.restartNeeded) {
                        restartFailed.visible = !page.language.restart()
                        return
                    }
                    // A book that has categories already keeps its currency.
                    page.continued(page.firstStart.needsCurrency ? currencyBox.currentText : page.firstStart.currencies[0])
                }
            }
        }

        Kirigami.InlineMessage {
            id: languageNotSaved

            visible: false
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then choose again.")
        }

        Kirigami.InlineMessage {
            id: restartFailed

            visible: false
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            text: page.i18nContext.i18nc("@info", "KassenSturz couldn't start again by itself. Close it and open it again to see it in the new language; your choice is saved.")
        }
    }
}
