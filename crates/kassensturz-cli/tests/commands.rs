// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs whole command lines against book files in a temporary folder. All
//! names and amounts are made up (AGENTS.md §3.1 rule 2).

use std::path::{Path, PathBuf};
use std::process::Command;

use kassensturz_cli::{Environment, run};
use kassensturz_core::{AccountId, Timestamp};
use kassensturz_store::{Book, Computer};

/// A folder of its own for one test, removed again afterwards, with a book
/// path and a backup folder in it.
struct Session(PathBuf);

/// What one command line printed and returned.
struct Outcome {
    status: u8,
    out: String,
    err: String,
}

impl Session {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("kassensturz-cli-test-{}", AccountId::generate()));
        std::fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    fn book(&self) -> PathBuf {
        self.0.join("Household.ksz")
    }

    fn backups(&self) -> PathBuf {
        self.0.join("Backups")
    }

    /// Runs the command with this session's book on a computer called
    /// "Laptop".
    fn cli(&self, args: &[&str]) -> Outcome {
        let mut line = vec![
            "--book".to_owned(),
            self.book().display().to_string(),
            "--backup-folder".to_owned(),
            self.backups().display().to_string(),
            "--computer".to_owned(),
            "Laptop".to_owned(),
        ];
        line.extend(args.iter().map(|arg| (*arg).to_owned()));
        run_line(&line, &Environment::default())
    }

    /// Runs the command, which must succeed, and returns what it printed.
    fn ok(&self, args: &[&str]) -> String {
        let outcome = self.cli(args);
        assert_eq!(outcome.status, 0, "{args:?} failed: {}", outcome.err);
        assert_eq!(outcome.err, "", "{args:?} printed an error");
        outcome.out
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_line(line: &[String], environment: &Environment) -> Outcome {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run(line, environment, &mut out, &mut err);
    Outcome {
        status,
        out: String::from_utf8(out).expect("UTF-8 output"),
        err: String::from_utf8(err).expect("UTF-8 errors"),
    }
}

/// A book with two accounts and a few categories, as the first steps of a
/// household would make it.
fn household() -> Session {
    let session = Session::new();
    session.ok(&["create"]);
    session.ok(&[
        "account",
        "add",
        "Checking account",
        "--type",
        "checking",
        "--currency",
        "EUR",
    ]);
    session.ok(&[
        "account",
        "add",
        "Savings",
        "--type",
        "savings",
        "--currency",
        "eur",
    ]);
    session.ok(&[
        "category",
        "add",
        "Salary",
        "--kind",
        "income",
        "--currency",
        "EUR",
    ]);
    session.ok(&[
        "category",
        "add",
        "Food",
        "--kind",
        "expense",
        "--currency",
        "EUR",
    ]);
    session.ok(&["category", "add", "Groceries", "--inside", "Food"]);
    session.ok(&["category", "add", "Other", "--inside", "Food"]);
    session.ok(&[
        "category",
        "add",
        "Home",
        "--kind",
        "expense",
        "--currency",
        "EUR",
    ]);
    session.ok(&["category", "add", "Other", "--inside", "Home"]);
    session
}

#[test]
fn a_household_from_an_empty_book_to_its_register_and_balances() {
    let session = household();
    assert_eq!(
        session.ok(&["account", "list"]),
        "Name              Type      Currency  Status\n\
         Checking account  checking  EUR       open\n\
         Savings           savings   EUR       open\n"
    );
    assert_eq!(
        session.ok(&["category", "list"]),
        "Category     Kind     Currency\n\
         Salary       income   EUR\n\
         Food         expense  EUR\n\
         \x20 Groceries  expense  EUR\n\
         \x20 Other      expense  EUR\n\
         Home         expense  EUR\n\
         \x20 Other      expense  EUR\n"
    );

    let args = |date: &'static str, amount: &'static str| {
        [
            "transaction",
            "add",
            "--date",
            date,
            "--account",
            "Checking account",
            "--amount",
            amount,
        ]
    };
    let mut pay = args("2026-10-01", "2500.00").to_vec();
    pay.extend(["--category", "Salary", "--payee", "Example Employer"]);
    session.ok(&pay);
    let mut shop = args("2026-10-05", "-53.20").to_vec();
    shop.extend(["--category", "Food/Groceries", "--payee", "Corner Grocery"]);
    session.ok(&shop);
    let mut save = args("2026-10-06", "-100").to_vec();
    save.extend(["--to", "Savings"]);
    session.ok(&save);
    // Split in two Parts, one of them left open for the remainder (D-017).
    let mut split = args("2026-10-07", "-60.00").to_vec();
    split.extend([
        "--part",
        "groceries=-35.50",
        "--part",
        "home/other=",
        "--payee",
        "Big Store",
    ]);
    let split_id = last_added(&session.ok(&split));
    // No category yet (D-083).
    session.ok(&args("2026-10-08", "-9.99"));

    let register = session.ok(&["register", "Checking account"]);
    assert_eq!(
        without_ids(&register),
        "Checking account\n\
         Date        Payee             Amount (EUR)  Balance (EUR)\n\
         2026-10-01  Example Employer       2500.00        2500.00\n\
         2026-10-05  Corner Grocery          -53.20        2446.80\n\
         2026-10-06                         -100.00        2346.80\n\
         2026-10-07  Big Store               -60.00        2286.80\n\
         2026-10-08                           -9.99        2276.81\n"
    );
    assert_eq!(
        session.ok(&["balances"]),
        "Account               Balance\n\
         Checking account  2276.81 EUR\n\
         Savings            100.00 EUR\n\
         Total             2376.81 EUR\n"
    );
    // Both Parts of the split show up in their categories.
    assert!(
        session
            .ok(&["register", "Food/Groceries"])
            .contains("35.50")
    );
    assert!(session.ok(&["register", "Home/Other"]).contains("24.50"));
    assert_eq!(
        session
            .ok(&["transaction", "show", &split_id])
            .lines()
            .skip(1)
            .collect::<Vec<_>>(),
        [
            "Date:  2026-10-07",
            "Payee: Big Store",
            "Parts:",
            "  Checking account  -60.00 EUR",
            "  Groceries          35.50 EUR",
            "  Other              24.50 EUR",
        ]
    );

    // Removing a transaction takes it out of every register and balance.
    session.ok(&["transaction", "remove", &split_id]);
    assert!(
        !session
            .ok(&["register", "Checking account"])
            .contains("Big Store")
    );
    assert!(session.ok(&["balances"]).contains("2336.81 EUR"));
}

/// The ID in "Added the transaction <id>."
fn last_added(output: &str) -> String {
    output
        .trim_end_matches(".\n")
        .rsplit(' ')
        .next()
        .expect("an ID")
        .to_owned()
}

/// The register without its last column, which holds the random IDs.
fn without_ids(register: &str) -> String {
    register
        .lines()
        .map(|line| match line.rsplit_once("  ") {
            Some((rest, id)) if id.len() == 12 => rest.trim_end().to_owned(),
            _ if line.ends_with("ID") => line.trim_end_matches("ID").trim_end().to_owned(),
            _ => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn a_part_in_another_currency_is_converted_with_the_price_table() {
    let session = household();
    session.ok(&[
        "category",
        "add",
        "Travel",
        "--kind",
        "expense",
        "--currency",
        "USD",
    ]);
    let add = [
        "transaction",
        "add",
        "--date",
        "2026-10-02",
        "--account",
        "Checking account",
        "--amount",
        "-100.00",
        "--category",
        "Travel",
        "--payee",
        "Hotel",
    ];
    // Without a rate there is nothing to convert with, and nothing is added.
    let missing = session.cli(&add);
    assert_eq!(missing.status, 1);
    assert!(
        missing.err.starts_with("error: store.no-rate\n"),
        "{}",
        missing.err
    );
    assert!(
        missing
            .err
            .contains("from: EUR\n  to: USD\n  date: 2026-10-02\n")
    );

    session.ok(&[
        "rate",
        "add",
        "--date",
        "2026-10-01",
        "--from",
        "EUR",
        "--to",
        "USD",
        "--rate",
        "1.0856",
    ]);
    let duplicate = session.cli(&[
        "rate",
        "add",
        "--date",
        "2026-10-01",
        "--from",
        "EUR",
        "--to",
        "USD",
        "--rate",
        "1.1",
    ]);
    assert_eq!(duplicate.status, 1);
    assert!(duplicate.err.starts_with("error: store.duplicate-rate\n"));
    assert_eq!(
        session.ok(&["rate", "list", "--from", "EUR", "--to", "USD"]),
        "Date        1 EUR in USD  Source\n2026-10-01        1.0856  entered by hand\n"
    );

    let added = session.ok(&add);
    // The category counts in dollars, the account in euros.
    let travel = session.ok(&["register", "Travel"]);
    assert!(
        travel.contains("Amount (USD)") && travel.contains("108.56"),
        "{travel}"
    );
    assert!(
        session
            .ok(&["register", "Checking account"])
            .contains("-100.00")
    );
    let shown = session.ok(&["transaction", "show", &last_added(&added)]);
    assert!(
        shown.contains(
            "  Travel             100.00 EUR  = 108.56 USD at 1 EUR = 1.0856 USD (2026-10-01, entered by hand)"
        ),
        "{shown}"
    );

    session.ok(&[
        "rate",
        "remove",
        "--date",
        "2026-10-01",
        "--from",
        "EUR",
        "--to",
        "USD",
    ]);
    assert_eq!(
        session.ok(&["rate", "list", "--from", "EUR", "--to", "USD"]),
        "Date  1 EUR in USD  Source\n"
    );
    // The transaction keeps the rate it was made with (D-043).
    assert!(session.ok(&["register", "Travel"]).contains("108.56"));
}

#[test]
fn accounts_can_be_renamed_closed_and_reopened() {
    let session = household();
    session.ok(&["account", "rename", "savings", "Holiday fund"]);
    session.ok(&["account", "close", "Holiday fund"]);
    assert!(!session.ok(&["account", "list"]).contains("Holiday"));
    assert!(
        session
            .ok(&["account", "list", "--all"])
            .contains("Holiday fund      savings   EUR       closed")
    );
    // A closed account takes no new transactions.
    let refused = session.cli(&[
        "transaction",
        "add",
        "--date",
        "2026-10-02",
        "--account",
        "Holiday fund",
        "--amount",
        "5",
    ]);
    assert_eq!(refused.status, 1);
    assert_eq!(
        refused.err,
        "error: store.account-closed\n  account: Holiday fund\n"
    );
    session.ok(&["account", "reopen", "Holiday fund"]);
    assert!(session.ok(&["balances"]).contains("Holiday fund"));
}

#[test]
fn categories_can_be_moved() {
    let session = household();
    session.ok(&["category", "move", "Home/Other", "--inside", "Food/Other"]);
    assert!(
        session
            .ok(&["category", "list"])
            .contains("\n    Other    expense  EUR\n")
    );
    session.ok(&["category", "move", "Food/Other/Other", "--inside", "Home"]);
    let refused = session.cli(&["category", "move", "Food", "--inside", "Food/Groceries"]);
    assert_eq!(refused.status, 1);
    assert!(
        refused
            .err
            .starts_with("error: store.category-inside-itself\n")
    );
}

#[test]
fn mistakes_in_a_transaction_are_refused_without_changing_the_book() {
    let session = household();
    let add = |date: &str, amount: &str, extra: &[&str]| {
        let mut line = vec![
            "transaction",
            "add",
            "--date",
            date,
            "--account",
            "Savings",
            "--amount",
            amount,
        ];
        line.extend(extra);
        session.cli(&line)
    };
    // The parts must add up to the amount (D-017): 3 of 9 isn't all of it.
    let unbalanced = add("2026-10-02", "-9", &["--part", "Groceries=-3"]);
    assert_eq!(unbalanced.status, 1);
    assert_eq!(
        unbalanced.err,
        "error: transaction.unbalanced\n  total: -6.00 EUR\n"
    );
    // Names that fit more than one category must be spelled out.
    let ambiguous = add("2026-10-02", "-9", &["--category", "Other"]);
    assert_eq!(ambiguous.status, 1);
    assert!(
        ambiguous.err.contains("Food/Other, Home/Other"),
        "{}",
        ambiguous.err
    );
    // Only one part can be left open, and the ways to name the other side
    // don't mix.
    let two_open = add(
        "2026-10-02",
        "-9",
        &["--part", "Groceries=", "--part", "Home/Other="],
    );
    assert_eq!(two_open.status, 2);
    let mixed = add(
        "2026-10-02",
        "-9",
        &["--part", "Groceries=-9", "--category", "Salary"],
    );
    assert_eq!(mixed.status, 2);
    // Amounts are never rounded for the user, and dates have one form only.
    assert_eq!(add("2026-10-02", "-9.999", &[]).status, 1);
    assert_eq!(add("02.10.2026", "-9", &[]).status, 1);

    assert_eq!(session.ok(&["register", "Savings"]).lines().count(), 2);
    let balances = session.ok(&["balances"]);
    let total = balances.lines().last().expect("a total line");
    assert_eq!(
        total.split_whitespace().collect::<Vec<_>>(),
        ["Total", "0.00", "EUR"]
    );
}

#[test]
fn both_sides_of_a_transfer_are_suggested_and_joined() {
    let session = household();
    // Each account's side, with no category yet, as two bank statements
    // would show them: out on Friday, in on Monday.
    let out = last_added(&session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-09",
        "--account",
        "Checking account",
        "--amount",
        "-200",
        "--payee",
        "To savings",
    ]));
    let into = last_added(&session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-12",
        "--account",
        "Savings",
        "--amount",
        "200",
    ]));
    // A purchase of the same amount, a week later, doesn't fit.
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-19",
        "--account",
        "Checking account",
        "--amount",
        "-200",
    ]);
    let suggested = session.ok(&["transfer", "suggest"]);
    let lines: Vec<&str> = suggested.lines().collect();
    assert_eq!(lines.len(), 3, "{suggested}");
    assert_eq!(
        lines[1].split_whitespace().collect::<Vec<_>>(),
        [
            "2026-10-09",
            "Checking",
            "account",
            "2026-10-12",
            "Savings",
            "200.00",
            "EUR",
            out.as_str(),
            into.as_str()
        ]
    );

    // Joining needs the two sides of one Transfer.
    let refused = session.cli(&["transfer", "join", &out, &out]);
    assert_eq!(refused.status, 1);
    assert!(
        refused
            .err
            .starts_with("error: transfer.same-transaction\n"),
        "{}",
        refused.err
    );
    assert_eq!(
        session.ok(&["transfer", "join", &into, &out]),
        format!("Joined them into the Transfer {out}.\n")
    );
    assert_eq!(
        session.ok(&["transfer", "suggest"]),
        "No transactions look like the two sides of a Transfer.\n"
    );
    let shown = session.ok(&["transaction", "show", &out]);
    assert!(shown.contains("Checking account  -200.00 EUR"), "{shown}");
    assert!(shown.contains("Savings            200.00 EUR"), "{shown}");
    assert!(!shown.contains("no category yet"), "{shown}");
    assert_eq!(session.ok(&["register", "Savings"]).lines().count(), 3);

    // Undo brings back both sides.
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: joined two transactions into a Transfer.\n"
    );
    assert_eq!(session.ok(&["transfer", "suggest"]), suggested);
}

