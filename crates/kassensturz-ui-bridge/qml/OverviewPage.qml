// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The overview after the first steps (M1.M): the accounts side by side
// (D-021), what came in and went out, where it went, and the latest
// transactions. Every amount, total and order comes from Rust (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property FirstRunMock mock
    readonly property var overview: mock.overview

    signal addAccountClicked()
    signal addTransactionClicked()
    signal accountClicked(index: int)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function typeName(key: string): string {
        switch (key) {
        case "checking":
            return i18nContext.i18nc("@item an account type", "Checking account");
        case "savings":
            return i18nContext.i18nc("@item an account type", "Savings account");
        case "cash":
            return i18nContext.i18nc("@item an account type", "Cash");
        case "credit-card":
            return i18nContext.i18nc("@item an account type", "Credit card");
        default:
            return "";
        }
    }

    // A debt reads "You owe" (D-067).
    function balanceText(balance: string, owes: bool): string {
        return owes ? i18nContext.i18nc("@label %1 is an amount of money", "You owe %1", balance) : balance;
    }

    title: i18nContext.i18nc("@title", "Overview")
    header: MockNotice {}

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add transaction")
            onTriggered: page.addTransactionClicked()
        },
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add account")
            onTriggered: page.addAccountClicked()
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Heading {
            text: page.i18nContext.i18nc("@title", "Your accounts")
            level: 2
        }

        Kirigami.CardsLayout {
            Layout.fillWidth: true

            Repeater {
                model: page.overview.accounts

                delegate: Kirigami.AbstractCard {
                    id: card

                    required property var modelData
                    required property int index

                    readonly property string balanceText: page.balanceText(modelData.balance, modelData.owes)

                    showClickFeedback: true
                    Accessible.name: [modelData.name, page.typeName(modelData.type), balanceText].join(", ")
                    onClicked: page.accountClicked(index)

                    contentItem: ColumnLayout {
                        spacing: 0

                        Controls.Label {
                            Layout.fillWidth: true
                            text: card.modelData.name
                            elide: Text.ElideRight
                            font.bold: true
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            text: page.typeName(card.modelData.type)
                            // An account named after its type needn't say it twice.
                            visible: text !== card.modelData.name
                            font: Kirigami.Theme.smallFont
                        }

                        Kirigami.Heading {
                            Layout.fillWidth: true
                            Layout.topMargin: Kirigami.Units.smallSpacing
                            level: 3
                            text: card.balanceText
                            horizontalAlignment: Text.AlignRight
                        }
                    }
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            text: page.overview.togetherOwes
                ? page.i18nContext.i18nc("@label %1 is an amount of money", "All your accounts together: you owe %1", page.overview.together)
                : page.i18nContext.i18nc("@label %1 is an amount of money", "All your accounts together: %1", page.overview.together)
            font.bold: true
            wrapMode: Text.Wrap
        }

        Kirigami.Heading {
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: page.i18nContext.i18nc("@title", "Money in and out")
            level: 2
        }

        Repeater {
            model: [
                { name: page.i18nContext.i18nc("@label", "Money in"), amount: page.overview.moneyIn },
                { name: page.i18nContext.i18nc("@label", "Money out"), amount: page.overview.moneyOut },
            ]

            delegate: AmountLine {
                Layout.fillWidth: true
            }
        }

        Kirigami.Heading {
            Layout.topMargin: Kirigami.Units.largeSpacing
            visible: page.overview.spending.length > 0
            text: page.i18nContext.i18nc("@title", "Where your money went")
            level: 2
        }

        Repeater {
            model: page.overview.spending

            delegate: AmountLine {
                Layout.fillWidth: true
            }
        }

        Kirigami.Heading {
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: page.i18nContext.i18nc("@title", "Latest transactions")
            level: 2
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: page.overview.latest.length === 0
            text: page.i18nContext.i18nc("@info", "No transactions yet. Add what you paid or received, and it shows up here.")
            wrapMode: Text.Wrap
        }

        Repeater {
            model: page.overview.latest

            delegate: TransactionRow {
                Layout.fillWidth: true
            }
        }
    }
}
