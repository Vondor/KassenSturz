// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Page 2 of the setup wizard (M7a.9, docs/SETUP_WIZARD.md): how the user
// wants to start. Three choices of equal size, the user's own accounts first
// (Q-396), and a small link for a book they already have. Nothing is written
// here.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    signal ownAccountsClicked()
    signal exampleClicked()
    signal fromProgramClicked()
    signal openBookClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "How would you like to start?")
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

            Choice {
                iconName: "view-bank-account"
                heading: page.i18nContext.i18nc("@action:button", "My own accounts")
                line: page.i18nContext.i18nc("@info", "Type how much is in each account today. Takes about two minutes.")
                onClicked: {
                    ActionLog.taken("WizardStartPage_ownAccounts")
                    page.ownAccountsClicked()
                }
            }

            Choice {
                iconName: "view-preview"
                heading: page.i18nContext.i18nc("@action:button", "Try an example book")
                line: page.i18nContext.i18nc("@info", "Look around in a made-up household.")
                onClicked: {
                    ActionLog.taken("WizardStartPage_example")
                    page.exampleClicked()
                }
            }

            Choice {
                iconName: "document-import"
                heading: page.i18nContext.i18nc("@action:button", "Bring my data from another program")
                line: page.i18nContext.i18nc("@info", "A KMyMoney book, or a copy you saved from KassenSturz.")
                onClicked: {
                    ActionLog.taken("WizardStartPage_fromProgram")
                    page.fromProgramClicked()
                }
            }

            Controls.Button {
                Layout.alignment: Qt.AlignHCenter
                flat: true
                text: page.i18nContext.i18nc("@action:button", "Open a KassenSturz book I already have…")
                onClicked: {
                    ActionLog.taken("WizardStartPage_openBook")
                    page.openBookClicked()
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
