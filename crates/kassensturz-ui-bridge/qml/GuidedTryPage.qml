// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Steps 4 to 6 of the guided setup and its last page (#1031,
// docs/GUIDED_SETUP.md): a first chart (D-900), a budget and a planned
// payment (D-901), the start page (D-902), and "Done" (D-902, D-931). Each
// says in a few words what the thing is for and opens the real function on
// the person's own book; nothing is copied here, so the guided setup can't
// show something the app does differently. Closing that function leads back
// to this page. Every step can be skipped (D-911, D-342).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    // Which page this is: "chart", "budget", "start" or "done".
    required property string which
    // This page's step and how many there are; 0 when opened from Help, and
    // for "done", which is no step.
    property int step: 0
    property int count: 0

    // The person wants to try `what`: "chart", "budget", "plan", "arrange"
    // or, on the last page, "wiki".
    signal tryClicked(string what)
    signal nextClicked()
    signal skipClicked()
    signal endClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property bool done: which === "done"

    function titleOf(which: string): string {
        switch (which) {
        case "chart":
            return i18nContext.i18nc("@title a step of the guided setup", "Your first chart");
        case "budget":
            return i18nContext.i18nc("@title a step of the guided setup", "A budget and a planned payment");
        case "start":
            return i18nContext.i18nc("@title a step of the guided setup", "Shape your start page");
        default:
            return i18nContext.i18nc("@title the last page of the guided setup", "You are set up");
        }
    }

    function introOf(which: string): string {
        switch (which) {
        case "chart":
            return i18nContext.i18nc("@info", "A chart shows at a glance where your money goes. Build one from your own transactions: “Spending by category” is already chosen, you pick the period and the look, give it a name and save it. Money without a category shows as “Not sorted yet”. When you are done, come back here and go on.");
        case "budget":
            return i18nContext.i18nc("@info", "Two things that help you plan. Each is saved only when you say so, and you can leave out either. Both can be paused and continued later; what was there before is kept.");
        case "start":
            return i18nContext.i18nc("@info", "The start page is yours: it shows what you want to see every day. Try these four things, then click “Done” there and come back here. Everything can be taken back with Undo.");
        default:
            return i18nContext.i18nc("@info", "That was the guided setup. You find every step again whenever you like: in Settings, under “Help: guided setup”, also the ones you skipped.");
        }
    }

    title: Plain.plain(titleOf(which))
    header: GuidedBar {
        step: page.step
        count: page.count
        onEndClicked: page.endClicked()
    }
    footer: GuidedFooter {
        nextText: Plain.plain(page.done
            ? page.i18nContext.i18nc("@action:button leaves the guided setup", "Go to my overview")
            : page.step > 0
                ? page.i18nContext.i18nc("@action:button", "Continue")
                : page.i18nContext.i18nc("@action:button closes a step opened from Help", "Done"))
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
                text: Plain.plain(page.introOf(page.which))
                wrapMode: Text.Wrap
            }

            Choice {
                visible: page.which === "chart"
                iconName: "office-chart-bar"
                heading: page.i18nContext.i18nc("@action:button", "Build my first chart")
                line: page.i18nContext.i18nc("@info", "Opens the Statistic Builder, which leads you through it. You find it later in the menu, with many more kinds of charts.")
                onClicked: {
                    ActionLog.taken("GuidedTryPage_chart")
                    page.tryClicked("chart")
                }
            }

            Choice {
                visible: page.which === "budget"
                iconName: "wallet-open"
                heading: page.i18nContext.i18nc("@action:button", "Set a budget")
                line: page.i18nContext.i18nc("@info", "How much you want to spend on something in a month, for example on groceries. KassenSturz then shows how much of it is left.")
                onClicked: {
                    ActionLog.taken("GuidedTryPage_budget")
                    page.tryClicked("budget")
                }
            }

            Choice {
                visible: page.which === "budget"
                iconName: "view-calendar-upcoming-events"
                heading: page.i18nContext.i18nc("@action:button", "Plan a payment")
                line: page.i18nContext.i18nc("@info", "Something that is paid again and again, for example a standing order, the rent or a subscription that is taken monthly. KassenSturz then counts it in before it happens.")
                onClicked: {
                    ActionLog.taken("GuidedTryPage_plan")
                    page.tryClicked("plan")
                }
            }

            // The four things to try on the start page.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.gridUnit
                visible: page.which === "start"
                spacing: Kirigami.Units.smallSpacing

                Repeater {
                    model: [
                        page.i18nContext.i18nc("@info one of four things to try on the start page", "1. Move a tile: drag it to another place."),
                        page.i18nContext.i18nc("@info one of four things to try on the start page", "2. Make a tile larger or smaller: drag any of its edges or its corner."),
                        page.i18nContext.i18nc("@info one of four things to try on the start page", "3. Add a tile: for example the chart you just made."),
                        page.i18nContext.i18nc("@info one of four things to try on the start page", "4. Remove a tile: best the one you just added, so nothing you want is lost.")
                    ]

                    delegate: Controls.Label {
                        required property string modelData

                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        text: modelData
                        wrapMode: Text.Wrap
                    }
                }
            }

            Choice {
                visible: page.which === "start"
                iconName: "view-grid"
                heading: page.i18nContext.i18nc("@action:button", "Arrange my start page")
                line: page.i18nContext.i18nc("@info", "Opens your start page ready to be arranged. “Done” there keeps what you changed, “Cancel” throws it away.")
                onClicked: {
                    ActionLog.taken("GuidedTryPage_arrange")
                    page.tryClicked("arrange")
                }
            }

            Choice {
                visible: page.done
                iconName: "help-contents"
                heading: page.i18nContext.i18nc("@action:button opens the handbook in the web browser", "More help in the wiki")
                line: page.i18nContext.i18nc("@info", "The handbook, with every function explained step by step. Opens in your web browser.")
                onClicked: {
                    ActionLog.taken("GuidedTryPage_wiki")
                    page.tryClicked("wiki")
                }
            }
        }
    }
}
