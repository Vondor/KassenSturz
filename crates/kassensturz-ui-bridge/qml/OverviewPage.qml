// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The overview (M3.9): how the user is doing on one page: what all the
// accounts hold together, and one month's money in and money out with where
// it came from and where it went. The accounts and their transactions are on
// their own screens (round 1). Money moved between the user's own accounts
// counts as neither (D-021). Every amount, total and order comes from Rust,
// per currency (D-065); this page only shows text.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property BookOverview overview
    readonly property var view: overview.view ?? ({})
    readonly property var sections: view.sections ?? []
    readonly property var totals: view.totals ?? []

    signal addAccountClicked()
    signal addTransactionClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // All accounts of a currency together, with `amount` as shown or exactly.
    function togetherText(amount: string, owes: bool): string {
        return owes
            ? i18nContext.i18nc("@label %1 is an amount of money", "All your accounts together: you owe %1", amount)
            : i18nContext.i18nc("@label %1 is an amount of money", "All your accounts together: %1", amount);
    }

    // What was left of the month's money in, or spent on top of it.
    function leftText(amount: string, overspent: bool): string {
        return overspent
            ? i18nContext.i18nc("@label %1 is an amount of money", "Spent more than came in: %1", amount)
            : i18nContext.i18nc("@label %1 is an amount of money", "Left over: %1", amount);
    }

    function categoryName(total: var): string {
        return total.unsorted ? i18nContext.i18nc("@label money with no category yet", "Not sorted yet") : total.name;
    }

    // Going to another screen and back starts from the current month again.
    Component.onCompleted: overview.showThisMonth()

    title: i18nContext.i18nc("@title", "Overview")

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

        // What all the accounts hold, per currency.
        Repeater {
            model: page.sections

            delegate: ColumnLayout {
                id: section

                required property var modelData

                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                // Only a book with several currencies needs to say which.
                Kirigami.Heading {
                    visible: page.sections.length > 1
                    text: section.modelData.currency
                    level: 3
                }

                AmountLabel {
                    Layout.fillWidth: true
                    text: page.togetherText(section.modelData.together, section.modelData.togetherOwes)
                    exactText: page.togetherText(section.modelData.exactTogether, section.modelData.togetherOwes)
                    font.bold: true
                    font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.3
                    wrapMode: Text.Wrap
                }
            }
        }

        // The month, with the way to the ones before and after it.
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.smallSpacing

            Controls.ToolButton {
                id: earlierButton

                icon.name: "go-previous"
                text: page.i18nContext.i18nc("@action:button the month before", "Earlier month")
                display: Controls.AbstractButton.IconOnly
                enabled: page.view.hasEarlier ?? false
                onClicked: page.overview.showEarlierMonth()

                Controls.ToolTip.text: text
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }

            Kirigami.Heading {
                Layout.fillWidth: true
                text: page.view.month ?? ""
                level: 2
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }

            Controls.ToolButton {
                icon.name: "go-next"
                text: page.i18nContext.i18nc("@action:button the month after", "Later month")
                display: Controls.AbstractButton.IconOnly
                enabled: page.view.hasLater ?? false
                onClicked: page.overview.showLaterMonth()

                Controls.ToolTip.text: text
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }

        Controls.Button {
            Layout.alignment: Qt.AlignHCenter
            visible: !(page.view.thisMonth ?? true)
            icon.name: "go-jump-today"
            text: page.i18nContext.i18nc("@action:button", "Back to this month")
            onClicked: page.overview.showThisMonth()
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: page.view.empty ?? false
            text: page.i18nContext.i18nc("@info", "Nothing came in or went out this month yet. Add what you paid or received, and it shows up here.")
            wrapMode: Text.Wrap
        }

        Repeater {
            model: page.totals

            delegate: ColumnLayout {
                id: monthTotals

                required property var modelData

                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing
                visible: !(page.view.empty ?? false)

                Kirigami.Heading {
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    text: page.totals.length > 1
                        ? page.i18nContext.i18nc("@title %1 is a currency code, e.g. EUR", "Money in and out in %1", monthTotals.modelData.currency)
                        : page.i18nContext.i18nc("@title", "Money in and out")
                    level: 3
                }

                Repeater {
                    model: [
                        { name: page.i18nContext.i18nc("@label", "Money in"), amount: monthTotals.modelData.moneyIn, exactAmount: monthTotals.modelData.exactMoneyIn },
                        { name: page.i18nContext.i18nc("@label", "Money out"), amount: monthTotals.modelData.moneyOut, exactAmount: monthTotals.modelData.exactMoneyOut },
                    ]

                    delegate: AmountLine {
                        Layout.fillWidth: true
                    }
                }

                AmountLabel {
                    Layout.fillWidth: true
                    text: page.leftText(monthTotals.modelData.left, monthTotals.modelData.overspent)
                    exactText: page.leftText(monthTotals.modelData.exactLeft, monthTotals.modelData.overspent)
                    font.bold: true
                    color: monthTotals.modelData.overspent ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                    wrapMode: Text.Wrap
                }

                Kirigami.Heading {
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    visible: monthTotals.modelData.spending.length > 0
                    text: page.i18nContext.i18nc("@title", "Where your money went")
                    level: 3
                }

                Repeater {
                    model: monthTotals.modelData.spending

                    delegate: AmountLine {
                        Layout.fillWidth: true
                        name: page.categoryName(modelData)
                    }
                }

                Kirigami.Heading {
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    visible: monthTotals.modelData.income.length > 0
                    text: page.i18nContext.i18nc("@title", "Where your money came from")
                    level: 3
                }

                Repeater {
                    model: monthTotals.modelData.income

                    delegate: AmountLine {
                        Layout.fillWidth: true
                        name: page.categoryName(modelData)
                    }
                }
            }
        }

        // Transfers are left out of the totals (D-021); the page says so
        // only when the month has one.
        Controls.Label {
            Layout.fillWidth: true
            visible: (page.view.transfers ?? 0) > 0
            text: page.i18nContext.i18ncp("@info", "Money you moved between your accounts (once this month) isn't counted as money in or out.", "Money you moved between your accounts (%1 times this month) isn't counted as money in or out.", page.view.transfers ?? 0)
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
