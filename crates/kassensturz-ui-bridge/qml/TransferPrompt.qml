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

    readonly property var suggestion: suggestions[current] ?? ({})
    readonly property var outgoing: suggestion.outgoing ?? ({})
    readonly property var incoming: suggestion.incoming ?? ({})

    // Every suggestion has been answered, or the dialog was closed.
    signal finished()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Asks about `found`, a list of suggestions; an error object or an empty
    // list asks nothing and finishes at once.
    function ask(found: var) {
        // An error comes back as an object without a length.
        if (!(found?.length > 0)) {
            finished();
            return;
        }
        error.result = ({});
        suggestions = found;
        current = 0;
        open();
    }

    // Joins the pair shown, or leaves it apart, and moves on.
    function answer(join: bool) {
        if (join) {
            const result = register.joinTransfer(outgoing.id, incoming.id);
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
            onTriggered: prompt.answer(true)
        },
        Kirigami.Action {
            text: prompt.i18nContext.i18nc("@action:button", "No, keep them apart")
            icon.name: "dialog-cancel"
            onTriggered: prompt.answer(false)
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
            Layout.fillWidth: true
            text: prompt.i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 a date", "%1 left %2 on %3.", prompt.suggestion.amount ?? "", prompt.outgoing.account ?? "", prompt.outgoing.date ?? "")
            wrapMode: Text.Wrap
            font.bold: true
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: (prompt.outgoing.otherParty ?? "") !== ""
            text: prompt.i18nContext.i18nc("@info %1 is a name that account shows for the money", "There it says: %1", prompt.outgoing.otherParty ?? "")
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            text: prompt.i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 a date", "%1 arrived in %2 on %3.", prompt.suggestion.amount ?? "", prompt.incoming.account ?? "", prompt.incoming.date ?? "")
            wrapMode: Text.Wrap
            font.bold: true
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: (prompt.incoming.otherParty ?? "") !== ""
            text: prompt.i18nContext.i18nc("@info %1 is a name that account shows for the money", "There it says: %1", prompt.incoming.otherParty ?? "")
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
    }
}
