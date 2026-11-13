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
// When the pair could be a monthly move, a box that starts unticked offers to
// join its later months by itself (US6.3, D-749); only the user's tick
// remembers it, and the join and the move are one Undo.
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
    // The monthly move the box offers for the pair shown; empty if none.
    property var offer: ({})
    readonly property bool offered: (offer.amount ?? "") !== ""
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
        refreshOffer();
        open();
    }

    // Asks the book whether the pair shown could be remembered as a monthly
    // move; the box always starts unticked (AGENTS.md: defaults never lower
    // protection).
    function refreshOffer() {
        rememberBox.checked = false;
        const found = outgoing.id !== undefined ? register.moveOffer(outgoing.id, incoming.id) : ({});
        offer = found?.code === undefined ? found : ({});
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
            const remember = offered && rememberBox.checked;
            const chosen = suggestion.partnerIsIncoming ? outgoing.id : incoming.id;
            const other = suggestion.partnerIsIncoming ? incoming.id : outgoing.id;
            let result;
            if (picked) {
                result = remember ? register.joinPickedRemembering(chosen, other) : register.joinPicked(chosen, other);
            } else {
                result = remember ? register.joinRemembering(outgoing.id, incoming.id) : register.joinTransfer(outgoing.id, incoming.id);
            }
            error.result = result;
            if (result.code !== undefined) {
                return;
            }
        }
        if (current + 1 < suggestions.length) {
            current += 1;
            refreshOffer();
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

        // A move that comes every month: the user may let KassenSturz join
        // it by itself from now on (D-749). Never ticked for them.
        Controls.CheckBox {
            id: rememberBox

            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            visible: prompt.offered
            text: prompt.i18nContext.i18nc("@option:check", "Next time, join this move by itself when it comes again around the same day of the month.")
            onToggled: {
                ActionLog.taken("TransferPrompt_rememberMove")
            }
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: prompt.offered
            text: prompt.i18nContext.i18nc("@info %1 is an amount, %2 and %3 the names of accounts, %4 a day of the month", "KassenSturz then joins entries of exactly %1 from “%2” to “%3” without asking, around day %4 of the month. After every import it lists what it joined, and Undo takes the joins back. You can stop it any time in Accounts › Monthly moves….", prompt.offer.amount ?? "", prompt.offer.from ?? "", prompt.offer.to ?? "", prompt.offer.day ?? 0)
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: prompt.offered && (prompt.offer.party ?? "") !== ""
            text: prompt.i18nContext.i18nc("@info %1 is the name the paying account shows for the money", "Only entries that say “%1” are joined.", prompt.offer.party ?? "")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