#[test]
fn rules_book_new_transactions_and_leave_the_users_choices_alone() {
    let session = household();
    session.ok(&[
        "rule",
        "add",
        "Supermarket",
        "--if",
        "payee contains market",
        "--if",
        "amount out",
        "--category",
        "Food/Groceries",
    ]);
    session.ok(&[
        "rule",
        "add",
        "Big shop",
        "--inside",
        "Supermarket",
        "--if",
        "amount between -1000 -100 EUR",
        "--payee",
        "Supermarket (big shop)",
    ]);
    session.ok(&[
        "rule",
        "add",
        "Savings plan",
        "--if",
        "payee is Monthly savings",
        "--to",
        "Savings",
    ]);
    let listed = session.ok(&["rule", "list"]);
    let lines: Vec<&str> = listed.lines().collect();
    assert_eq!(lines.len(), 9, "{listed}");
    assert!(lines[0].starts_with("1. Supermarket  (ID "), "{listed}");
    assert_eq!(
        lines[1],
        "   when all of: payee contains \"market\"; amount out"
    );
    assert_eq!(lines[2], "   then: category Groceries");
    assert!(lines[3].starts_with("  1. Big shop  (ID "), "{listed}");
    assert_eq!(
        lines[4],
        "     when all of: amount between -1000.00 EUR and -100.00 EUR"
    );
    assert_eq!(lines[5], "     then: payee \"Supermarket (big shop)\"");
    assert_eq!(lines[8], "   then: Transfer to Savings");

    let small = last_added(&session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-02",
        "--account",
        "Checking account",
        "--amount",
        "-23.40",
        "--payee",
        "Corner Market",
    ]));
    let big = last_added(&session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-03",
        "--account",
        "Checking account",
        "--amount",
        "-120",
        "--payee",
        "Corner Market",
    ]));
    let saved = last_added(&session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-05",
        "--account",
        "Checking account",
        "--amount",
        "-50",
        "--payee",
        "Monthly savings",
    ]));
    // Booked by the user, so no Rule may change it.
    let chosen = last_added(&session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-06",
        "--account",
        "Checking account",
        "--amount",
        "-8",
        "--payee",
        "Corner Market",
        "--category",
        "Food/Other",
    ]));

    let preview = session.ok(&["rule", "run", "--dry-run"]);
    assert!(
        preview.starts_with("Would change 3 transaction(s):\n"),
        "{preview}"
    );
    assert!(preview.ends_with("Nothing was changed.\n"), "{preview}");
    assert!(!preview.contains(&chosen), "{preview}");
    assert!(
        session
            .ok(&["transaction", "show", &small])
            .contains("(no category yet)")
    );

    let run = session.ok(&["rule", "run"]);
    assert!(run.starts_with("Changed 3 transaction(s):\n"), "{run}");
    let row = run
        .lines()
        .find(|line| line.contains(&big))
        .expect("big shop");
    assert!(row.contains("Supermarket (big shop)"), "{run}");
    assert!(row.contains("Big shop"), "{run}");

    let shown = session.ok(&["transaction", "show", &small]);
    assert!(shown.contains("Groceries          23.40 EUR"), "{shown}");
    assert!(
        shown.ends_with("Booked by the Rule Supermarket.\n"),
        "{shown}"
    );
    let shown = session.ok(&["transaction", "show", &big]);
    assert!(shown.contains("Payee: Supermarket (big shop)"), "{shown}");
    let shown = session.ok(&["transaction", "show", &saved]);
    assert!(shown.contains("Savings            50.00 EUR"), "{shown}");
    let shown = session.ok(&["transaction", "show", &chosen]);
    assert!(shown.contains("Other"), "{shown}");
    assert!(!shown.contains("Booked by"), "{shown}");
    assert_eq!(
        session.ok(&["rule", "run"]),
        "The Rules change no transaction.\n"
    );

    // A changed Rule keeps what it booked before (Q-044)...
    let to_other = [
        "rule",
        "change",
        "Supermarket",
        "--if",
        "payee contains market",
        "--category",
        "Food/Other",
    ];
    assert_eq!(session.ok(&to_other), "Changed the Rule Supermarket.\n");
    assert_eq!(
        session.ok(&["rule", "run"]),
        "The Rules change no transaction.\n"
    );
    assert!(
        session
            .ok(&["transaction", "show", &small])
            .contains("Groceries          23.40 EUR")
    );
    // ...unless it is asked to book that again.
    let preview = session.ok(&[&to_other[..], &["--rebook", "--dry-run"][..]].concat());
    assert!(
        preview.starts_with("Would change 2 transaction(s):\n"),
        "{preview}"
    );
    assert!(preview.ends_with("Nothing was changed.\n"), "{preview}");
    let rebooked = session.ok(&[&to_other[..], &["--rebook"][..]].concat());
    assert!(
        rebooked.starts_with("Changed the Rule Supermarket.\nChanged 2 transaction(s):\n"),
        "{rebooked}"
    );
    assert!(
        session
            .ok(&["transaction", "show", &small])
            .contains("Other              23.40 EUR")
    );
    let outcome = session.cli(&[&to_other[..], &["--dry-run"][..]].concat());
    assert_eq!(outcome.status, 2, "{}", outcome.err);

    // The change and what it booked again are one change.
    assert_eq!(session.ok(&["undo"]), "Undid the change: changed a Rule.\n");
    assert!(
        session
            .ok(&["transaction", "show", &small])
            .contains("Groceries          23.40 EUR")
    );

    session.ok(&["rule", "move", "Savings plan", "--position", "1"]);
    assert!(
        session
            .ok(&["rule", "list"])
            .starts_with("1. Savings plan  (ID ")
    );
    assert_eq!(
        session.ok(&["rule", "move", "Big shop", "--inside", "Savings plan"]),
        "Moved Big shop inside Savings plan.\n"
    );
    assert!(
        session
            .ok(&["rule", "list"])
            .contains("  1. Big shop  (ID ")
    );
    assert_eq!(
        session.ok(&["rule", "remove", "Supermarket"]),
        "Removed the Rule Supermarket and its sub-Rules.\n"
    );
    let shown = session.ok(&["transaction", "show", &small]);
    assert!(
        shown.ends_with("Booked by a Rule that was removed since.\n"),
        "{shown}"
    );
}

