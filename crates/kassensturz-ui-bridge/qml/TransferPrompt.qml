// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Asks about each suggested Transfer, one at a time (M3.7, D-030, D-087,
// D-120): two transactions that look like one move of money between the
// user's own accounts. Nothing is joined without the user's click, and a
// "No" is not remembered (D-087): the pair drops out once either side gets
// a category. Used after a transaction the user typed, and by the import
// (M3.10) for the Transfers its statements suggest; any AccountRegister
// will do as `register`, since the Transfers belong to the book.
//
//     prompt.ask(register.transferSuggestions(transactionId));
//
// It is also the preview for a pair the user picked (US3.3, D-692), which
// `TransferPartnerDialog` lists:
//
//     prompt.askPicked([partner]);
//
// A side that is in a category says which one goes and who put it there.
// `finished()` follows when the last suggestion has been answered or the
// dialog was closed.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.PromptDialog {
    id: prompt

    required property AccountRegister register
    // What register.transferSuggestions() returned.
    property var suggestions: []
    // The suggestion asked about now.
    property int current: 0
    // Whether the pairs were picked by the user (joinPicked) rather than
    // suggested (joinTransfer).
    property bool picked: false

    readonly property var suggestion: suggestions[current] ?? ({})
    readonly property var outgoing: suggestion.outgoing ?? ({})
    readonly property var incoming: suggestion.incoming ?? ({})
    readonly property bool anyCategory: (outgoing.category ?? "") !== "" || (incoming.category ?? "") !== ""

    // Every suggestion has been answered, or the dialog was closed.
    signal finished()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Asks about `found`, a list of suggestions; an error object or an empty
    // list asks nothing and finishes at once.
    function ask(found: var) {
        start(found, false);
    }

    // Asks about `found`, pairs the user picked from transferPartners().
    function askPicked(found: var) {
        start(found, true);
    }

    function start(found: var, byHand: bool) {
        // An error comes back as an object without a length.
        if (!(found?.length > 0)) {
            finished();
            return;
        }
        error.result = ({});
        picked = byHand;
        suggestions = found;
        current = 0;
        open();
    }

    // Who put a side in its category, and that it goes (D-692).
    function sortedText(side: var): string {
        const category = side.category ?? "";
        if (category === "") {
            return "";
        }
        if (side.sortedBy === "rule") {
            return (side.rule ?? "") !== ""
                ? i18nContext.i18nc("@info %1 is a category, %2 the name of a Rule", "Sorted as “%1” by the Rule “%2”. Joined, it no longer counts in “%1”.", category, side.rule)
                : i18nContext.i18nc("@info %1 is a category", "Sorted as “%1” by a Rule. Joined, it no longer counts in “%1”.", category);
        }
        return i18nContext.i18nc("@info %1 is a category", "You sorted it as “%1” yourself. Joined, it no longer counts in “%1”.", category);
    }

    // Joins the pair shown, or leaves it apart, and moves on.
    function answer(join: bool) {
        if (join) {
            const result = picked
                ? register.joinPicked(suggestion.partnerIsIncoming ? outgoing.id : incoming.id, suggestion.partnerIsIncoming ? incoming.id : outgoing.id)
                : register.joinTransfer(outgoing.id, incoming.id);
            error.result = result;
            if (result.code !== undefined) {
                return;
            }
        }
        if (current + 1 < suggestions.length) {
            current += 1;
        } else {
            close();
        }
    }

    title: i18nContext.i18nc("@title:window", "Money moved between your accounts?")
    subtitle: suggestions.length > 1
        ? i18nContext.i18nc("@info %1 is the number of this question, %2 how many there are", "Question %1 of %2: these two look like one move of money between your own accounts.", current + 1, suggestions.length)
        : i18nContext.i18nc("@info", "These two look like one move of money between your own accounts.")
    standardButtons: Kirigami.Dialog.NoButton
    customFooterActions: [
        Kirigami.Action {
            text: prompt.i18nContext.i18nc("@action:button", "Yes, join them")
            icon.name: "edit-link"
            onTriggered: {
                ActionLog.taken("TransferPrompt_yesJoinThem")
                prompt.answer(true)
            }
        },
        Kirigami.Action {
            text: prompt.i18nContext.i18nc("@action:button", "No, keep them apart")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("TransferPrompt_noKeepThem")
                prompt.answer(false)
            }
        }
    ]

    onClosed: finished()

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        FormError {
            id: error

            Layout.fillWidth: true
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            text: prompt.i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 a date", "%1 left %2 on %3.", prompt.suggestion.amount ?? "", prompt.outgoing.account ?? "", prompt.outgoing.date ?? "")
            wrapMode: Text.Wrap
            font.bold: true
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: (prompt.outgoing.otherParty ?? "") !== ""
            text: prompt.i18nContext.i18nc("@info %1 is a name that account shows for the money", "There it says: %1", prompt.outgoing.otherParty ?? "")
            wrapMode: Text.Wrap
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: text !== ""
            text: prompt.sortedText(prompt.outgoing)
            wrapMode: Text.Wrap
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: prompt.outgoing.movesNote ?? false
            text: prompt.i18nContext.i18nc("@info %1 is the name of an account", "What you wrote on it or attached to it stays, on the entry in %1.", prompt.outgoing.account ?? "")
            wrapMode: Text.Wrap
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            text: prompt.i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 a date", "%1 arrived in %2 on %3.", prompt.suggestion.amount ?? "", prompt.incoming.account ?? "", prompt.incoming.date ?? "")
            wrapMode: Text.Wrap
            font.bold: true
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: (prompt.incoming.otherParty ?? "") !== ""
            text: prompt.i18nContext.i18nc("@info %1 is a name that account shows for the money", "There it says: %1", prompt.incoming.otherParty ?? "")
            wrapMode: Text.Wrap
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: text !== ""
            text: prompt.sortedText(prompt.incoming)
            wrapMode: Text.Wrap
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: prompt.incoming.movesNote ?? false
            text: prompt.i18nContext.i18nc("@info %1 is the name of an account", "What you wrote on it or attached to it stays, on the entry in %1.", prompt.incoming.account ?? "")
            wrapMode: Text.Wrap
        }

        // Explain, don't assume (AGENTS.md §6).
        Controls.Label {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            text: prompt.i18nContext.i18nc("@info", "If you join them, they count as moving your own money around, not as spending and income. Both accounts keep their balance and the dates they show. Undo takes it apart again.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: prompt.anyCategory
            text: prompt.i18nContext.i18nc("@info", "Undo also brings the categories back.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
