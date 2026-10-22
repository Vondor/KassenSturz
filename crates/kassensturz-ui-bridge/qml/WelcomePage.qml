// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The first thing a new user sees: what KassenSturz does, which currency the
// household uses (D-111), and one button to start (M3.4, D-116). Starting
// writes the default categories in the user's language (D-029, D-067) and
// leads to the first account.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property FirstStart firstStart

    signal addAccountClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    DefaultCategories {
        id: defaults
    }

    title: i18nContext.i18nc("@title", "Welcome")
    header: ColumnLayout {
        spacing: 0

        MockNotice {
            Layout.fillWidth: true
        }

        FormError {
            id: error
            Layout.fillWidth: true
        }
    }

    function start() {
        if (firstStart.needsCurrency) {
            const result = firstStart.start(currencyBox.currentText, defaults.incomeNames,
                defaults.incomeParents, defaults.spendingNames, defaults.spendingParents);
            error.result = result;
            if (result.code !== undefined) {
                return;
            }
        }
        addAccountClicked();
    }

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
            text: page.i18nContext.i18nc("@info", "KassenSturz shows you how much money you have and where it goes. Start with one account, such as your checking account or the cash in your wallet. Everything stays on this computer.")
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
                text: page.i18nContext.i18nc("@action:button", "Add your first account")
                icon.name: "list-add"
                onClicked: page.start()
            }
        }
    }
}
