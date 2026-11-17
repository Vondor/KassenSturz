// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The *Coming up* tile (US7, #795, D-769, `docs/SCHEDULES.md` 2.6): the
// planned payments of the next 30 days, soonest first, as text from Rust
// (`PlannedPayments.view.comingUp`, D-065). A plan only says what is
// expected: it moves no money and sends nothing (D-053). With no plan it
// says so and offers to plan a payment (D-734: an unset tile is `unknown`).

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
    readonly property bool hasPlans: (view.plans ?? []).length > 0
    readonly property var payments: view.comingUp ?? []

    // *Plan a payment* was pressed: open the form for a new plan.
    signal planClicked()
    // *Planned payments…* was pressed: open the window.
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
            visible: !widget.failed && !widget.hasPlans
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: widget.i18nContext.i18nc("@info", "Nothing planned yet. A planned payment is one you expect on a day, like rent or insurance. It moves no money by itself.")
        }

        Controls.Button {
            visible: !widget.failed && !widget.hasPlans
            enabled: !widget.arranging
            icon.name: "list-add"
            text: widget.i18nContext.i18nc("@action:button", "Plan a payment…")
            onClicked: {
                ActionLog.taken("ComingUpWidget_plan")
                widget.planClicked()
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !widget.failed && widget.hasPlans && widget.payments.length === 0
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Kirigami.Theme.disabledTextColor
            text: widget.i18nContext.i18nc("@info", "No planned payment in the next 30 days.")
        }

        Repeater {
            model: widget.failed ? [] : widget.payments

            delegate: Controls.Label {
                id: line

                required property var modelData

                Layout.fillWidth: true
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: line.modelData.outgoing
                    ? widget.i18nContext.i18nc("@info %1 is the name of a planned payment, %2 an amount, %3 a date", "%1: %2 out, due %3", line.modelData.name, line.modelData.amount, line.modelData.due)
                    : widget.i18nContext.i18nc("@info %1 is the name of a planned payment, %2 an amount, %3 a date", "%1: %2 in, due %3", line.modelData.name, line.modelData.amount, line.modelData.due)
            }
        }

        Controls.Button {
            visible: !widget.failed && widget.hasPlans
            enabled: !widget.arranging
            icon.name: "view-calendar-tasks"
            text: widget.i18nContext.i18nc("@action:button", "Planned payments…")
            onClicked: {
                ActionLog.taken("ComingUpWidget_open")
                widget.openClicked()
            }
        }
    }
}
