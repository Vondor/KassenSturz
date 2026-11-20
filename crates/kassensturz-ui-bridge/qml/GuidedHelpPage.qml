// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Help: the steps of the guided setup again (#1031, D-902,
// docs/GUIDED_SETUP.md). Every step, also a skipped one, opens from here on
// the book that is open and stands alone: no step count, nothing to skip.
// How far the guided setup got is not stored anywhere (D-921), so this list
// is the same for every book.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    // The step named `step` is wanted: "transactions" or "balance".
    signal stepClicked(string step)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Help: guided setup")

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "The guided setup shows the important things step by step, with your own data. You can open every step here whenever you like, also one you skipped.")
    }

    FormCard.FormCard {
        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button a step of the guided setup", "Get your transactions")
            description: page.i18nContext.i18nc("@info", "Connect to your bank or import a file from it")
            onClicked: {
                ActionLog.taken("GuidedHelpPage_transactions")
                page.stepClicked("transactions")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button a step of the guided setup", "Is this balance right?")
            description: page.i18nContext.i18nc("@info", "Compare each account with what your bank shows, and correct it if needed")
            onClicked: {
                ActionLog.taken("GuidedHelpPage_balance")
                page.stepClicked("balance")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button a step of the guided setup", "Sort your transactions")
            description: page.i18nContext.i18nc("@info", "Give every transaction of a name its category at once, and let a Rule do it in future")
            onClicked: {
                ActionLog.taken("GuidedHelpPage_sort")
                page.stepClicked("sort")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button a step of the guided setup", "Your first chart")
            description: page.i18nContext.i18nc("@info", "Build a chart from your own transactions with the Statistic Builder")
            onClicked: {
                ActionLog.taken("GuidedHelpPage_chart")
                page.stepClicked("chart")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button a step of the guided setup", "A budget and a planned payment")
            description: page.i18nContext.i18nc("@info", "Say how much you want to spend, and count in what is paid again and again")
            onClicked: {
                ActionLog.taken("GuidedHelpPage_budget")
                page.stepClicked("budget")
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            text: page.i18nContext.i18nc("@action:button a step of the guided setup", "Shape your start page")
            description: page.i18nContext.i18nc("@info", "Move, resize, add and remove the tiles of your start page")
            onClicked: {
                ActionLog.taken("GuidedHelpPage_start")
                page.stepClicked("start")
            }
        }
    }
}
