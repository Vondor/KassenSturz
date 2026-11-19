// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Plans page of the menu, right under Budgets (B-388, D-863): the
// payments that come again and again, with what a plan is in two sentences
// and "Add a plan" in the header. The list is PlannedPaymentsView.qml, the
// same one the accounts page's "Planned payments…" window shows.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property PlannedPayments plans
    required property AccountRegister register

    // For the smoke test.
    readonly property alias form: content.form
    readonly property alias planList: content.planList

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title a screen of the menu", "Plans")

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add a plan")
            onTriggered: {
                ActionLog.taken("PlansPage_addPlan")
                content.create()
            }
        }
    ]

    Component.onCompleted: content.refresh()

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        ExplanationCard {
            heading: page.i18nContext.i18nc("@title", "What is a plan?")
            explanation: page.i18nContext.i18nc("@info", "A payment that comes again and again: rent, salary, insurance. Plans count in the outlook ahead; you confirm each payment once it has happened.")
        }

        PlannedPaymentsView {
            id: content

            Layout.fillWidth: true
            plans: page.plans
            register: page.register
            newButtonShown: false
        }
    }
}
