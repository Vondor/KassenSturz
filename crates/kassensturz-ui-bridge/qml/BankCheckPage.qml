// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// "Check against my bank": the user types what the bank says is in the
// account on a day, and KassenSturz says whether it agrees and, if not,
// where the difference can come from (D-029, D-111). It changes nothing by
// itself: it offers to correct the starting balance, and only the user's
// click does it (D-116). Every amount shows exactly, since a rounded one
// could hide the difference. Rust returns codes and amounts as text; every
// sentence is written here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountList accounts
    // The account's ID, and what the Rust side says about it.
    required property string accountId
    property var account: ({})
    readonly property bool debt: account.debt ?? false
    // The comparison, once the user asked for it.
    property var check: ({})
    readonly property bool checked: check.date !== undefined
    // The correction KassenSturz offers, if any, and whether one was made.
    readonly property var correction: check.correction ?? null
    property bool corrected: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The account doesn't change while this page is open.
    Component.onCompleted: account = accounts.details(accountId) ?? ({})

    title: i18nContext.i18nc("@title %1 is the name of an account", "Check %1 against my bank", account.name ?? "")
    header: FormError {
        id: error
    }

    function compare() {
        corrected = false;
        const result = accounts.checkAgainstBank(accountId, bankField.text, dateField.text);
        error.result = result;
        check = result.code === undefined ? result : ({});
    }

    // The user confirmed the offer: the starting balance changes so both
    // agree, and the comparison is shown again.
    function correct() {
        const result = accounts.correctStartingBalance(accountId, bankField.text, dateField.text);
        error.result = result;
        if (result.code === undefined) {
            compare();
            corrected = true;
        }
    }

    // What the bank or KassenSturz says: a debt reads "you owe".
    function says(amount: string, owes: bool): string {
        return owes ? i18nContext.i18nc("@label %1 is an amount of money", "you owe %1", amount) : amount;
    }

    // An amount of money that stands for itself in a sentence: a debt is
    // written as the amount owed.
    function money(amount: string, owes: bool): string {
        return owes ? i18nContext.i18nc("@label %1 is an amount of money", "%1 owed", amount) : amount;
    }

    // One possible reason for a difference, written for its code (D-034).
    function explanation(reason: var): string {
        const date = reason.date;
        const who = reason.other_party ?? "";
        switch (reason.code) {
        case "bank-check.later-entries":
            return i18nContext.i18ncp("@info %2 is a date", "%1 entry is dated after %2. Your bank's figure for %2 doesn't have it either, so it is left out on both sides.", "%1 entries are dated after %2. Your bank's figure for %2 doesn't have them either, so they are left out on both sides.", parseInt(reason.count), date);
        case "bank-check.starting-balance-later":
            return i18nContext.i18nc("@info %1 is an amount of money, %2 is a date", "The starting balance of %1 is dated %2, after the day you checked, so it isn't counted. Check a later day.", money(reason.amount, reason.owes === "true"), date);
        case "bank-check.entry-twice":
            return who === ""
                ? i18nContext.i18nc("@info %1 is an amount of money, %2 is a date", "The entry of %1 on %2 is as large as the difference. Maybe it is in your book twice, or your bank hasn't booked it yet.", reason.amount, date)
                : i18nContext.i18nc("@info %1 is an amount of money, %2 is a date, %3 is who it was paid to or by", "The entry of %1 on %2 (%3) is as large as the difference. Maybe it is in your book twice, or your bank hasn't booked it yet.", reason.amount, date, who);
        case "bank-check.entry-wrong-way":
            return who === ""
                ? i18nContext.i18nc("@info %1 is an amount of money, %2 is a date", "If the entry of %1 on %2 should be the other way round, money out instead of in or the other way, both figures would agree.", reason.amount, date)
                : i18nContext.i18nc("@info %1 is an amount of money, %2 is a date, %3 is who it was paid to or by", "If the entry of %1 on %2 (%3) should be the other way round, money out instead of in or the other way, both figures would agree.", reason.amount, date, who);
        case "bank-check.entry-missing":
            return reason.direction === "in"
                ? i18nContext.i18nc("@info %1 is an amount of money", "An entry of %1 that added money to the account may be missing. Look for it in your bank's statement.", reason.amount)
                : i18nContext.i18nc("@info %1 is an amount of money", "An entry of %1 that took money out of the account may be missing, or one of your entries is too much. Look for it in your bank's statement.", reason.amount);
        case "bank-check.starting-balance":
            return i18nContext.i18nc("@info %1 and %2 are amounts of money", "If the difference comes from before you started using KassenSturz, the starting balance would have to be %1 instead of %2 for both to agree. KassenSturz doesn't change it by itself.", money(reason.would_be, reason.owes === "true"), money(reason.current, reason.current_owes === "true"));
        case "bank-check.no-starting-balance":
            return i18nContext.i18nc("@info %1 is an amount of money", "This account has no starting balance. With a starting balance of %1, both would agree.", money(reason.would_be, reason.owes === "true"));
        default:
            return i18nContext.i18nc("@info %1 is an error code", "Something went wrong (%1). Please report this, so it can be fixed.", reason.code);
        }
    }

    // What the difference is, in words.
    function differenceText(): string {
        if (debt) {
            return check.bookHigher
                ? i18nContext.i18nc("@info %1 is an amount of money", "KassenSturz shows %1 less owed than your bank.", check.difference)
                : i18nContext.i18nc("@info %1 is an amount of money", "KassenSturz shows %1 more owed than your bank.", check.difference);
        }
        return check.bookHigher
            ? i18nContext.i18nc("@info %1 is an amount of money", "KassenSturz has %1 more than your bank.", check.difference)
            : i18nContext.i18nc("@info %1 is an amount of money", "KassenSturz has %1 less than your bank.", check.difference);
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.TextField {
                id: bankField
                Kirigami.FormData.label: page.debt
                    ? page.i18nContext.i18nc("@label:textbox", "What does your bank say you owe?")
                    : page.i18nContext.i18nc("@label:textbox", "What does your bank say is in it?")
                placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", page.accounts.amountExample(page.account.currency ?? ""))
                inputMethodHints: Qt.ImhFormattedNumbersOnly
                onAccepted: page.compare()
            }

            Controls.TextField {
                id: dateField
                Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "On which day?")
                placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is today's date", "Today, %1", page.accounts.today)
                onAccepted: page.compare()
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Compare")
                icon.name: "view-refresh"
                onClicked: page.compare()
            }
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.checked && page.check.matches
            type: Kirigami.MessageType.Positive
            text: !page.checked
                ? ""
                : page.corrected
                    ? page.i18nContext.i18nc("@info %1 is a date, %2 says how much money there is or what is owed", "The starting balance is corrected. On %1 both your bank and KassenSturz now say %2.", page.check.date, page.says(page.check.book, page.check.bookOwes))
                    : page.i18nContext.i18nc("@info %1 is a date, %2 says how much money there is or what is owed", "KassenSturz agrees with your bank. On %1 both say %2.", page.check.date, page.says(page.check.book, page.check.bookOwes))
        }

        ColumnLayout {
            Layout.fillWidth: true
            visible: page.checked && !page.check.matches
            spacing: Kirigami.Units.smallSpacing

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: parent.visible
                type: Kirigami.MessageType.Warning
                text: page.checked && !page.check.matches
                    ? page.i18nContext.i18nc("@info %1 is a date, %2 and %3 say how much money there is or what is owed", "On %1 your bank says %2. KassenSturz says %3.", page.check.date, page.says(page.check.bank, page.check.bankOwes), page.says(page.check.book, page.check.bookOwes)) + " " + page.differenceText()
                    : ""
            }

            Kirigami.Heading {
                Layout.topMargin: Kirigami.Units.largeSpacing
                text: page.i18nContext.i18nc("@title", "Where the difference can come from")
                level: 2
            }

            Repeater {
                model: page.checked ? page.check.explanations : []

                delegate: Controls.Label {
                    required property var modelData

                    Layout.fillWidth: true
                    text: "• " + page.explanation(modelData)
                    wrapMode: Text.Wrap
                    // The other party's name could contain anything.
                    textFormat: Text.PlainText
                }
            }

            // The offer (D-111): KassenSturz never changes the starting
            // balance by itself.
            Kirigami.Heading {
                Layout.topMargin: Kirigami.Units.largeSpacing
                visible: page.correction !== null
                text: page.i18nContext.i18nc("@title", "Correct the starting balance")
                level: 2
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: page.correction !== null
                text: page.correction === null
                    ? ""
                    : page.correction.adds
                        ? page.i18nContext.i18nc("@info %1 is an amount of money, %2 and %3 are dates", "If the difference comes from before you started using KassenSturz, a starting balance of %1 dated %2 makes both agree. KassenSturz then has on %3 what your bank says. Your other entries stay as they are.", page.money(page.correction.wouldBe, page.correction.owes), page.correction.date, page.check.date)
                        : page.i18nContext.i18nc("@info %1 is an amount of money, %2 and %3 are dates", "If the difference comes from before you started using KassenSturz, a starting balance of %1 (dated %2) makes both agree. KassenSturz then has on %3 what your bank says. Your other entries stay as they are.", page.money(page.correction.wouldBe, page.correction.owes), page.correction.date, page.check.date)
                wrapMode: Text.Wrap
            }

            Controls.Button {
                visible: page.correction !== null
                text: page.correction === null
                    ? ""
                    : page.correction.adds
                        ? page.i18nContext.i18nc("@action:button %1 is an amount of money", "Add a starting balance of %1", page.money(page.correction.wouldBe, page.correction.owes))
                        : page.i18nContext.i18nc("@action:button %1 is an amount of money", "Set the starting balance to %1", page.money(page.correction.wouldBe, page.correction.owes))
                icon.name: "dialog-ok-apply"
                onClicked: page.correct()
            }
        }
    }
}
