// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Every move between the user's accounts the book suggests, listed once
// (US6.3, D-749): "Join all 7" joins them in one go, one Undo takes them all
// back; "Go through them one by one" asks about each as before (D-692). After
// "Join all", moves that look monthly (the same two accounts, amount and
// other party in at least two months) are offered once, each with a box that
// starts unticked. Every text and amount comes from Rust (D-065).
//
//     joinAll.show(register.transferSuggestions(""));
//     onOneByOne: found => transferPrompt.ask(found)

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    required property AccountRegister register
    // What register.transferSuggestions("") returned.
    property var suggestions: []
    // What register.joinAll() returned, once the user joined them all.
    property var done: ({})
    readonly property bool joined: done.joined !== undefined
    readonly property var looksMonthly: done.looksMonthly ?? []

    // The user chose to go through the pairs one by one.
    signal oneByOne(var found)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function show(found: var) {
        error.result = ({});
        done = ({});
        suggestions = found;
        open();
    }

    // Joins the pairs the user saw, or says what changed.
    function joinAll() {
        const result = register.joinAll(suggestions.length);
        error.result = result;
        if (result.code === undefined) {
            done = result;
            if (result.looksMonthly.length === 0) {
                close();
            }
        }
    }

    // Remembers the moves the user ticked, then closes.
    function rememberTicked() {
        const ticked = [];
        for (let i = 0; i < monthlyList.count; ++i) {
            if (monthlyList.itemAt(i).ticked) {
                ticked.push(i);
            }
        }
        const result = register.rememberLooksMonthly(ticked.join(","));
        error.result = result;
        if (result.code === undefined) {
            close();
        }
    }

    title: Plain.plain(joined
        ? i18nContext.i18nc("@title:window", "These look like monthly moves")
        : i18nContext.i18nc("@title:window", "Moves between your accounts"))
    standardButtons: Kirigami.Dialog.NoButton
    preferredWidth: Kirigami.Units.gridUnit * 30
    customFooterActions: joined ? afterActions : beforeActions

    readonly property list<Kirigami.Action> afterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Join them by themselves from now on")
            icon.name: "edit-link"
            onTriggered: {
                ActionLog.taken("JoinAllDialog_rememberTicked")
                dialog.rememberTicked()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Not now")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("JoinAllDialog_notNow")
                dialog.close()
            }
        }
    ]

    readonly property list<Kirigami.Action> beforeActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18ncp("@action:button %1 is how many moves", "Join the move", "Join all %1", dialog.suggestions.length)
            icon.name: "edit-link"
            onTriggered: {
                ActionLog.taken("JoinAllDialog_joinAll")
                dialog.joinAll()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Go through them one by one")
            icon.name: "view-list-details"
            onTriggered: {
                ActionLog.taken("JoinAllDialog_oneByOne")
                const found = dialog.suggestions;
                dialog.close();
                dialog.oneByOne(found);
            }
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        FormError {
            id: error

            Layout.fillWidth: true
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !dialog.joined
            text: dialog.i18nContext.i18nc("@info", "These pairs look like money moved between your own accounts. Joined, they count as moving your own money around, not as spending and income. Both accounts keep their balance and the dates they show. One Undo takes them all apart again; where a Rule had sorted a side, Undo brings its category back.")
            wrapMode: Text.Wrap
        }

        Repeater {
            model: dialog.joined ? [] : dialog.suggestions

            delegate: Controls.Label {
                required property var modelData

                textFormat: Text.PlainText
                Layout.fillWidth: true
                text: dialog.i18nContext.i18nc("@info %1 is an amount, %2 and %4 names of accounts, %3 and %5 dates", "%1 from “%2” (%3) to “%4” (%5)", modelData.amount, modelData.outgoing.account, modelData.outgoing.date, modelData.incoming.account, modelData.incoming.date)
                wrapMode: Text.Wrap
            }
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: dialog.joined
            text: dialog.i18nContext.i18ncp("@info %1 is how many were joined", "%1 move was joined.", "%1 moves were joined.", dialog.done.joined ?? 0) + " " + dialog.i18nContext.i18nc("@info", "Join them by themselves from now on? Only the ones you tick.")
            wrapMode: Text.Wrap
        }

        Repeater {
            id: monthlyList

            model: dialog.looksMonthly

            delegate: Controls.CheckBox {
                required property var modelData
                readonly property bool ticked: checked

                Layout.fillWidth: true
                checked: false
                text: Plain.plain(dialog.i18nContext.i18nc("@option:check %1 is an amount, %2 and %3 names of accounts, %4 a day of the month, %5 how many months", "%1 from “%2” to “%3”, around day %4 of the month, seen in %5 months", modelData.amount, modelData.from, modelData.to, modelData.day, modelData.months))
                onToggled: {
                    ActionLog.taken("JoinAllDialog_tickMove")
                }
            }
        }
    }
}
