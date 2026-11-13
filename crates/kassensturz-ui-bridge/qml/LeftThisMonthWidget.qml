// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The *Left this month* tile (US7, #725, D-770, `docs/SCHEDULES.md` 5): the
// balance of the open checking and cash accounts once the planned payments
// still due this month are taken off. Every figure is text from Rust
// (`PlannedPayments.view.forecast`, D-065); nothing is added up here. It
// never guesses from past spending and never gives advice: the line under
// the figure says so. With no plan it shows no figure, only the way to plan
// a payment (D-734: an unset tile is `unknown`).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Flickable {
    id: widget

    // `PlannedPayments.view`, or an error object.
    property var view: ({})
    // The layout is being arranged: the buttons are pictures of themselves.
    property bool arranging: false

    readonly property bool failed: view.code !== undefined
    readonly property var forecast: view.forecast ?? ({})
    readonly property bool hasAccounts: (forecast.accounts ?? []).length > 0
    readonly property bool hasPlans: forecast.hasPlans ?? false
    readonly property int overdue: forecast.overdue ?? 0
    readonly property int leftOut: forecast.leftOut ?? 0

    // *Plan a payment* was pressed: open the form for a new plan.
    signal planClicked()
    // *Check them…* or *Planned payments…* was pressed: open the window.
    signal openClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    contentHeight: column.implicitHeight
    clip: true

    ColumnLayout {
        id: column

        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            visible: widget.failed
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: widget.i18nContext.i18nc("@info", "The planned payments couldn't be read. Open Planned payments under Accounts to see what went wrong.")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !widget.failed && !widget.hasAccounts
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Kirigami.Theme.disabledTextColor
            text: widget.i18nContext.i18nc("@info", "There is no open checking or cash account to look ahead for yet.")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !widget.failed && widget.hasAccounts && !widget.hasPlans
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: widget.i18nContext.i18nc("@info", "Nothing planned yet. Add the payments you expect, like rent or insurance, to see what is left this month.")
        }

        Controls.Button {
            visible: !widget.failed && !widget.hasPlans
            enabled: !widget.arranging
            icon.name: "list-add"
            text: widget.i18nContext.i18nc("@action:button", "Plan a payment…")
            onClicked: {
                ActionLog.taken("LeftThisMonthWidget_plan")
                widget.planClicked()
            }
        }

        // The figure. Below zero it is drawn in the negative colour and
        // says so in words too, so colour is never the only sign.
        Kirigami.Heading {
            Layout.fillWidth: true
            visible: !widget.failed && widget.hasAccounts && widget.hasPlans
            level: 1
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: (widget.forecast.belowZero ?? false) ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
            text: widget.forecast.left ?? ""
            Accessible.name: (widget.forecast.belowZero ?? false)
                ? widget.i18nContext.i18nc("@info %1 is an amount below zero", "Left this month: %1, below zero", widget.forecast.exactLeft ?? "")
                : widget.i18nContext.i18nc("@info %1 is an amount", "Left this month: %1", widget.forecast.exactLeft ?? "")

            Controls.ToolTip.visible: hover.hovered && (widget.forecast.left ?? "") !== (widget.forecast.exactLeft ?? "")
            Controls.ToolTip.text: widget.forecast.exactLeft ?? ""

            HoverHandler {
                id: hover
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !widget.failed && widget.hasAccounts && widget.hasPlans
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: widget.i18nContext.i18nc("@info %1 is the balance now, %2 the planned payments still due this month", "%1 now, minus %2 still to pay this month", widget.forecast.balanceNow ?? "", widget.forecast.outDue ?? "")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !widget.failed && widget.hasAccounts && widget.hasPlans && (widget.forecast.inDue ?? "") !== ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: widget.i18nContext.i18nc("@info %1 is the planned income still to come, %2 the balance expected at the month's end", "At month end, with the %1 you expect: %2", widget.forecast.inDue ?? "", widget.forecast.expectedEnd ?? "")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !widget.failed && widget.overdue > 0
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            font.bold: true
            text: widget.i18nContext.i18ncp("@info %1 is how many planned payments", "%1 planned payment passed without an entry and is not counted.", "%1 planned payments passed without an entry and are not counted.", widget.overdue)
        }

        Controls.Button {
            visible: !widget.failed && widget.overdue > 0
            enabled: !widget.arranging
            icon.name: "view-calendar-tasks"
            text: widget.i18nContext.i18nc("@action:button check the planned payments that passed", "Check them…")
            onClicked: {
                ActionLog.taken("LeftThisMonthWidget_check")
                widget.openClicked()
            }
        }

        // What the figure leaves out, in small print.
        Controls.Label {
            Layout.fillWidth: true
            visible: !widget.failed && widget.hasAccounts && widget.hasPlans
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.disabledTextColor
            text: [
                widget.i18nContext.i18nc("@info %1 lists names of accounts", "For %1.", (widget.forecast.accounts ?? []).join(", ")),
                widget.i18nContext.i18nc("@info", "Based on your planned payments only. Spending that is not planned is not included."),
                (widget.forecast.accounts ?? []).length === 1
                    ? widget.i18nContext.i18nc("@info", "Moves to your other accounts are not counted.") : "",
                widget.leftOut > 0
                    ? widget.i18nContext.i18ncp("@info %1 is how many accounts", "%1 account in another currency is not included.", "%1 accounts in other currencies are not included.", widget.leftOut) : ""
            ].filter(line => line !== "").join(" ")
        }
    }
}