/// A statement file in the made-up test format, in this session's folder.
fn statement_file(session: &Session, name: &str, records: &[&str]) -> String {
    let path = session.0.join(name);
    std::fs::write(&path, format!("KSTEST\n{}\n", records.join("\n"))).expect("file written");
    path.display().to_string()
}

#[test]
fn a_statement_file_is_imported_once_and_undone_as_a_whole() {
    let session = household();
    // Typed in on Friday, before the statement came.
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-09",
        "--account",
        "Checking account",
        "--amount",
        "-25",
        "--payee",
        "Bakery",
    ]);
    session.ok(&[
        "rule",
        "add",
        "Shops",
        "--if",
        "other-party contains REWE",
        "--category",
        "Food/Groceries",
    ]);
    // The IBAN is the example for Germany in SWIFT's IBAN Registry (D-055).
    let file = statement_file(
        &session,
        "october.kstest",
        &[
            "account;DE89370400440532013000;;EUR",
            "entry;booked;2026-10-05;2026-10-05;-53.20;B1;REWE Markt;;Card payment",
            "entry;booked;2026-10-12;;-25.00;;Bakery;;",
            "entry;pending;2026-10-13;;-9.99;P1;Kiosk;;",
            "entry;booked;2026-10-14;;1500.00;B2;Example Employer;;Salary",
            "closing;2026-10-14;1421.80",
        ],
    );
    let register = || {
        session
            .ok(&["register", "Checking account"])
            .lines()
            .count()
    };
    let before = register();
    let import = |extra: &[&str]| {
        let mut line = vec!["import", file.as_str(), "--account", "Checking account"];
        line.extend(extra);
        session.ok(&line)
    };

    let preview = import(&["--dry-run"]);
    let lines: Vec<&str> = preview.lines().collect();
    assert_eq!(
        lines[0], "Would import 2 of 4 entries into Checking account.",
        "{preview}"
    );
    assert_eq!(
        lines[1].split_whitespace().collect::<Vec<_>>(),
        ["Entry", "Date", "Payee", "Amount"]
    );
    assert!(
        preview.contains("Left out 1 entry the bank hasn't booked yet: 1.3.\n"),
        "{preview}"
    );
    assert!(
        preview.contains("Left out 1 entry that may be in the book already."),
        "{preview}"
    );
    let similar = lines
        .iter()
        .find(|line| line.trim_start().starts_with("1.2 "))
        .expect("the bakery is listed");
    assert!(similar.contains("2026-10-09 Bakery ("), "{similar}");
    assert!(preview.contains("The Rules would change 1 of the new transactions:"));
    assert!(
        lines
            .iter()
            .any(|line| line.split_whitespace().collect::<Vec<_>>() == ["entry", "1.1", "Shops"]),
        "{preview}"
    );
    assert!(preview.contains("The balance on 2026-10-14 is 1421.80 EUR, as the bank says.\n"));
    assert!(preview.ends_with("Nothing was changed.\n"), "{preview}");
    assert_eq!(register(), before);

    let imported = import(&[]);
    assert!(
        imported.starts_with("Imported 2 of 4 entries into Checking account.\n"),
        "{imported}"
    );
    assert!(!imported.contains("Nothing was changed."), "{imported}");
    assert_eq!(register(), before + 2);
    let groceries = session.ok(&["register", "Food/Groceries"]);
    assert!(groceries.contains("REWE Markt"), "{groceries}");

    // The same file again: what has a bank ID is skipped, the rest waits.
    let again = import(&[]);
    assert!(
        again.starts_with("Imported 0 of 4 entries into Checking account.\n"),
        "{again}"
    );
    assert!(
        again.contains("Skipped 2 entries already in the book: 1.1, 1.4.\n"),
        "{again}"
    );
    // The bakery was already typed in; importing it anyway counts it twice,
    // which the balance check shows.
    let twice = import(&["--also", "1.2"]);
    assert!(twice.starts_with("Imported 1 of 4 entries"), "{twice}");
    assert!(
        twice.contains("The balance on 2026-10-14 is 1396.80 EUR, but the bank says 1421.80 EUR."),
        "{twice}"
    );
    // Importing nothing new changed nothing, so two undos take back both
    // imports, each as a whole.
    for _ in 0..2 {
        assert_eq!(
            session.ok(&["undo"]),
            "Undid the change: imported a statement.\n"
        );
    }
    assert_eq!(register(), before);
}

/// A CAMT.053 statement for the example IBAN, in this session's folder.
fn camt053_file(session: &Session, name: &str, entries: &str) -> String {
    let path = session.0.join(name);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.02\"><BkToCstmrStmt><Stmt>\
         <Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy></Acct>{entries}\
         </Stmt></BkToCstmrStmt></Document>"
    );
    std::fs::write(&path, xml).expect("file written");
    path.display().to_string()
}

