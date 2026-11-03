// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Page 4 of the setup wizard (M7a.9, docs/SETUP_WIZARD.md): how new entries
// get in. Shown once, right after page 3b created the book, so it only offers
// what comes next: typing them, importing a file from the bank (the existing
// import, M3.10), or, in a build that offers it (Q-398) and where bank
// connections work, the existing bank setup (M4.6). Nothing is written here.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountList accounts
    // Whether the bank connection is offered: the build allows it and bank
    // connections work here.
    required property bool bankOffered

    signal typeMyselfClicked()
    signal importClicked(string accountId)
    signal connectBankClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The accounts the wizard just made, one list.
    readonly property var ownAccounts: (page.accounts.view.sections ?? []).reduce((all, section) => all.concat(section.accounts ?? []), [])
    // The accounts are listed once the user wants a file imported and has
    // more than one.
    property bool choosingAccount: false

    title: i18nContext.i18nc("@title", "How do new entries get in?")
    header: MockNotice {}

    component Choice: Controls.AbstractButton {
        id: choice

        property string iconName
        property string heading
        property string line

        Layout.fillWidth: true
        Accessible.name: heading
        Accessible.description: line
        implicitHeight: content.implicitHeight + Kirigami.Units.largeSpacing * 2
        background: Rectangle {
            radius: Kirigami.Units.cornerRadius
            color: choice.hovered || choice.visualFocus ? Qt.alpha(Kirigami.Theme.highlightColor, 0.12) : Kirigami.Theme.backgroundColor
            border.color: choice.visualFocus ? Kirigami.Theme.highlightColor : Qt.alpha(Kirigami.Theme.textColor, 0.2)
            border.width: choice.visualFocus ? 2 : 1
        }
        contentItem: RowLayout {
            id: content

            spacing: Kirigami.Units.largeSpacing

            Kirigami.Icon {
                Layout.leftMargin: Kirigami.Units.largeSpacing
                implicitWidth: Kirigami.Units.iconSizes.large
                implicitHeight: Kirigami.Units.iconSizes.large
                source: choice.iconName
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.rightMargin: Kirigami.Units.largeSpacing
                spacing: 2

                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 4
                    text: choice.heading
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                Controls.Label {
                    Layout.fillWidth: true
                    visible: choice.line !== ""
                    text: choice.line
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    opacity: 0.8
                }
            }
        }
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
                text: page.i18nContext.i18nc("@info", "Your household book is ready. How would you like to add what you pay and receive from now on?")
                wrapMode: Text.Wrap
            }

            Choice {
                id: typeChoice

                iconName: "document-edit"
                heading: page.i18nContext.i18nc("@action:button", "I'll type them myself")
                line: page.i18nContext.i18nc("@info", "Add each payment with + on the overview.")
                Component.onCompleted: forceActiveFocus()
                onClicked: {
                    ActionLog.taken("WizardEntriesPage_typeMyself")
                    page.typeMyselfClicked()
                }
            }

            Choice {
                iconName: "document-import"
                heading: page.i18nContext.i18nc("@action:button", "Import a file from my bank")
                line: page.i18nContext.i18nc("@info", "A statement you downloaded from your online banking, for example as CSV or CAMT.")
                onClicked: {
                    ActionLog.taken("WizardEntriesPage_importFile")
                    if (page.ownAccounts.length === 1) {
                        page.importClicked(page.ownAccounts[0].id)
                    } else {
                        page.choosingAccount = true
                    }
                }
            }

            // Which account the file is for, when there is more than one.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.gridUnit * 2
                visible: page.choosingAccount
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    text: page.i18nContext.i18nc("@info", "Which account is the file for?")
                    wrapMode: Text.Wrap
                }

                Repeater {
                    model: page.choosingAccount ? page.ownAccounts : []

                    delegate: Controls.Button {
                        required property var modelData

                        Layout.fillWidth: true
                        text: Plain.plain(modelData.name)
                        onClicked: {
                            ActionLog.taken("WizardEntriesPage_importInto")
                            page.importClicked(modelData.id)
                        }
                    }
                }
            }

            Choice {
                visible: page.bankOffered
                iconName: "network-connect"
                heading: page.i18nContext.i18nc("@action:button", "Connect to my bank")
                line: page.i18nContext.i18nc("@info", "For banks in Germany. KassenSturz only reads, it never sends money.")
                onClicked: {
                    ActionLog.taken("WizardEntriesPage_connectBank")
                    page.connectBankClicked()
                }
            }

            Controls.Label {
                Layout.alignment: Qt.AlignHCenter
                text: page.i18nContext.i18nc("@info", "You can change everything later.")
                opacity: 0.7
                font: Kirigami.Theme.smallFont
            }
        }
    }
}
