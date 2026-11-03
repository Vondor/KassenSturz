// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Page 1 of the setup wizard (M7a.9, docs/SETUP_WIZARD.md): what
// KassenSturz does and which currency the household uses (D-111). Nothing is
// written here: the currency goes along to the page that creates the book,
// so leaving the wizard halfway leaves the book as it was.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property FirstStart firstStart

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
                    // A book that has categories already keeps its currency.
                    page.continued(page.firstStart.needsCurrency ? currencyBox.currentText : page.firstStart.currencies[0])
                }
            }
        }
    }
}
