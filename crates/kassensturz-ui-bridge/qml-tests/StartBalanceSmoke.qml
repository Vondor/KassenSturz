// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: starting balance change shown

// Opens the two pages that show a change to a starting balance (B-411,
// #1237, D-903) on stand-ins, with the objects as the Rust side builds
// them: the import preview's "Starting balance" line, and "Check against
// my bank" with the correction listed before and after. It checks that
// both read every field of the change, that a debt and an added starting
// balance read too, and that the click asks for the correction with the
// figures the page was opened with. `tools/check-panel.sh` runs it and
// fails on the first FAIL line or on any QML warning about a binding or a
// type.

import QtQuick
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 1200
    visible: true
    property bool failed: false

    // The change of B-411: entered for 19 November, worked out for the day
    // before the first transaction.
    readonly property var change: ({
        wouldBe: "734,50 €", owes: false, adds: false, date: "2026-08-19",
        was: "1.000,00 €", wasOwes: false, wasDate: "2026-11-19", moves: true
    })

    AccountList {
        id: accountList

        account: ({ name: "Checking account", currency: "EUR", debt: false })
        check: ({
            date: "2026-11-19", bank: "1.000,00 €", bankOwes: false,
            book: "1.265,50 €", bookOwes: false, debt: false, matches: false,
            difference: "265,50 €", bookHigher: true,
            explanations: [{ code: "bank-check.starting-balance", would_be: "734,50 €", owes: "false", current: "1.000,00 €", current_owes: "false" }],
            correction: window.change
        })
    }

    StatementImport {
        id: fakeImport

        view: ({
            fileName: "", fromBank: true, accountName: "Checking account", debt: false,
            entries: [], imported: 4, toDecide: 0, duplicates: 0, pending: 0, rules: 0, transfers: 0,
            balances: [{ which: "closing", date: "2026-11-19", bank: "1.000,00 €", bankOwes: false, book: "1.000,00 €", bookOwes: false, matches: true, difference: "0,00 €", bookHigher: false, checkAmount: "1.000,00 €", checkDate: "2026-11-19", stated: true }],
            start: Object.assign({ fromBank: true, holds: "1.000,00 €", holdsOwes: false, holdsOn: "2026-11-19" }, window.change)
        })
    }

    Component {
        id: previewPage

        ImportPreviewPage {
            statementImport: fakeImport
        }
    }

    pageStack.initialPage: BankCheckPage {
        id: checkPage

        accounts: accountList
        accountId: "a"
        initialBank: "1.000,00 €"
        initialDate: "2026-11-19"
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    Timer {
        interval: 300
        running: true
        onTriggered: {
            // Sent here with the figures, the page compares at once and has
            // the correction ready to confirm.
            window.check(checkPage.checked && checkPage.correction !== null, "the correction is offered at once");
            window.check(checkPage.correction.moves && checkPage.correction.was === "1.000,00 €" && checkPage.correction.wasDate === "2026-11-19", "the change has its before: " + JSON.stringify(checkPage.correction));
            checkPage.correct();
            window.check(accountList.corrected === "1.000,00 €|2026-11-19", "the click corrects with the figures shown, got " + accountList.corrected);

            const preview = window.pageStack.push(previewPage);
            window.check(preview.start !== null, "the preview has the starting balance change");
            const text = preview.startText(preview.start);
            window.check(text.includes("1.000,00 €") && text.includes("2026-11-19") && text.includes("734,50 €") && text.includes("2026-08-19"), "before and after are named: " + text);
            const why = preview.startDescription(preview.start);
            window.check(why.includes("1.000,00 €") && why.includes("2026-11-19"), "the bank's balance is named: " + why);

            // A credit card that gets its first starting balance.
            const added = preview.startText({ wouldBe: "300,00 €", owes: true, adds: true, date: "2026-10-11", was: "", wasOwes: false, wasDate: "", moves: false });
            window.check(added.includes("300,00 €") && added.includes("2026-10-11"), "an added starting balance is named: " + added);
            const stated = preview.startDescription({ fromBank: false, holds: "345,65 €", holdsOwes: true, holdsOn: "2026-11-19" });
            window.check(stated.includes("345,65 €"), "a stated debt is named: " + stated);

            // Without a change the section stays away.
            fakeImport.view = ({ accountName: "Checking account", entries: [], balances: [] });
            window.check(preview.start === null, "no change, no section");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