#[test]
fn a_camt053_file_goes_through_the_whole_pipeline() {
    let session = household();
    let entry = |amount: &str, indicator: &str, status: &str, reference: &str, name: &str| {
        format!(
            "<Ntry><Amt Ccy=\"EUR\">{amount}</Amt><CdtDbtInd>{indicator}</CdtDbtInd>\
             <Sts>{status}</Sts><BookgDt><Dt>2026-10-05</Dt></BookgDt>\
             <AcctSvcrRef>{reference}</AcctSvcrRef><NtryDtls><TxDtls><RltdPties>\
             <Cdtr><Nm>{name}</Nm></Cdtr><Dbtr><Nm>{name}</Nm></Dbtr></RltdPties>\
             </TxDtls></NtryDtls></Ntry>"
        )
    };
    let entries = [
        entry("53.20", "DBIT", "BOOK", "C1", "REWE Markt"),
        entry("9.99", "DBIT", "PDNG", "C2", "Kiosk"),
        entry("1500.00", "CRDT", "BOOK", "C3", "Example Employer"),
    ]
    .concat();
    // The wrong closing balance shows that the check reads the file's.
    let closing = "<Bal><Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp><Amt Ccy=\"EUR\">1.00</Amt>\
                   <CdtDbtInd>CRDT</CdtDbtInd><Dt><Dt>2026-10-31</Dt></Dt></Bal>";
    let file = camt053_file(&session, "october.xml", &format!("{closing}{entries}"));
    let register = || {
        session
            .ok(&["register", "Checking account"])
            .lines()
            .count()
    };
    let before = register();
    // The format is recognized by the file's namespace.
    let imported = session.ok(&["import", &file, "--account", "Checking account"]);
    assert!(
        imported.starts_with("Imported 2 of 3 entries into Checking account.\n"),
        "{imported}"
    );
    assert!(
        imported.contains("Left out 1 entry the bank hasn't booked yet: 1.2.\n"),
        "{imported}"
    );
    assert!(
        imported.contains("The balance on 2026-10-31 is ")
            && imported.contains("but the bank says 1.00 EUR."),
        "{imported}"
    );
    assert_eq!(register(), before + 2);
    // The bank's references recognize the entries the second time.
    let again = session.ok(&["import", &file, "--account", "Checking account"]);
    assert!(
        again.contains("Skipped 2 entries already in the book: 1.1, 1.3.\n"),
        "{again}"
    );
    assert_eq!(register(), before + 2);
    // Importing nothing new changed nothing, so one undo takes it all back.
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: imported a statement.\n"
    );
    assert_eq!(register(), before);
    // A file that isn't CAMT.053 says so, and the format can be named.
    let csv = session.0.join("bank.csv");
    std::fs::write(&csv, "Date;Amount\n2026-10-01;1,00\n").expect("file written");
    let csv = csv.display().to_string();
    let refused = session.cli(&[
        "import",
        &csv,
        "--account",
        "Checking account",
        "--format",
        "camt053",
    ]);
    assert_eq!(refused.status, 1);
    assert!(
        refused.err.starts_with("error: import.malformed\n"),
        "{}",
        refused.err
    );
}

#[test]
fn an_mt940_file_goes_through_the_whole_pipeline() {
    let session = household();
    let path = session.0.join("october.sta");
    // The wrong closing balance shows that the check reads the file's.
    std::fs::write(
        &path,
        ":20:STARTUMS\n:25:DE89370400440532013000\n:60F:C261001EUR100,00\n\
         :61:2610051005D53,20NMSCNONREF//M1\n\
         :86:177?00Kartenzahlung?20SVWZ+Einkauf?32REWE Markt\n\
         :61:2610061006C1500,00NTRFNONREF//M2\n\
         :86:166?00Gutschrift?20SVWZ+Gehalt?32Example Employer\n\
         :62F:C261031EUR1,00\n-\n",
    )
    .expect("file written");
    let file = path.display().to_string();
    let register = || {
        session
            .ok(&["register", "Checking account"])
            .lines()
            .count()
    };
    let before = register();
    // The format is recognized by the file's first fields.
    let imported = session.ok(&["import", &file, "--account", "Checking account"]);
    assert!(
        imported.starts_with("Imported 2 of 2 entries into Checking account.\n"),
        "{imported}"
    );
    assert!(
        imported.contains("The balance on 2026-10-31 is ")
            && imported.contains("but the bank says 1.00 EUR."),
        "{imported}"
    );
    assert_eq!(register(), before + 2);
    // The bank's references recognize the entries the second time.
    let again = session.ok(&["import", &file, "--account", "Checking account"]);
    assert!(
        again.contains("Skipped 2 entries already in the book: 1.1, 1.2.\n"),
        "{again}"
    );
    assert_eq!(register(), before + 2);
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: imported a statement.\n"
    );
    assert_eq!(register(), before);
    // The format can be named, and a file of another format is refused.
    let named = session.ok(&[
        "import",
        &file,
        "--account",
        "Checking account",
        "--format",
        "mt940",
        "--dry-run",
    ]);
    assert!(named.contains("Would import 2 of 2 entries"), "{named}");
    let csv = session.0.join("bank.csv");
    std::fs::write(&csv, "Date;Amount\n2026-10-01;1,00\n").expect("file written");
    let csv = csv.display().to_string();
    let refused = session.cli(&[
        "import",
        &csv,
        "--account",
        "Checking account",
        "--format",
        "mt940",
    ]);
    assert_eq!(refused.status, 1);
    assert!(
        refused.err.starts_with("error: import.malformed\n"),
        "{}",
        refused.err
    );
}

#[test]
fn an_ofx_file_goes_through_the_whole_pipeline() {
    let session = household();
    let path = session.0.join("october.ofx");
    // The wrong closing balance shows that the check reads the file's.
    std::fs::write(
        &path,
        "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\nSECURITY:NONE\nENCODING:USASCII\n\
         CHARSET:1252\nCOMPRESSION:NONE\nOLDFILEUID:NONE\nNEWFILEUID:NONE\n\n\
         <OFX>\n<BANKMSGSRSV1>\n<STMTTRNRS>\n<TRNUID>1\n<STMTRS>\n<CURDEF>EUR\n\
         <BANKACCTFROM>\n<BANKID>COBADEFF\n<ACCTID>DE89370400440532013000\n\
         <ACCTTYPE>CHECKING\n</BANKACCTFROM>\n<BANKTRANLIST>\n\
         <STMTTRN>\n<TRNTYPE>DEBIT\n<DTPOSTED>20261005\n<TRNAMT>-53.20\n<FITID>M1\n\
         <NAME>REWE Markt\n<MEMO>Einkauf\n</STMTTRN>\n\
         <STMTTRN>\n<TRNTYPE>CREDIT\n<DTPOSTED>20261006\n<TRNAMT>1500.00\n<FITID>M2\n\
         <NAME>Example Employer\n<MEMO>Gehalt\n</STMTTRN>\n\
         </BANKTRANLIST>\n<LEDGERBAL><BALAMT>1.00<DTASOF>20261031</LEDGERBAL>\n\
         </STMTRS>\n</STMTTRNRS>\n</BANKMSGSRSV1>\n</OFX>\n",
    )
    .expect("file written");
    let file = path.display().to_string();
    let register = || {
        session
            .ok(&["register", "Checking account"])
            .lines()
            .count()
    };
    let before = register();
    // The format is recognized by the file's header.
    let imported = session.ok(&["import", &file, "--account", "Checking account"]);
    assert!(
        imported.starts_with("Imported 2 of 2 entries into Checking account.\n"),
        "{imported}"
    );
    assert!(
        imported.contains("The balance on 2026-10-31 is ")
            && imported.contains("but the bank says 1.00 EUR."),
        "{imported}"
    );
    assert_eq!(register(), before + 2);
    // The bank's transaction IDs recognize the entries the second time.
    let again = session.ok(&["import", &file, "--account", "Checking account"]);
    assert!(
        again.contains("Skipped 2 entries already in the book: 1.1, 1.2.\n"),
        "{again}"
    );
    assert_eq!(register(), before + 2);
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: imported a statement.\n"
    );
    assert_eq!(register(), before);
    // The format can be named, and a file of another format is refused.
    let named = session.ok(&[
        "import",
        &file,
        "--account",
        "Checking account",
        "--format",
        "ofx",
        "--dry-run",
    ]);
    assert!(named.contains("Would import 2 of 2 entries"), "{named}");
    let csv = session.0.join("bank.csv");
    std::fs::write(&csv, "Date;Amount\n2026-10-01;1,00\n").expect("file written");
    let csv = csv.display().to_string();
    let refused = session.cli(&[
        "import",
        &csv,
        "--account",
        "Checking account",
        "--format",
        "ofx",
    ]);
    assert_eq!(refused.status, 1);
    assert!(
        refused.err.starts_with("error: import.malformed\n"),
        "{}",
        refused.err
    );
}

#[test]
fn a_csv_file_is_read_with_its_settings() {
    let session = household();
    let write = |name: &str, content: &[u8]| {
        let path = session.0.join(name);
        std::fs::write(&path, content).expect("file written");
        path.display().to_string()
    };
    let settings = write(
        "bank.settings",
        b"# Example Bank\n\
          encoding = windows-1252\n\
          delimiter = \";\"\n\
          skip-lines = 1\n\
          decimal-mark = comma\n\
          date-order = day-month-year\n\
          booking-date = \"Buchungstag\"\n\
          amount = \"Betrag\"\n\
          other-party-name = \"Empf\xc3\xa4nger\"\n\
          status = \"Status\"\n\
          booked = \"Gebucht\"\n",
    );
    // Windows-1252, as many German banks write it: \xe4 is ä.
    let file = write(
        "umsaetze.csv",
        b"Konto DE89370400440532013000\r\n\
          Buchungstag;Empf\xe4nger;Betrag;Status\r\n\
          05.10.2026;REWE Markt;-53,20;Gebucht\r\n\
          06.10.2026;Kiosk;-9,99;Vorgemerkt\r\n\
          07.10.2026;B\xe4ckerei;-1.003,50;Gebucht\r\n",
    );
    let import = |extra: &[&str]| {
        let mut line = vec![
            "import",
            file.as_str(),
            "--account",
            "Checking account",
            "--csv-settings",
            settings.as_str(),
        ];
        line.extend(extra);
        session.cli(&line)
    };
    let imported = import(&[]);
    assert_eq!(imported.status, 0, "{}", imported.err);
    assert!(
        imported
            .out
            .starts_with("Imported 2 of 3 entries into Checking account.\n"),
        "{}",
        imported.out
    );
    assert!(imported.out.contains("B\u{e4}ckerei"), "{}", imported.out);
    assert!(imported.out.contains("-1003.50 EUR"), "{}", imported.out);
    assert!(
        imported
            .out
            .contains("Left out 1 entry the bank hasn't booked yet: 1.2.\n"),
        "{}",
        imported.out
    );
    // CSV files have no bank IDs: the second time, the check for similar
    // entries holds both back for the user to decide.
    let again = import(&[]);
    assert!(
        again
            .out
            .starts_with("Imported 0 of 3 entries into Checking account.\n"),
        "{}",
        again.out
    );
    assert!(
        again
            .out
            .contains("Left out 2 entries that may be in the book already."),
        "{}",
        again.out
    );
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: imported a statement.\n"
    );

    // --format csv needs the settings, which are for CSV files only.
    let without = session.cli(&[
        "import",
        &file,
        "--account",
        "Checking account",
        "--format",
        "csv",
    ]);
    assert_eq!(without.status, 2);
    assert!(without.err.contains("--csv-settings"), "{}", without.err);
    let other = import(&["--format", "camt053"]);
    assert_eq!(other.status, 2);
    // Settings that don't say where the amount is are refused with the line.
    let broken = write(
        "broken.settings",
        b"delimiter = \";\"\ndecimal-mark = comma\nbetrag = 3\n",
    );
    let refused = session.cli(&[
        "import",
        &file,
        "--account",
        "Checking account",
        "--csv-settings",
        &broken,
    ]);
    assert_eq!(refused.status, 1);
    assert_eq!(refused.err, "error: import.bad-settings\n  line: 3\n");
}

