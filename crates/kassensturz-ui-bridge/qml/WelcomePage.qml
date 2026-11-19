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

    // The books already in KassenSturz's own folder when there was none to
    // open (B-404, D-872), as `BookSession.foundBooks` gives them: then they
    // are offered first, and nothing is made until the user chooses.
    property var found: ({ names: [] })
    readonly property bool offersFound: (found.names ?? []).length > 0

    // The household's currency, as an ISO 4217 code.
    signal continued(string currency)
    // Open the found book called `name`.
    signal openFound(string name)
    // Start with a new book of the app's own instead of a found one.
    signal startNew()

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

        // The books found in KassenSturz's own folder, at the top (D-872).
        ColumnLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            Layout.topMargin: Kirigami.Units.largeSpacing
            visible: page.offersFound
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                textFormat: Text.PlainText
                text: page.i18nContext.i18ncp("@info %2 is a folder", "KassenSturz found a book from before in its folder, %2:", "KassenSturz found %1 books from before in its folder, %2:", (page.found.names ?? []).length, page.found.shown ?? "")
                wrapMode: Text.Wrap
            }

            Repeater {
                model: page.found.names ?? []

                delegate: RowLayout {
                    required property string modelData

                    Layout.fillWidth: true

                    Controls.Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        text: modelData.replace(/\.ksz$/i, "")
                        elide: Text.ElideMiddle
                        font.bold: true
                    }

                    Controls.Button {
                        text: page.i18nContext.i18nc("@action:button", "Open this book")
                        icon.name: "document-open"
                        highlighted: true
                        onClicked: {
                            ActionLog.taken("WelcomePage_openFound")
                            page.openFound(modelData)
                        }
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                text: page.i18nContext.i18nc("@info", "Nothing is made or changed until you choose.")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }

            Controls.Button {
                Layout.alignment: Qt.AlignLeft
                flat: true
                icon.name: "document-new"
                text: page.i18nContext.i18nc("@action:button", "Start a new book instead")
                onClicked: {
                    ActionLog.taken("WelcomePage_startNew")
                    page.startNew()
                }
            }
        }

        // One page in plain words on why the data is safe (D-862), for
        // people who trust a written explanation more than a program. It is
        // part of the handbook and opens in the web browser; the address is
        // translated, so each language opens its own page (English where
        // there is none yet).
        Controls.Button {
            Layout.alignment: Qt.AlignHCenter
            flat: true
            icon.name: "security-high"
            text: page.i18nContext.i18nc("@action:button opens a page of the handbook in the web browser", "Why your data is safe")
            Controls.ToolTip.text: page.i18nContext.i18nc("@info:tooltip", "Opens a page of the handbook in your web browser")
            Controls.ToolTip.visible: hovered
            Accessible.description: Controls.ToolTip.text
            onClicked: {
                ActionLog.taken("WelcomePage_whyYourDataIsSafe")
                Qt.openUrlExternally(page.i18nContext.i18nc("@info the web address of the handbook page \"Why your data is safe\"; a language with its own page puts its address here", "https://github.com/Vondor/KassenSturz/blob/main/docs/user/en/why-your-data-is-safe.md"))
            }
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
                    { value: "pl", text: "Polski" },
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
                visible: page.firstStart.needsCurrency && !page.offersFound
                // The regional settings' currency comes first (D-111).
                model: page.firstStart.currencies
                currentIndex: 0
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                visible: page.firstStart.needsCurrency && !page.offersFound
                text: page.i18nContext.i18nc("@info", "Most people only need to confirm this. You can add an account in another currency later.")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }

            Controls.Button {
                // With books found, the choice above comes first.
                visible: !page.offersFound
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
