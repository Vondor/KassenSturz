// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Step 1 of the guided setup (#1031, docs/GUIDED_SETUP.md): get the
// transactions, from the bank (D-896, with the sentence on which PIN is
// meant, D-897) or from a file of the bank (D-911). The page only leads to
// the existing bank setup and the existing import, which save what they save
// today, each after its own confirmation; nothing is written here. The user
// comes back to this page afterwards and goes on.

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
    // This page's step and how many there are; 0 when opened from Help.
    property int step: 0
    property int count: 0

    signal connectBankClicked()
    signal importClicked(string accountId)
    signal nextClicked()
    signal skipClicked()
    signal endClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The book's accounts, one list.
    readonly property var ownAccounts: (page.accounts.view.sections ?? []).reduce((all, section) => all.concat(section.accounts ?? []), [])
    // The accounts are listed once the user wants a file imported and has
    // more than one.
    property bool choosingAccount: false

    title: i18nContext.i18nc("@title a step of the guided setup", "Get your transactions")
    header: GuidedBar {
        step: page.step
        count: page.count
        onEndClicked: page.endClicked()
    }
    footer: GuidedFooter {
        nextText: page.step > 0
            ? page.i18nContext.i18nc("@action:button", "Continue")
            : page.i18nContext.i18nc("@action:button closes a step opened from Help", "Done")
        skipVisible: page.step > 0
        onSkipClicked: page.skipClicked()
        onNextClicked: page.nextClicked()
    }

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
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@info", "Let KassenSturz read what you paid and received, so you don't have to type it. You see everything as a preview first; it is saved only when you agree. When you are done, come back here and go on.")
                wrapMode: Text.Wrap
            }

            Choice {
                visible: page.bankOffered
                iconName: "network-connect"
                heading: page.i18nContext.i18nc("@action:button", "Connect to my bank")
                line: page.i18nContext.i18nc("@info", "For banks in Germany. KassenSturz only reads, it never sends money. You need the PIN or password of your online banking (not your card's PIN).")
                onClicked: {
                    ActionLog.taken("GuidedTransactionsPage_connectBank")
                    page.connectBankClicked()
                }
            }

            Choice {
                iconName: "document-import"
                heading: page.i18nContext.i18nc("@action:button", "Import a file from my bank")
                line: page.i18nContext.i18nc("@info", "For banks without online access: a file with your transactions that you download in your online banking, for example as CSV, MT940 or CAMT.")
                onClicked: {
                    ActionLog.taken("GuidedTransactionsPage_importFile")
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
                    textFormat: Text.PlainText
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
                            ActionLog.taken("GuidedTransactionsPage_importInto")
                            page.importClicked(modelData.id)
                        }
                    }
                }
            }
        }
    }
}