#[test]
fn a_statement_in_marks_comes_into_an_account_in_euros() {
    let session = household();
    let marks = statement_file(
        &session,
        "marks.kstest",
        &[
            "account;DE89370400440532013000;;DEM",
            "opening;2001-10-01;0.00",
            "entry;booked;2001-10-05;;100.00;M1;Employer;;",
            "entry;booked;2001-10-06;;-10.00;M2;Kiosk;;",
            "closing;2001-10-06;90.00",
        ],
    );
    let imported = session.ok(&["import", &marks, "--account", "Checking account"]);
    assert!(
        imported.contains(
            "Converted the entries in DEM into EUR, which replaced it at 1 EUR = 1.95583 DEM \
             on 1999-01-01. Each transaction keeps its amount in DEM.\n"
        ),
        "{imported}"
    );
    // 90.00 DEM are 46.02 EUR, as the account's balance converted once.
    assert!(
        imported
            .contains("The balance on 2001-10-06 is 46.02 EUR, as the bank says (90.00 DEM).\n"),
        "{imported}"
    );
    let register = session.ok(&["register", "Checking account"]);
    assert!(register.contains("51.13"), "{register}");
    assert!(register.contains("-5.11"), "{register}");
}

#[test]
fn statement_files_that_do_not_fit_are_refused() {
    let session = household();
    let dollars = statement_file(
        &session,
        "dollars.kstest",
        &[
            "account;;12345;USD",
            "entry;booked;2026-10-05;;-5.00;;Kiosk;;",
        ],
    );
    let refused = session.cli(&["import", &dollars, "--account", "Checking account"]);
    assert_eq!(refused.status, 1);
    assert!(
        refused
            .err
            .starts_with("error: import.account-currency-differs\n  account: Checking account\n"),
        "{}",
        refused.err
    );

    let two = statement_file(
        &session,
        "two.kstest",
        &[
            "account;DE89370400440532013000;;EUR",
            "entry;booked;2026-10-05;;-5.00;A1;Kiosk;;",
            "account;GB82WEST12345698765432;;EUR",
            "entry;booked;2026-10-05;;-7.00;A1;Kiosk;;",
        ],
    );
    let refused = session.cli(&["import", &two, "--account", "Checking account"]);
    assert_eq!(refused.status, 1);
    assert!(refused.err.contains("--statement N"), "{}", refused.err);
    let second = session.ok(&[
        "import",
        &two,
        "--account",
        "Checking account",
        "--statement",
        "2",
    ]);
    assert!(second.contains("  2.1 "), "{second}");
    assert!(second.contains("-7.00 EUR"), "{second}");

    let missing = session.cli(&["import", "no-such-file.kstest", "--account", "Savings"]);
    assert_eq!(missing.status, 1);
    assert!(
        missing.err.starts_with("error: import.io\n"),
        "{}",
        missing.err
    );
    let notes = session.0.join("notes.txt");
    std::fs::write(&notes, "Just some notes\n").expect("file written");
    let unknown = session.cli(&[
        "import",
        &notes.display().to_string(),
        "--account",
        "Savings",
    ]);
    assert_eq!(unknown.status, 1);
    assert!(
        unknown.err.contains("doesn't recognize the file's format"),
        "{}",
        unknown.err
    );
    for wrong in [
        vec!["import", &two, "--account", "Savings", "--format", "csv"],
        vec!["import", &two, "--account", "Savings", "--statement", "3"],
        vec![
            "import",
            &two,
            "--account",
            "Savings",
            "--statement",
            "1",
            "--also",
            "1",
        ],
        vec![
            "import",
            &two,
            "--account",
            "Savings",
            "--statement",
            "1",
            "--also",
            "2.1",
        ],
    ] {
        assert_eq!(session.cli(&wrong).status, 2, "{wrong:?}");
    }
}

#[test]
fn mistakes_in_a_rule_are_refused() {
    let session = household();
    session.ok(&["rule", "add", "Shop", "--if", "payee is Shop"]);
    for (line, code) in [
        (
            &["rule", "add", "Shop", "--if", "payee is Other shop"][..],
            "store.duplicate-rule-name",
        ),
        (
            &["rule", "add", "Broken", "--if", "purpose matches (unclosed"],
            "rule.bad-pattern",
        ),
        (
            &["rule", "add", "Empty", "--if", "payee contains "],
            "rule.empty-text",
        ),
        (
            &["rule", "add", "Range", "--if", "amount between 10 5 EUR"],
            "rule.amount-range-reversed",
        ),
        (
            &["rule", "move", "Shop", "--inside", "Shop"],
            "store.rule-inside-itself",
        ),
    ] {
        let outcome = session.cli(line);
        assert_eq!(outcome.status, 1, "{line:?}: {}", outcome.err);
        assert!(
            outcome.err.starts_with(&format!("error: {code}\n")),
            "{line:?}: {}",
            outcome.err
        );
    }
    let outcome = session.cli(&["rule", "add", "Range", "--if", "amount between 10 5 EUR"]);
    assert!(outcome.err.contains("  rule: "), "{}", outcome.err);
    assert!(outcome.err.contains("  condition: 1\n"), "{}", outcome.err);
    let unknown = session.cli(&["rule", "remove", "Nothing"]);
    assert_eq!(unknown.status, 1);
    assert!(
        unknown.err.contains("no Rule called `Nothing`"),
        "{}",
        unknown.err
    );
    assert_eq!(session.ok(&["rule", "list"]).lines().count(), 3);
}

#[test]
fn wrong_command_lines_end_with_status_2() {
    let session = household();
    for line in [
        &["bogus"][..],
        &["account"],
        &["account", "add", "Cash"],
        &["account", "frobnicate"],
        &["register"],
        &["balances", "--nope"],
        &["transaction", "add", "--date", "2026-10-02"],
        &["undo", "twice"],
        &["redo", "--all"],
        &["history"],
        &["history", "forget"],
        &["history", "keep"],
        &["history", "list", "--all"],
        &["transfer"],
        &["transfer", "match"],
        &["transfer", "suggest", "--all"],
        &["transfer", "join", "123456"],
        &["rule"],
        &["rule", "sort"],
        &["rule", "add", "Shop"],
        &["rule", "add", "Shop", "--if", "payee"],
        &["rule", "add", "Shop", "--if", "payee sounds-like Shop"],
        &["rule", "add", "Shop", "--if", "amount is 5"],
        &[
            "rule",
            "add",
            "Shop",
            "--if",
            "x",
            "--category",
            "Food",
            "--to",
            "Savings",
        ],
        &["rule", "list", "--all"],
        &["rule", "run", "now"],
    ] {
        let outcome = session.cli(line);
        assert_eq!(outcome.status, 2, "{line:?}: {}", outcome.err);
        assert!(outcome.err.starts_with("error: "), "{line:?}");
        assert!(outcome.err.contains("kassensturz-cli help"), "{line:?}");
        assert_eq!(outcome.out, "");
    }
    // Wrong values are failures of the command, not of the command line.
    let wallet = session.cli(&[
        "account",
        "add",
        "Cash",
        "--type",
        "wallet",
        "--currency",
        "EUR",
    ]);
    assert_eq!(wallet.status, 1);
    // No book to use.
    let outcome = run_line(&["balances".to_owned()], &Environment::default());
    assert_eq!(outcome.status, 2);
    assert!(outcome.err.contains("no book"));
}

