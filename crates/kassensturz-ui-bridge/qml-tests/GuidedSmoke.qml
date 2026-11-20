// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: guided setup, start choice, steps 1, 2, 4 to 6 and the last page

// Opens the pages of the guided setup (#1031, docs/GUIDED_SETUP.md) on
// stand-ins for FirstStart and AccountList (fakes/) and checks:
//
// - the start page offers "Guided setup" and still every choice it had
//   (D-899);
// - the accounts page in the guided setup shows no amount field, and
//   "Continue" writes the accounts with empty balances (D-895);
// - step 1 hides the bank where it isn't offered, imports straight into the
//   only account and asks which one when there are two (D-911);
// - step 2 shows one box per open account with its exact balance and hands
//   the typed bank figure to the check page (D-911);
// - every step has Skip and "End the setup" (D-911, D-342), and a step
//   opened from Help has neither;
// - steps 4 to 6 each offer the real function to try and the last page
//   offers the wiki and leads to the overview (D-900 to D-902, D-931).
//
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 900
    visible: true
    property bool failed: false
    property var seen: []

    FirstStart {
        id: start
    }

    AccountList {
        id: oneAccount

        all: [{ id: "giro", name: "Checking account", currency: "EUR", exactBalance: "2.340,17 €", owes: false, closed: false }]
    }

    AccountList {
        id: twoAccounts

        all: [
            { id: "giro", name: "Checking account", currency: "EUR", exactBalance: "2.340,17 €", owes: false, closed: false,
              stated: { exactBalance: "2.340,17 €", date: "18.11.2026", exactHeld: "2.340,17 €", matches: true } },
            { id: "card", name: "Credit card", currency: "EUR", exactBalance: "412,30 €", owes: true, closed: false,
              stated: { exactBalance: "400,00 €", date: "18.11.2026", exactHeld: "412,30 €", matches: false } },
            { id: "old", name: "Old account", currency: "EUR", exactBalance: "0,00 €", owes: false, closed: true }
        ]
    }

    Component {
        id: startPage

        WizardStartPage {
            onGuidedClicked: window.seen.push("guided")
        }
    }

    Component {
        id: accountsPage

        WizardAccountsPage {
            firstStart: start
            accounts: oneAccount
            currency: "EUR"
            guided: true
            guidedStep: 1
            guidedCount: 2
            onFinished: window.seen.push("finished")
            onGuidedEnded: window.seen.push("ended-accounts")
        }
    }

    Component {
        id: transactionsPage

        GuidedTransactionsPage {
            bankOffered: false
            onImportClicked: accountId => window.seen.push("import:" + accountId)
            onConnectBankClicked: window.seen.push("bank")
            onNextClicked: window.seen.push("next")
            onSkipClicked: window.seen.push("skip")
            onEndClicked: window.seen.push("end")
        }
    }

    Component {
        id: balancePage

        GuidedBalancePage {
            accounts: twoAccounts
            onCompareClicked: (accountId, bank) => window.seen.push("compare:" + accountId + ":" + bank)
            onNextClicked: window.seen.push("next")
            onSkipClicked: window.seen.push("skip")
            onEndClicked: window.seen.push("end")
        }
    }

    Component {
        id: tryPage

        GuidedTryPage {
            onTryClicked: what => window.seen.push("try:" + what)
            onNextClicked: window.seen.push("next:" + which)
            onSkipClicked: window.seen.push("skip:" + which)
            onEndClicked: window.seen.push("end:" + which)
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function find(item, test, out) {
        if (!item) {
            return out;
        }
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (test(child)) {
                out.push(child);
            }
            find(child, test, out);
        }
        return out;
    }

    // Everything of a page; its header and its footer are among its children.
    function parts(page, test) {
        return find(page, test, []);
    }

    function buttons(page, text) {
        return parts(page, child => typeof child.clicked === "function" && child.down !== undefined && (child.text === text || child.Accessible.name === text));
    }

    function took(what) {
        return seen.indexOf(what) >= 0;
    }

    // The pages, made first: a page finishes building only after the push
    // that shows it has returned.
    property var first
    property var accounts
    property var one
    property var two
    property var balance
    property var tries: ({})

    Component.onCompleted: {
        first = pageStack.push(startPage);
        accounts = pageStack.push(accountsPage);
        one = pageStack.push(transactionsPage, { accounts: oneAccount, step: 1, count: 2 });
        two = pageStack.push(transactionsPage, { accounts: twoAccounts, bankOffered: true, step: 0, count: 0 });
        balance = pageStack.push(balancePage, { step: 2, count: 2 });
        const made = {};
        for (const [which, step] of [["chart", 4], ["budget", 5], ["start", 6], ["done", 0]]) {
            made[which] = pageStack.push(tryPage, { which: which, step: step, count: step === 0 ? 0 : 6 });
        }
        tries = made;
    }

    Timer {
        interval: 1000
        running: true
        onTriggered: {
            const first = window.first;
            const accounts = window.accounts;
            const one = window.one;
            const two = window.two;
            const balance = window.balance;
            // The start page: the guided choice, and every choice it had.
            for (const name of ["Guided setup", "My own accounts", "Try an example book", "Bring my data from another program", "Open a KassenSturz book I already have…"]) {
                window.check(window.buttons(first, name).length === 1, "the start page offers “" + name + "”, got " + window.buttons(first, name).length);
            }
            window.buttons(first, "Guided setup")[0].clicked();
            window.check(window.took("guided"), "Guided setup is asked for");

            // The accounts page of the guided setup: no amount, its own words.
            const amountFields = window.find(accounts, child => child.placeholderText !== undefined && child.Accessible.name === "How much is in it today?", []);
            window.check(amountFields.length === 1 && amountFields[0].visible === false, "the amount field is hidden in the guided setup");
            window.check(window.buttons(accounts, "Create my household book").length === 0, "no “Create my household book” in the guided setup");
            window.check(window.parts(accounts, child => child.text === "Step 1 of 2").length === 1, "the accounts page says Step 1 of 2");
            const go = window.buttons(accounts, "Continue");
            window.check(go.length === 1, "one Continue on the accounts page, got " + go.length);
            go[0].clicked();
            window.check(window.took("finished"), "Continue finishes the accounts page");
            window.check(start.written.length === 5, "one account is written, got " + start.written.length / 5);
            window.check(start.written[3] === "" && start.written[4] === "", "the account is written without a balance or a day, got “" + start.written[3] + "”, “" + start.written[4] + "”");
            window.check(start.writtenCurrency === "EUR", "the household's currency is written, got " + start.writtenCurrency);
            window.buttons(accounts, "End the setup")[0].clicked();
            window.check(window.took("ended-accounts"), "End the setup on the accounts page is told");

            // Step 1 with one account: no bank where it isn't offered, the
            // file goes straight into the only account.
            const bank = window.buttons(one, "Connect to my bank");
            window.check(bank.length === 1 && bank[0].visible === false, "the bank is hidden where it isn't offered");
            window.buttons(one, "Import a file from my bank")[0].clicked();
            window.check(window.took("import:giro"), "one account: the file is for it");
            window.check(window.parts(one, child => child.text === "Step 1 of 2").length === 1, "step 1 says Step 1 of 2");
            window.buttons(one, "Skip")[0].clicked();
            window.buttons(one, "Continue")[0].clicked();
            window.buttons(one, "End the setup")[0].clicked();
            window.check(window.took("skip") && window.took("next") && window.took("end"), "step 1 has Skip, Continue and End the setup, got " + window.seen);

            // Step 1 with two open accounts and the bank: it asks which one.
            window.seen = [];
            window.check(window.buttons(two, "Connect to my bank")[0].visible === true, "the bank is offered where it is");
            window.buttons(two, "Connect to my bank")[0].clicked();
            window.check(window.took("bank"), "the bank setup is asked for");
            window.buttons(two, "Import a file from my bank")[0].clicked();
            window.check(!window.seen.some(what => what.startsWith("import:")), "two accounts: nothing is imported before the choice");
            window.check(two.choosingAccount === true, "two accounts: the page asks which one");
            // Opened from Help: no step count, nothing to skip, "Done".
            window.check(two.header.visible === false, "from Help there is no step row");
            window.check(window.buttons(two, "Skip")[0].visible === false, "from Help there is no Skip");
            window.check(window.buttons(two, "Done").length === 1, "from Help the main button says Done");

            // Step 2: one box per open account, exact balances, the bank's
            // figure handed on as typed.
            window.seen = [];
            window.check(balance.ownAccounts.length === 2, "two open accounts are shown, got " + balance.ownAccounts.length);
            const texts = window.find(balance, child => child.text !== undefined, []).map(child => child.text);
            window.check(texts.indexOf("2.340,17 €") >= 0, "the checking account's exact balance is shown");
            window.check(texts.indexOf("You owe 412,30 €") >= 0, "the credit card says what is owed");
            window.check(texts.indexOf("Your bank showed 2.340,17 € on 18.11.2026, and that is what KassenSturz has for that day.") >= 0, "a stated balance that matches is said");
            window.check(texts.indexOf("Your bank showed 400,00 € on 18.11.2026, but KassenSturz has 412,30 € for that day.") >= 0, "a stated balance that differs is said");
            window.check(texts.indexOf("Old account") < 0, "a closed account is left out");
            const fields = window.find(balance, child => child.placeholderText !== undefined && child.inputMethodHints !== undefined, []);
            window.check(fields.length === 2, "one bank field per account, got " + fields.length);
            fields[1].text = "400,00";
            window.buttons(balance, "Compare…")[1].clicked();
            window.check(window.took("compare:card:400,00"), "the typed figure goes to the check page, got " + window.seen);
            window.check(window.parts(balance, child => child.text === "Step 2 of 2").length === 1, "step 2 says Step 2 of 2");
            window.buttons(balance, "Yes, that's right")[0].clicked();
            window.buttons(balance, "Skip")[0].clicked();
            window.buttons(balance, "End the setup")[0].clicked();
            window.check(window.took("skip") && window.took("next") && window.took("end"), "step 2 has Skip, its main button and End the setup, got " + window.seen);

            // Steps 4 to 6: each opens the real thing, and can be skipped.
            window.seen = [];
            const offered = { chart: ["Build my first chart"], budget: ["Set a budget", "Plan a payment"], start: ["Arrange my start page"] };
            const asks = { "Build my first chart": "chart", "Set a budget": "budget", "Plan a payment": "plan", "Arrange my start page": "arrange" };
            for (const which of ["chart", "budget", "start"]) {
                const page = window.tries[which];
                const visible = window.parts(page, child => typeof child.clicked === "function" && child.down !== undefined && child.heading !== undefined && child.visible).map(child => child.heading);
                window.check(visible.join("|") === offered[which].join("|"), which + " offers " + offered[which] + ", got " + visible);
                for (const heading of offered[which]) {
                    window.parts(page, child => child.heading === heading)[0].clicked();
                    window.check(window.took("try:" + asks[heading]), heading + " asks for " + asks[heading]);
                }
                window.buttons(page, "Skip")[0].clicked();
                window.buttons(page, "Continue")[0].clicked();
                window.buttons(page, "End the setup")[0].clicked();
                window.check(["skip:", "next:", "end:"].every(what => window.took(what + which)), which + " has Skip, Continue and End the setup, got " + window.seen);
            }
            window.check(window.parts(window.tries.start, child => child.text !== undefined && child.visible && /^[1-4]\. /.test(child.text)).length === 4, "the start page step names four things to try");
            window.check(window.parts(window.tries.chart, child => child.text === "Step 4 of 6").length === 1, "the chart step says Step 4 of 6");

            // The last page: the wiki, no step row, on to the overview.
            const last = window.tries.done;
            window.parts(last, child => child.heading === "More help in the wiki")[0].clicked();
            window.check(window.took("try:wiki"), "the last page offers the wiki");
            window.check(last.header.visible === false, "the last page has no step row");
            window.check(window.buttons(last, "Skip")[0].visible === false, "the last page has nothing to skip");
            window.buttons(last, "Go to my overview")[0].clicked();
            window.check(window.took("next:done"), "the last page leads on");
            window.check(window.parts(last, child => child.text !== undefined && child.visible && child.text.indexOf("Help: guided setup") >= 0).length >= 1, "the last page says where the steps are found again");

            console.log(window.failed ? "guided setup: FAILED" : "guided setup: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