#[test]
fn changes_can_be_undone_and_redone_by_later_commands() {
    let session = household();
    session.ok(&["account", "rename", "Savings", "Holiday fund"]);
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: renamed an account.\n"
    );
    assert!(session.ok(&["account", "list"]).contains("Savings"));
    assert_eq!(
        session.ok(&["redo"]),
        "Redid the change: renamed an account.\n"
    );
    assert!(session.ok(&["account", "list"]).contains("Holiday fund"));
    assert_eq!(session.ok(&["redo"]), "There is nothing to redo.\n");

    // A transaction removed by mistake comes back with its parts.
    let added = session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-05",
        "--account",
        "Checking account",
        "--amount",
        "-53.20",
        "--category",
        "Food/Groceries",
        "--payee",
        "Corner Grocery",
    ]);
    let shopping = last_added(&added);
    session.ok(&["transaction", "remove", &shopping]);
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: removed a transaction.\n"
    );
    assert!(
        session
            .ok(&["register", "Food/Groceries"])
            .contains("Corner Grocery")
    );
    assert!(session.ok(&["balances"]).contains("-53.20 EUR"));

    // An account in a currency the book doesn't use yet is one change, and
    // so is a rate between two new currencies.
    session.ok(&[
        "account",
        "add",
        "Dollar account",
        "--type",
        "checking",
        "--currency",
        "USD",
    ]);
    session.ok(&[
        "rate",
        "add",
        "--date",
        "2026-10-01",
        "--from",
        "CHF",
        "--to",
        "JPY",
        "--rate",
        "170",
    ]);
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: added an exchange rate.\n"
    );
    assert_eq!(
        session.ok(&["undo"]),
        "Undid the change: added an account.\n"
    );
    assert!(!session.ok(&["account", "list"]).contains("Dollar account"));
    assert!(
        session
            .ok(&["history", "list"])
            .lines()
            .take(3)
            .skip(1)
            .all(|line| line.ends_with("undone"))
    );
}

#[test]
fn the_history_lists_every_change_and_is_kept_as_long_as_the_user_says() {
    let session = Session::new();
    session.ok(&["create"]);
    session.ok(&[
        "account",
        "add",
        "Cash",
        "--type",
        "cash",
        "--currency",
        "EUR",
    ]);
    session.ok(&["account", "rename", "Cash", "Wallet"]);
    session.ok(&["undo"]);
    let list = session.ok(&["history", "list"]);
    let lines: Vec<&str> = list.lines().collect();
    assert_eq!(lines.len(), 4, "{list}");
    assert_eq!(lines[0], "Made at (UTC)         Change              State");
    for (line, rest) in [
        (lines[1], "renamed an account  undone"),
        (lines[2], "added an account    done"),
    ] {
        let (made_at, change) = line.split_at(20);
        assert!(made_at.parse::<Timestamp>().is_ok(), "{line}");
        assert_eq!(change, format!("  {rest}"));
    }
    assert_eq!(lines[3], "The book keeps its history for 30 days.");

    assert_eq!(
        session.ok(&["history", "keep", "forever"]),
        "The book keeps its history forever.\n"
    );
    assert!(
        session
            .ok(&["history", "list"])
            .ends_with("The book keeps its history forever.\n")
    );
    let wrong = session.cli(&["history", "keep", "-1"]);
    assert_eq!(wrong.status, 1);
    assert_eq!(
        wrong.err,
        "error: `-1` isn't a number of days from 0 to 4294967295, or forever\n"
    );

    // With 0 days, every command starts without a history.
    session.ok(&["history", "keep", "0"]);
    session.ok(&["account", "rename", "Cash", "Wallet"]);
    assert_eq!(session.ok(&["undo"]), "There is nothing to undo.\n");
    session.ok(&["history", "keep", "7"]);
    session.ok(&["account", "rename", "Wallet", "Cash"]);
    assert_eq!(
        session.ok(&["history", "clean-up"]),
        "Deleted the whole history: nothing done so far can be undone.\n"
    );
    assert_eq!(session.ok(&["undo"]), "There is nothing to undo.\n");
    assert_eq!(
        session.ok(&["history", "list"]),
        "Made at (UTC)  Change  State\nThe book keeps its history for 7 days.\n"
    );
}

#[test]
fn help_is_shown_without_a_book() {
    for line in [&[][..], &["help"], &["--help"], &["-h"]] {
        let line: Vec<String> = line.iter().map(|arg| (*arg).to_owned()).collect();
        let outcome = run_line(&line, &Environment::default());
        assert_eq!(outcome.status, 0);
        assert!(
            outcome
                .out
                .starts_with("kassensturz-cli: try out KassenSturz's core")
        );
        assert_eq!(outcome.err, "");
    }
}

#[test]
fn text_from_the_book_cannot_move_the_terminal() {
    let session = household();
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-02",
        "--account",
        "Savings",
        "--amount",
        "1",
        "--payee",
        "Sneaky \u{1b}[2J shop",
    ]);
    let register = session.ok(&["register", "Savings"]);
    assert!(register.contains("Sneaky ?[2J shop"));
    assert!(!register.contains('\u{1b}'));
}

#[test]
fn a_book_open_elsewhere_is_refused_unless_opened_anyway() {
    let session = household();
    let elsewhere = Computer {
        name: "Desktop".into(),
        backup_folder: session.backups(),
    };
    let held = Book::open(&session.book(), &elsewhere).expect("opened elsewhere");

    let refused = session.cli(&["balances"]);
    assert_eq!(refused.status, 1);
    assert!(
        refused
            .err
            .starts_with("error: store.locked\n  computer: \"Desktop\"\n  since: "),
        "{}",
        refused.err
    );

    let anyway = session.cli(&["--open-anyway", "balances"]);
    assert_eq!(anyway.status, 0, "{}", anyway.err);
    assert!(anyway.err.starts_with(
        "warning: opening the book anyway, although it says it is open on Desktop since "
    ));
    assert!(anyway.out.contains("Checking account"));
    drop(held);
}

#[test]
fn a_mistake_in_the_command_is_found_before_the_book_is_touched() {
    let session = Session::new();
    assert_eq!(session.cli(&["create", "extra"]).status, 2);
    assert_eq!(session.cli(&["bogus"]).status, 2);
    // Neither created the book, and neither looked for one.
    assert!(!session.book().exists());
}

#[test]
fn creating_a_book_never_replaces_an_existing_file() {
    let session = Session::new();
    session.ok(&["create"]);
    let again = session.cli(&["create"]);
    assert_eq!(again.status, 1);
    assert_eq!(
        again.err,
        format!(
            "error: store.already-exists\n  file: \"{}\"\n",
            session.book().display()
        )
    );
    let missing = run_line(
        &[
            "--book".into(),
            session.0.join("Nothing.ksz").display().to_string(),
            "--backup-folder".into(),
            session.backups().display().to_string(),
            "balances".into(),
        ],
        &Environment::default(),
    );
    assert_eq!(missing.status, 1);
    assert_eq!(missing.err, "error: store.not-found\n");
}

#[test]
fn the_environment_supplies_the_book_and_the_backup_folder() {
    let session = Session::new();
    let environment = Environment {
        book: Some(session.book()),
        computer_name: "Laptop".into(),
        backup_folder: Some(session.backups()),
    };
    let create = run_line(&["create".to_owned()], &environment);
    assert_eq!(create.status, 0, "{}", create.err);
    let list = run_line(&["account".to_owned(), "list".to_owned()], &environment);
    assert_eq!(
        (list.status, list.out.as_str()),
        (0, "Name  Type  Currency  Status\n")
    );
}

/// The real program, started the way a user starts it.
#[test]
fn the_program_reads_the_book_from_the_environment() {
    let session = Session::new();
    let program = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_kassensturz-cli"))
            .args(args)
            .env("KASSENSTURZ_BOOK", session.book())
            .env("XDG_DATA_HOME", &session.0)
            .env("HOME", &session.0)
            .output()
            .expect("the program starts")
    };
    let created = program(&["create"]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let failed = program(&["transaction", "show", "123456"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).starts_with("error: no transaction"));
    assert!(Path::new(&session.book()).exists());
}

#[test]
fn transactions_are_exported_as_csv_into_a_new_file() {
    let session = household();
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-01",
        "--account",
        "Checking account",
        "--amount",
        "2500.00",
        "--payee",
        "Employer",
        "--category",
        "Salary",
    ]);
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-05",
        "--account",
        "Checking account",
        "--amount",
        "-12.50",
        "--payee",
        "=HYPERLINK(\"x\")",
        "--category",
        "Food/Groceries",
    ]);
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-06",
        "--account",
        "Checking account",
        "--amount",
        "-300.00",
        "--to",
        "Savings",
    ]);
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-07",
        "--account",
        "Checking account",
        "--amount",
        "-40.00",
        "--payee",
        "Market",
        "--part",
        "Food/Groceries=-25.00",
        "--part",
        "Home/Other=",
    ]);
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-08",
        "--account",
        "Checking account",
        "--amount",
        "-1.00",
        "--payee",
        "Kiosk",
    ]);

    // All accounts, closed ones too, in the order of the list.
    let file = session.0.join("all.csv");
    let file_text = file.display().to_string();
    let said = session.ok(&["export", &file_text]);
    assert_eq!(
        said,
        format!("Exported 6 transactions of 2 accounts to {file_text} as csv.\n")
    );
    let csv = std::fs::read_to_string(&file).expect("file written");
    assert_eq!(
        csv,
        "date,account,payee,category,transfer,memo,amount,currency,parts,original_amount,original_currency\r\n\
         2026-10-01,Checking account,Employer,Salary,,,2500.00,EUR,,,\r\n\
         2026-10-05,Checking account,\"'=HYPERLINK(\"\"x\"\")\",Food/Groceries,,,-12.50,EUR,,,\r\n\
         2026-10-06,Checking account,,,Savings,,-300.00,EUR,,,\r\n\
         2026-10-07,Checking account,Market,,,,-40.00,EUR,2,,\r\n\
         2026-10-08,Checking account,Kiosk,,,,-1.00,EUR,,,\r\n\
         2026-10-06,Savings,,,Checking account,,300.00,EUR,,,\r\n"
    );

    // One account, with Excel-friendly settings.
    let one = session.0.join("savings.csv");
    let one_text = one.display().to_string();
    session.ok(&[
        "export",
        &one_text,
        "--account",
        "savings",
        "--separator",
        "semicolon",
        "--decimal-mark",
        "comma",
        "--byte-order-mark",
        "--no-header",
    ]);
    assert_eq!(
        std::fs::read(&one).expect("file written"),
        "\u{feff}2026-10-06;Savings;;;Checking account;;300,00;EUR;;;\r\n".as_bytes()
    );

    // A file is never overwritten.
    let again = session.cli(&["export", &file_text]);
    assert_eq!(again.status, 1);
    assert_eq!(
        again.err,
        format!("error: export.already-exists\n  file: \"{file_text}\"\n")
    );
    assert_eq!(std::fs::read_to_string(&file).expect("kept"), csv);

    // Mistakes in the command, and a comma that is both separator and mark.
    for (args, message) in [
        (vec!["--format", "xml"], "`xml` isn't a format to export"),
        (vec!["--separator", "pipe"], "`pipe` isn't a separator"),
        (vec!["--decimal-mark", "dot"], "`dot` isn't a decimal mark"),
    ] {
        let target = session.0.join("never.csv").display().to_string();
        let mut line = vec!["export", target.as_str()];
        line.extend(args);
        let refused = session.cli(&line);
        assert_eq!(refused.status, 2, "{line:?}");
        assert!(refused.err.contains(message), "{}", refused.err);
        assert!(!session.0.join("never.csv").exists());
    }
    let target = session.0.join("never.csv").display().to_string();
    let clash = session.cli(&["export", &target, "--decimal-mark", "comma"]);
    assert_eq!(clash.status, 1);
    assert_eq!(clash.err, "error: export.bad-settings\n");
    let unknown = session.cli(&["export", &target, "--account", "Nowhere"]);
    assert_eq!(unknown.status, 1);
    assert!(unknown.err.contains("there is no account called `Nowhere`"));
    assert!(!session.0.join("never.csv").exists());
}

#[test]
fn the_whole_book_is_exported_and_read_back_into_a_new_book() {
    let session = household();
    session.ok(&[
        "rule",
        "add",
        "Market",
        "--if",
        "payee is Market",
        "--category",
        "Food/Groceries",
    ]);
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-01",
        "--account",
        "Checking account",
        "--amount",
        "2500.00",
        "--payee",
        "Employer",
        "--category",
        "Salary",
    ]);
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-06",
        "--account",
        "Checking account",
        "--amount",
        "-300.00",
        "--to",
        "Savings",
    ]);
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-07",
        "--account",
        "Checking account",
        "--amount",
        "-40.00",
        "--payee",
        "Market",
    ]);
    session.ok(&["rule", "run"]);
    session.ok(&[
        "account",
        "add",
        "Old account",
        "--type",
        "cash",
        "--currency",
        "EUR",
    ]);
    session.ok(&["account", "close", "Old account"]);
    session.ok(&["history", "keep", "forever"]);

    let file = session.0.join("household.json");
    let file_text = file.display().to_string();
    assert_eq!(
        session.ok(&["export", &file_text, "--format", "book"]),
        format!(
            "Exported the book to {file_text} as book: 9 accounts and categories, 3 transactions.\n"
        )
    );

    // A new book from the file holds the same, and writes the same file.
    let copy = Session::new();
    assert_eq!(
        copy.ok(&["create", "--from", &file_text]),
        format!("Created the book from {file_text}: 9 accounts and categories, 3 transactions.\n")
    );
    for command in [
        &["balances", "--all"][..],
        &["register", "Checking account"],
        &["register", "Food/Groceries"],
        &["category", "list"],
        &["rule", "list"],
    ] {
        assert_eq!(copy.ok(command), session.ok(command), "{command:?}");
    }
    // Reading the file isn't a change that could be undone.
    assert_eq!(
        copy.ok(&["history", "list"]),
        "Made at (UTC)  Change  State\nThe book keeps its history forever.\n"
    );
    let again = copy.0.join("again.json");
    copy.ok(&["export", &again.display().to_string(), "--format", "book"]);
    assert_eq!(
        std::fs::read(&again).expect("written"),
        std::fs::read(&file).expect("written")
    );

    // The complete export takes none of the CSV options.
    let target = session.0.join("never.json").display().to_string();
    for option in [
        &["--account", "Savings"][..],
        &["--separator", "tab"],
        &["--no-header"],
    ] {
        let mut line = vec!["export", target.as_str(), "--format", "book"];
        line.extend(option);
        let refused = session.cli(&line);
        assert_eq!(refused.status, 2, "{line:?}");
        assert!(
            refused.err.contains("is only for the csv format"),
            "{}",
            refused.err
        );
    }
    assert!(!session.0.join("never.json").exists());
}

#[test]
fn a_file_that_does_not_make_a_book_leaves_nothing_behind() {
    let session = household();
    session.ok(&[
        "transaction",
        "add",
        "--date",
        "2026-10-01",
        "--account",
        "Checking account",
        "--amount",
        "10.00",
    ]);
    let file = session.0.join("household.json");
    let file_text = file.display().to_string();
    session.ok(&["export", &file_text, "--format", "book"]);
    let written = std::fs::read_to_string(&file).expect("written");

    let new = Session::new();
    let not_json = new.0.join("notes.txt");
    std::fs::write(&not_json, "Not a book\n").expect("written");
    // A balance that isn't zero in an account the file says is closed: the
    // file reads, but the book refuses it.
    let closed = new.0.join("closed.json");
    std::fs::write(
        &closed,
        written.replacen("\"closed\": false", "\"closed\": true", 1),
    )
    .expect("written");
    std::fs::write(
        new.0.join("busy.json"),
        written.replace("\"version\": 2", "\"version\": 3"),
    )
    .expect("written");
    for (from, code) in [
        (new.0.join("missing.json"), "export.io"),
        (not_json, "export.not-json"),
        (new.0.join("busy.json"), "export.newer-version"),
    ] {
        let refused = new.cli(&["create", "--from", &from.display().to_string()]);
        assert_eq!(refused.status, 1, "{from:?}");
        assert!(
            refused.err.starts_with(&format!("error: {code}\n")),
            "{from:?}: {}",
            refused.err
        );
        assert!(!new.book().exists(), "{from:?}");
    }
    let refused = new.cli(&["create", "--from", &closed.display().to_string()]);
    assert_eq!(refused.status, 1);
    assert!(
        refused.err.starts_with("error: store.account-not-empty\n"),
        "{}",
        refused.err
    );
    assert!(!new.book().exists());
    assert!(!new.0.join("Household.ksz.lock").exists());

    // An existing book is never replaced by one from a file.
    let kept = session.cli(&["create", "--from", &file_text]);
    assert_eq!(kept.status, 1);
    assert!(session.ok(&["category", "list"]).contains("Groceries"));
    // And `create` takes nothing else.
    assert_eq!(
        new.cli(&["create", "--from", &file_text, "extra"]).status,
        2
    );
    assert!(!new.book().exists());
}

/// The project's KMyMoney sample (D-102), the golden file of the importer.
fn kmymoney_sample() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden/kmy/kassensturz-sample.kmy")
        .display()
        .to_string()
}

#[test]
fn a_kmymoney_file_makes_a_new_book() {
    let session = Session::new();
    let sample = kmymoney_sample();
    assert_eq!(
        session.ok(&["create", "--from", &sample]),
        format!(
            "Created the book from {sample}: 5 accounts, 51 categories, 538 transactions, 0 rates.\n\
             536 transactions didn't add up and got a part without a category for the rest.\n\
             Not imported, because KassenSturz can't hold it yet:\n  \
             institutions: 1\n  \
             account-numbers: 2\n  \
             account-groups: 3\n  \
             reconciliation: 73\n  \
             rates: 20\n  \
             online-banking: 2\n\
             8 pairs of transactions look like Transfers between your accounts; \
             see `transfer suggest`.\n"
        )
    );
    // The balances KMyMoney itself keeps for the sample's accounts.
    assert_eq!(
        session.ok(&["balances"]),
        "Account           Balance\n\
         Besitz Wofenn    0.00 EUR\n\
         Kelmarl          0.00 EUR\n\
         Konto Wodübex   83.70 EUR\n\
         Loturdros        8.05 EUR\n\
         Schulden Never   0.00 EUR\n\
         Total           91.75 EUR\n"
    );
    // The eight transfers between the two bank accounts, each kept by
    // KMyMoney as one entry in each account.
    let suggested = session.ok(&["transfer", "suggest"]);
    assert_eq!(
        suggested
            .lines()
            .filter(|line| line.starts_with("20"))
            .count(),
        8,
        "{suggested}"
    );
    // Nothing to undo: the book was made this way, with the settings of a
    // new book.
    assert_eq!(
        session.ok(&["history", "list"]),
        "Made at (UTC)  Change  State\nThe book keeps its history for 30 days.\n"
    );
}

/// The same book saved by KMyMoney as a SQLite file (D-102, D-103).
fn kmymoney_sqlite_sample() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden/kmy-sqlite/kassensturz-sample.sqlite")
        .display()
        .to_string()
}

#[test]
fn a_kmymoney_sqlite_file_makes_the_same_book_as_a_kmy_file() {
    let from_kmy = Session::new();
    let said_kmy = from_kmy.ok(&["create", "--from", &kmymoney_sample()]);
    let sqlite = kmymoney_sqlite_sample();
    // Recognized, and the same when asked for.
    for format in [&[][..], &["--format", "kmy-sqlite"][..]] {
        let session = Session::new();
        let mut args = vec!["create", "--from", &sqlite];
        args.extend_from_slice(format);
        let said = session.ok(&args);
        assert_eq!(
            said.replace(&sqlite, "FILE"),
            said_kmy.replace(&kmymoney_sample(), "FILE")
        );
        assert_eq!(session.ok(&["balances"]), from_kmy.ok(&["balances"]));
    }
    // Asked for as the other format, it is no such file.
    let session = Session::new();
    let refused = session.cli(&["create", "--from", &sqlite, "--format", "kmy"]);
    assert_eq!(refused.status, 1);
    assert!(
        refused.err.starts_with("error: import.malformed\n"),
        "{}",
        refused.err
    );
    assert!(!session.book().exists());
    // A SQLite file that isn't KMyMoney's leaves nothing behind either.
    let other = session.0.join("other.sqlite");
    std::fs::write(&other, b"SQLite format 3\0 and then nothing").expect("written");
    let refused = session.cli(&["create", "--from", &other.display().to_string()]);
    assert_eq!(refused.status, 1);
    assert!(
        refused.err.starts_with("error: import.malformed\n"),
        "{}",
        refused.err
    );
    assert!(!session.book().exists());
}

#[test]
fn a_kmymoney_file_in_marks_comes_in_in_euros() {
    let session = Session::new();
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden/kmy/withdrawn-currency-transaction.kmy")
        .display()
        .to_string();
    assert_eq!(
        session.ok(&["create", "--from", &file]),
        format!(
            "Created the book from {file}: 2 accounts, 1 category, 2 transactions, 0 rates.\n\
             Converted DEM into EUR, which replaced it at 1 EUR = 1.95583 DEM on 1999-01-01: \
             1 accounts and categories, 4 parts. Each part keeps its amount in DEM.\n"
        )
    );
    // The old checking account's 10.00 DEM are 5.11 EUR at the fixed rate.
    assert_eq!(
        session.ok(&["balances"]),
        "Account          Balance\n\
         Checking      -24.75 EUR\n\
         Old checking   -5.11 EUR\n\
         Total         -29.86 EUR\n"
    );
    let register = session.ok(&["register", "Old checking"]);
    let (_, id) = register
        .lines()
        .last()
        .and_then(|line| line.rsplit_once("  "))
        .expect("a transaction");
    // Each part shows the amount it had in marks.
    assert_eq!(
        session
            .ok(&["transaction", "show", id])
            .lines()
            .skip(4)
            .collect::<Vec<_>>(),
        [
            "  Old checking  -5.11 EUR  was -10.00 DEM at 1 EUR = 1.95583 DEM \
             (1999-01-01, fixed when the currency was replaced)",
            "  Groceries      5.11 EUR  was 10.00 DEM at 1 EUR = 1.95583 DEM \
             (1999-01-01, fixed when the currency was replaced)",
        ]
    );
}

/// A holiday in Romania in 2003, kept in the old leu, which neither of
/// KassenSturz's tables lists, with KMyMoney's rate into the new one.
const IN_LEI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE KMYMONEY-FILE>
<KMYMONEY-FILE>
  <FILEINFO>
    <VERSION id="1"/>
  </FILEINFO>
  <ACCOUNTS>
    <ACCOUNT id="AStd::Asset" parentaccount="" type="9" name="Anlagen" currency="EUR"/>
    <ACCOUNT id="AStd::Expense" parentaccount="" type="13" name="Ausgabe" currency="EUR"/>
    <ACCOUNT id="A000001" parentaccount="AStd::Asset" type="1" name="Checking" currency="EUR"/>
    <ACCOUNT id="A000002" parentaccount="AStd::Asset" type="3" name="Lei in cash" currency="ROL"/>
    <ACCOUNT id="A000003" parentaccount="AStd::Expense" type="13" name="Holiday" currency="ROL"/>
  </ACCOUNTS>
  <TRANSACTIONS>
    <TRANSACTION id="T000000000000000001" postdate="2003-08-04" memo="" commodity="ROL">
      <SPLITS>
        <SPLIT id="S0001" payee="" reconcileflag="0" value="-150000000/100" shares="-150000000/100" memo="" account="A000002" bankid=""/>
        <SPLIT id="S0002" payee="" reconcileflag="0" value="150000000/100" shares="150000000/100" memo="" account="A000003" bankid=""/>
      </SPLITS>
    </TRANSACTION>
  </TRANSACTIONS>
  <CURRENCIES>
    <CURRENCY id="ROL" name="Old leu" symbol="ROL" type="3" saf="100" pp="4" scf="100"/>
  </CURRENCIES>
  <PRICES>
    <PRICEPAIR from="ROL" to="RON">
      <PRICE date="2005-06-30" price="1/10000" source="KMyMoney"/>
    </PRICEPAIR>
  </PRICES>
</KMYMONEY-FILE>
"#;

#[test]
fn a_kmymoney_file_in_a_currency_no_table_lists_needs_its_successor() {
    let session = Session::new();
    let file = session.0.join("holiday.kmy");
    std::fs::write(&file, IN_LEI).expect("written");
    let file = file.display().to_string();
    // The file's rate is a suggestion; nothing is made without the user's
    // word.
    let refused = session.cli(&["create", "--from", &file]);
    assert_eq!(refused.status, 1);
    assert!(
        refused.err.contains(
            "Say which with --replaced ROL=NEW:RATE:DAY, where one NEW was worth RATE ROL \
             from DAY on. The file's own rates suggest --replaced ROL=RON:10000:2005-06-30;"
        ),
        "{}",
        refused.err
    );
    assert!(!session.book().exists());
    for wrong in [
        &["create", "--replaced", "ROL=RON:10000:2005-07-01"][..],
        &["create", "--from", &file, "--replaced", "ROL=RON:10000"],
        &[
            "create",
            "--from",
            &file,
            "--replaced",
            "TRL=TRY:1000000:2005-01-01",
        ],
        &[
            "create",
            "--from",
            &file,
            "--replaced",
            "ROL=RON:10000:2005-07-01",
            "--replaced",
            "rol=RON:10000:2005-07-01",
        ],
    ] {
        assert_eq!(session.cli(wrong).status, 2, "{wrong:?}");
    }
    let refused = session.cli(&[
        "create",
        "--from",
        &file,
        "--replaced",
        "ROL=DEM:1:2005-07-01",
    ]);
    assert!(
        refused
            .err
            .starts_with("error: import.successor-not-current\n"),
        "{}",
        refused.err
    );
    assert!(!session.book().exists());

    assert_eq!(
        session.ok(&[
            "create",
            "--from",
            &file,
            "--replaced",
            "ROL=RON:10000:2005-07-01"
        ]),
        format!(
            "Created the book from {file}: 2 accounts, 1 category, 1 transaction, 0 rates.\n\
             Converted ROL into RON, which replaced it at 1 RON = 10000 ROL on 2005-07-01: \
             2 accounts and categories, 2 parts. Each part keeps its amount in ROL.\n\
             Not imported, because KassenSturz can't hold it yet:\n  \
             rates: 1\n"
        )
    );
    // 1,500,000.00 ROL are 150.00 RON.
    assert!(
        session
            .ok(&["balances"])
            .contains("Lei in cash  -150.00 RON"),
    );
}

#[test]
fn a_kmymoney_file_is_read_as_one_when_asked() {
    let session = Session::new();
    let sample = kmymoney_sample();
    // Named, it is read even when it doesn't start like one.
    let notes = session.0.join("notes.kmy");
    std::fs::write(&notes, "Not a book\n").expect("written");
    let refused = session.cli(&[
        "create",
        "--from",
        &notes.display().to_string(),
        "--format",
        "kmy",
    ]);
    assert_eq!(refused.status, 1);
    assert!(
        refused.err.starts_with("error: import.malformed\n"),
        "{}",
        refused.err
    );
    assert!(!session.book().exists());
    // A KMyMoney file isn't a complete export.
    let refused = session.cli(&["create", "--from", &sample, "--format", "book"]);
    assert_eq!(refused.status, 1);
    assert!(!session.book().exists());
    // Only these formats make a book, and only from a file.
    assert_eq!(
        session
            .cli(&["create", "--from", &sample, "--format", "csv"])
            .status,
        2
    );
    assert_eq!(session.cli(&["create", "--format", "kmy"]).status, 2);
    assert!(!session.book().exists());
    assert!(
        session
            .ok(&["create", "--from", &sample, "--format", "kmy"])
            .starts_with("Created the book from ")
    );
}
