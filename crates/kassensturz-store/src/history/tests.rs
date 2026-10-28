// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the change history: undo and redo, also after a restart, how
//! long it is kept, and cleaning it up. All data is made up (AGENTS.md §3.1
//! rule 2).

use std::collections::HashSet;
use std::path::Path;

use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, BookTo, Condition, CurrencyCode, DatedRate,
    DatedRateId, ForeignAmount, ImportedDetails, MatchMode, Money, RateSource, RoundingMode, Rule,
    RuleId, Split, TextField, TextOperator, Transaction, TransactionId,
};

use super::*;
use crate::Computer;
use crate::lock::lock_path;
use crate::test_folder::TempFolder;
use crate::tests::{EUR, Sample, USD, account, date, eur, sample, transaction};

/// Everything the book holds: every row of every table the history records,
/// each value as SQL text, so two moments of a book can be compared exactly.
fn contents(book: &Book) -> Vec<String> {
    let mut rows = Vec::new();
    for table in &RECORDED {
        let values = table
            .columns
            .iter()
            .map(|(name, _)| format!("quote({name})"))
            .collect::<Vec<_>>()
            .join(" || ', ' || ");
        let mut statement = book
            .conn
            .prepare(&format!(
                "SELECT '{name}: ' || {values} FROM main.{name} ORDER BY 1",
                name = table.name
            ))
            .expect("rows read");
        let table_rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("rows read")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows read");
        rows.extend(table_rows);
    }
    rows
}

/// How many changes the history keeps in the book itself, not in memory.
fn changes_in_the_file(book: &Book) -> i64 {
    book.conn
        .query_row("SELECT count(*) FROM main.changes", [], |row| row.get(0))
        .expect("changes counted")
}

fn actions(book: &Book) -> Vec<(Action, bool)> {
    book.history()
        .expect("history read")
        .into_iter()
        .map(|change| (change.action, change.undone))
        .collect()
}

fn computer(folder: &TempFolder) -> Computer {
    Computer {
        name: "Laptop".into(),
        backup_folder: folder.join("Backups"),
    }
}

fn category(book: &mut Book, name: &str, parent: Option<AccountId>) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind: AccountKind::Expense,
        currency: EUR,
        account_type: None,
        closed: false,
        parent,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("category added");
    id
}

fn euros_in_dollars(rate: &str) -> DatedRate {
    DatedRate {
        id: DatedRateId::generate(),
        date: date(2026, 10, 1),
        from: EUR,
        to: USD,
        rate: rate.parse().expect("valid rate"),
        source: RateSource::Manual,
    }
}

/// What a bank might deliver for a card payment. The IBAN is the example
/// for Germany in SWIFT's IBAN Registry (D-055).
fn delivered() -> ImportedDetails {
    ImportedDetails {
        bank_id: Some("2026100500017".into()),
        booking_date: Some(date(2026, 10, 5)),
        value_date: Some(date(2026, 10, 6)),
        other_party_name: Some("CORNER GROCERY 0815".into()),
        other_party_iban: Some("DE89370400440532013000".into()),
        remittance_information: Some("Card payment 2026-10-04".into()),
    }
}

/// A card payment as an import would add it: the checking account's part
/// with what the bank delivered, and a part without a category yet.
fn imported_purchase(checking: AccountId) -> Transaction {
    let bank_part = Split {
        imported: Some(delivered()),
        ..Split::new(checking, eur(-5320))
    };
    Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        Some("Corner Grocery".into()),
        vec![bank_part, Split::uncategorized(eur(5320))],
    )
    .expect("balanced")
}

/// `transaction` with other parts.
fn with_parts(transaction: &Transaction, parts: Vec<Split>) -> Transaction {
    Transaction::new(
        transaction.id(),
        transaction.booking_date(),
        transaction.payee().map(Into::into),
        parts,
    )
    .expect("balanced")
}

/// What the book held after each change, and the action each was.
struct Steps {
    contents: Vec<Vec<String>>,
    actions: Vec<Action>,
}

impl Steps {
    fn new(book: &Book) -> Self {
        Self {
            contents: vec![contents(book)],
            actions: Vec::new(),
        }
    }

    fn did(&mut self, book: &Book, action: Action) {
        self.contents.push(contents(book));
        self.actions.push(action);
    }
}

#[test]
fn every_change_can_be_undone_and_redone() {
    let mut book = Book::create_in_memory().expect("book created");
    let mut steps = Steps::new(&book);
    book.use_currency(EUR).expect("currency used");
    steps.did(&book, Action::UseCurrency);
    book.use_currency(USD).expect("currency used");
    steps.did(&book, Action::UseCurrency);
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    steps.did(&book, Action::AddAccount);
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD);
    steps.did(&book, Action::AddAccount);
    let food = category(&mut book, "Food", None);
    steps.did(&book, Action::AddCategory);
    let groceries = category(&mut book, "Groceries", None);
    steps.did(&book, Action::AddCategory);
    book.move_category(groceries, Some(food)).expect("moved");
    steps.did(&book, Action::MoveCategory);
    let unused = category(&mut book, "Unused", None);
    steps.did(&book, Action::AddCategory);
    book.remove_category(unused).expect("removed");
    steps.did(&book, Action::RemoveCategory);
    book.rename_account(groceries, "Groceries and drinks")
        .expect("renamed");
    steps.did(&book, Action::RenameCategory);
    book.rename_account(checking, "Main account")
        .expect("renamed");
    steps.did(&book, Action::RenameAccount);
    let rate = euros_in_dollars("1.0856");
    book.add_rate(&rate).expect("rate added");
    steps.did(&book, Action::AddRate);
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&shopping).expect("added");
    steps.did(&book, Action::AddTransaction);
    // 1000.00 EUR into the dollar account, converted with the rate.
    let received = Split {
        foreign: Some(ForeignAmount {
            amount: Money::from_minor_units(108_560, USD),
            rate: Some(rate),
        }),
        ..Split::new(dollars, eur(100_000))
    };
    let moved = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 7),
        Some("To the dollar account".into()),
        vec![Split::new(checking, eur(-100_000)), received],
    )
    .expect("balanced");
    book.add_transaction(&moved).expect("added");
    steps.did(&book, Action::AddTransaction);
    let purchase = imported_purchase(checking);
    book.add_transaction(&purchase).expect("added");
    steps.did(&book, Action::AddTransaction);
    let categorized = with_parts(
        &purchase,
        vec![
            purchase.splits()[0].clone(),
            Split {
                account: Some(groceries),
                ..purchase.splits()[1].clone()
            },
        ],
    );
    book.update_transaction(&categorized).expect("changed");
    steps.did(&book, Action::EditTransaction);
    // Its rate and its parts go with it.
    book.remove_transaction(moved.id()).expect("removed");
    steps.did(&book, Action::RemoveTransaction);
    // And so does what the bank delivered.
    book.remove_transaction(purchase.id()).expect("removed");
    steps.did(&book, Action::RemoveTransaction);
    book.set_rounding_mode(RoundingMode::HalfEven)
        .expect("rounding set");
    steps.did(&book, Action::SetRoundingMode);
    book.remove_rate(rate.id).expect("rate removed");
    steps.did(&book, Action::RemoveRate);
    let savings = account(&mut book, "Savings account", AccountKind::Asset, EUR);
    steps.did(&book, Action::AddAccount);
    book.close_account(savings).expect("closed");
    steps.did(&book, Action::CloseAccount);
    book.reopen_account(savings).expect("opened again");
    steps.did(&book, Action::ReopenAccount);
    // The two sides of a move into savings, joined into one Transfer: one
    // transaction goes, the other gets the savings part (D-087).
    let out = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 9),
        None,
        vec![
            Split {
                imported: Some(delivered()),
                ..Split::new(checking, eur(-20_000))
            },
            Split::uncategorized(eur(20_000)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&out).expect("added");
    steps.did(&book, Action::AddTransaction);
    let into = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 12),
        Some("From checking".into()),
        vec![
            Split {
                imported: Some(delivered()),
                ..Split::new(savings, eur(20_000))
            },
            Split::uncategorized(eur(-20_000)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&into).expect("added");
    steps.did(&book, Action::AddTransaction);
    book.join_transfer(out.id(), into.id()).expect("joined");
    steps.did(&book, Action::JoinTransfer);
    // Rules: a Rule with a sub-Rule, changed, moved, run on an imported
    // purchase and removed (D-088).
    let mut shops = Rule {
        id: RuleId::generate(),
        name: "Shops".into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::OtherPartyName,
            operator: TextOperator::Contains,
            value: "grocery".into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: Some(BookTo::Category(food)),
            payee: None,
        },
    };
    book.add_rule(&shops, None).expect("added");
    steps.did(&book, Action::AddRule);
    let smaller = Rule {
        id: RuleId::generate(),
        name: "Groceries".into(),
        conditions: vec![Condition::Amount(kassensturz_core::AmountTest::MoneyOut)],
        actions: Actions {
            book_to: Some(BookTo::Category(groceries)),
            payee: Some("Grocery store".into()),
        },
        ..shops.clone()
    };
    book.add_rule(&smaller, Some(shops.id)).expect("added");
    steps.did(&book, Action::AddRule);
    shops.match_mode = MatchMode::Any;
    book.update_rule(&shops).expect("changed");
    steps.did(&book, Action::EditRule);
    book.move_rule(smaller.id, None, 0).expect("moved");
    steps.did(&book, Action::MoveRule);
    book.move_rule(smaller.id, Some(shops.id), 0)
        .expect("moved back");
    steps.did(&book, Action::MoveRule);
    let bought = imported_purchase(checking);
    book.add_transaction(&bought).expect("added");
    steps.did(&book, Action::AddTransaction);
    let run = book.run_rules().expect("run");
    assert_eq!(run.changed.len(), 1);
    steps.did(&book, Action::RunRules);
    // Changed once more, with what it booked following the change (Q-044).
    let grocer = Rule {
        actions: Actions {
            payee: Some("Grocer".into()),
            ..smaller.actions.clone()
        },
        ..smaller.clone()
    };
    let rebooked = book.update_rule_and_rebook(&grocer).expect("changed");
    assert_eq!(rebooked.changed.len(), 1);
    steps.did(&book, Action::EditRule);
    book.remove_rule(shops.id).expect("removed");
    steps.did(&book, Action::RemoveRule);

    let expected: Vec<(Action, bool)> = steps
        .actions
        .iter()
        .rev()
        .map(|&action| (action, false))
        .collect();
    assert_eq!(actions(&book), expected);

    // Undo goes back through every earlier state, newest change first...
    for (index, &action) in steps.actions.iter().enumerate().rev() {
        assert_eq!(
            book.change_to_undo().expect("read").map(|c| c.action),
            Some(action)
        );
        let undone = book.undo().expect("undone").expect("a change to undo");
        assert_eq!((undone.action, undone.undone), (action, true));
        assert_eq!(contents(&book), steps.contents[index], "undoing {action:?}");
    }
    assert_eq!(book.undo().expect("nothing to undo"), None);
    assert_eq!(book.change_to_undo().expect("read"), None);

    // ...and redo forward again, oldest undone change first.
    for (index, &action) in steps.actions.iter().enumerate() {
        assert_eq!(
            book.change_to_redo().expect("read").map(|c| c.action),
            Some(action)
        );
        let redone = book.redo().expect("redone").expect("a change to redo");
        assert_eq!((redone.action, redone.undone), (action, false));
        assert_eq!(
            contents(&book),
            steps.contents[index + 1],
            "redoing {action:?}"
        );
    }
    assert_eq!(book.redo().expect("nothing to redo"), None);
    assert_eq!(actions(&book), expected);

    // What was written back reads as it was.
    book.undo().expect("undone");
    assert_eq!(book.rules().expect("Rules")[0].sub_rules[0].rule, grocer);
    let rebooked_payee = book.transaction(bought.id()).expect("read");
    assert_eq!(rebooked_payee.payee(), Some("Grocer"));
    book.undo().expect("undone");
    assert_eq!(book.rules().expect("Rules")[0].sub_rules[0].rule, smaller);
    let booked = book.transaction(bought.id()).expect("read");
    assert_eq!(booked.payee(), Some("Grocery store"));
    book.undo().expect("undone");
    assert_eq!(book.transaction(bought.id()).expect("back"), bought);
    for _ in 0..6 {
        book.undo().expect("undone");
    }
    assert_eq!(
        book.undo().expect("undone").map(|c| c.action),
        Some(Action::JoinTransfer)
    );
    assert_eq!(book.transaction(out.id()).expect("back"), out);
    assert_eq!(book.transaction(into.id()).expect("back"), into);
    book.undo().expect("undone");
    book.undo().expect("undone");
    assert_eq!(
        book.undo().expect("undone").map(|c| c.action),
        Some(Action::ReopenAccount)
    );
    assert!(book.account(savings).expect("account").closed);
    for _ in 0..5 {
        book.undo().expect("undone");
    }
    assert_eq!(book.transaction(purchase.id()).expect("back"), categorized);
    book.undo().expect("undone");
    assert_eq!(book.transaction(moved.id()).expect("back"), moved);
    book.undo().expect("undone");
    assert_eq!(book.transaction(purchase.id()).expect("back"), purchase);
}

#[test]
fn undo_and_redo_work_after_a_restart() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let mut book = Book::create(&path, &computer).expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let before = contents(&book);
    book.rename_account(checking, "Main account")
        .expect("renamed");
    let after = contents(&book);
    drop(book);

    let mut book = Book::open(&path, &computer).expect("book opened");
    let undone = book.undo().expect("undone").expect("the rename");
    assert_eq!(undone.action, Action::RenameAccount);
    assert_eq!(contents(&book), before);
    drop(book);

    let mut book = Book::open(&path, &computer).expect("book opened again");
    assert_eq!(book.change_to_redo().expect("read"), Some(undone.clone()));
    let redone = book.redo().expect("redone").expect("the rename");
    assert_eq!(
        redone,
        Change {
            undone: false,
            ..undone
        }
    );
    assert_eq!(contents(&book), after);
}

#[test]
fn the_history_lists_the_newest_change_first() {
    let start = Timestamp::now();
    let Sample { mut book, .. } = sample();
    book.undo().expect("undone");
    // Undone changes come first: redo can make them again.
    assert_eq!(
        actions(&book),
        [
            (Action::AddCategory, true),
            (Action::AddCategory, false),
            (Action::AddAccount, false),
            (Action::UseCurrency, false),
        ]
    );
    for change in book.history().expect("history read") {
        // In whole seconds, from this computer's clock.
        assert_eq!(change.made_at.subsec_nanosecond(), 0);
        assert!(change.made_at >= whole_seconds(start) && change.made_at <= Timestamp::now());
    }
    let ids: HashSet<ChangeId> = book
        .history()
        .expect("history read")
        .into_iter()
        .map(|change| change.id)
        .collect();
    assert_eq!(ids.len(), 4);
}

#[test]
fn changes_are_undone_in_the_order_they_were_made_whatever_the_clock_said() {
    let Sample {
        mut book, checking, ..
    } = sample();
    book.rename_account(checking, "Main account")
        .expect("renamed");
    // As if this computer's clock had been a year ahead, or the next change
    // had been made on a computer whose clock is behind. A year from today,
    // not a fixed date, which would one day be too old to be kept.
    let ahead =
        Timestamp::from_second(Timestamp::now().as_second() + 365 * 86_400).expect("a year ahead");
    book.conn
        .execute(
            "UPDATE main.changes SET made_at = ?1
             WHERE sequence = (SELECT max(sequence) FROM main.changes)",
            [ahead.to_string()],
        )
        .expect("time changed");
    book.rename_account(checking, "Everyday account")
        .expect("renamed");
    book.undo().expect("undone");
    assert_eq!(
        book.account(checking).expect("account").name,
        "Main account"
    );
    book.undo().expect("undone");
    assert_eq!(
        book.account(checking).expect("account").name,
        "Checking account"
    );
}

#[test]
fn a_new_change_ends_what_could_be_redone() {
    let Sample {
        mut book, checking, ..
    } = sample();
    book.rename_account(checking, "Main account")
        .expect("renamed");
    book.undo().expect("undone");
    let undone = book.change_to_redo().expect("read").expect("the rename");
    book.rename_account(checking, "Everyday account")
        .expect("renamed");
    assert_eq!(book.change_to_redo().expect("read"), None);
    assert_eq!(book.redo().expect("nothing to redo"), None);
    assert!(
        book.history()
            .expect("history read")
            .iter()
            .all(|change| change.id != undone.id)
    );
    assert_eq!(
        book.change_to_undo().expect("read").map(|c| c.action),
        Some(Action::RenameAccount)
    );
}

#[test]
fn a_change_that_changes_nothing_is_not_recorded() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let history = book.history().expect("history read");
    book.set_rounding_mode(RoundingMode::HalfAwayFromZero)
        .expect("same rounding");
    book.use_currency(EUR).expect("used already");
    book.rename_account(checking, "Checking account")
        .expect("same name");
    book.reopen_account(checking).expect("open already");
    assert_eq!(book.history().expect("history read"), history);

    // Nor does it end what could be redone: the book is still as the undo
    // left it.
    book.rename_account(checking, "Main account")
        .expect("renamed");
    book.undo().expect("undone");
    book.set_rounding_mode(RoundingMode::HalfAwayFromZero)
        .expect("same rounding");
    assert!(book.change_to_redo().expect("read").is_some());
    book.redo().expect("redone");
    assert_eq!(
        book.account(checking).expect("account").name,
        "Main account"
    );
}

#[test]
fn a_change_and_its_reversal_in_one_action_are_no_change() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let history = book.history().expect("history read");
    book.as_one_action(Action::RenameAccount, |book| {
        book.rename_account(checking, "Main account")?;
        book.rename_account(checking, "Checking account")?;
        // An account added and removed again.
        let id = account(book, "Temporary", AccountKind::Asset, EUR);
        book.conn
            .execute("DELETE FROM accounts WHERE id = ?1", [id.to_bytes()])?;
        Ok::<_, StoreError>(())
    })
    .expect("changed and changed back");
    assert_eq!(book.history().expect("history read"), history);
}

#[test]
fn statements_that_change_many_rows_at_once_are_recorded_completely() {
    let Sample {
        mut book, checking, ..
    } = sample();
    book.use_currency(USD).expect("currency used");
    book.add_rate(&euros_in_dollars("1.0856"))
        .expect("rate added");
    let before = contents(&book);
    book.as_one_action(Action::RemoveRate, |book| {
        // A row replaced in one statement: SQLite deletes the old one first.
        book.conn.execute(
            "INSERT OR REPLACE INTO accounts (id, name, kind, currency, closed)
             VALUES (?1, 'Main account', 'asset', 'EUR', 0)",
            [checking.to_bytes()],
        )?;
        // A whole table emptied at once.
        book.conn.execute("DELETE FROM prices", [])?;
        Ok::<_, StoreError>(())
    })
    .expect("changed");
    assert_eq!(book.rates(EUR, USD).expect("rates"), Vec::new());
    book.undo().expect("undone");
    assert_eq!(contents(&book), before);
}

#[test]
fn one_user_action_can_take_several_calls() {
    let Sample { mut book, .. } = sample();
    let before = contents(&book);
    let changes = book.history().expect("history read").len();
    // Adding an account in a currency the book doesn't use yet.
    book.as_one_action(Action::AddAccount, |book| {
        book.use_currency(USD)?;
        Ok::<_, StoreError>(account(book, "Dollar account", AccountKind::Asset, USD))
    })
    .expect("added");
    let history = book.history().expect("history read");
    assert_eq!(history.len(), changes + 1);
    assert_eq!(history[0].action, Action::AddAccount);
    book.undo().expect("undone");
    assert_eq!(contents(&book), before);
}

#[test]
fn a_failed_user_action_changes_and_records_nothing() {
    let Sample { mut book, .. } = sample();
    let before = contents(&book);
    let history = book.history().expect("history read");
    let failed: Result<(), StoreError> = book.as_one_action(Action::AddAccount, |book| {
        book.use_currency(USD)?;
        Err(StoreError::EmptyAccountName)
    });
    assert!(matches!(failed, Err(StoreError::EmptyAccountName)));
    assert_eq!(contents(&book), before);
    assert_eq!(book.history().expect("history read"), history);

    // A part of an action that fails is taken back; the rest of the action
    // is recorded.
    let jpy = CurrencyCode::new("JPY").expect("valid code");
    book.as_one_action(Action::AddAccount, |book| {
        let part: Result<(), StoreError> = book.as_one_action(Action::UseCurrency, |book| {
            book.use_currency(USD)?;
            Err(StoreError::EmptyAccountName)
        });
        assert!(part.is_err());
        book.use_currency(jpy)?;
        Ok::<_, StoreError>(())
    })
    .expect("changed");
    assert!(matches!(
        book.currency_minor_digits(USD),
        Err(StoreError::UnknownCurrency(_))
    ));
    assert_eq!(book.currency_minor_digits(jpy).expect("used"), 0);
    assert_eq!(
        book.change_to_undo().expect("read").map(|c| c.action),
        Some(Action::AddAccount)
    );
    book.undo().expect("undone");
    assert_eq!(contents(&book), before);
}

#[test]
fn a_dry_run_shows_what_a_change_would_do_and_keeps_nothing() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let before = contents(&book);
    let history = book.history().expect("history read");
    let added = book
        .dry_run(|book| {
            book.use_currency(USD)?;
            let id = account(book, "Dollar account", AccountKind::Asset, USD);
            // Inside, it is there, as in a real action.
            assert_eq!(book.account(id)?.currency, USD);
            Ok::<_, StoreError>(id)
        })
        .expect("ran");
    assert!(matches!(
        book.account(added),
        Err(StoreError::UnknownAccount(_))
    ));
    assert_eq!(contents(&book), before);
    assert_eq!(book.history().expect("history read"), history);
    // Afterwards, changes outside a user action are refused again.
    assert!(
        book.conn
            .execute(
                "UPDATE accounts SET name = 'Renamed' WHERE id = ?1",
                [checking.to_bytes()],
            )
            .is_err()
    );

    // What comes next is recorded on its own, as if there had been no dry
    // run.
    account(&mut book, "Second account", AccountKind::Asset, EUR);
    assert_eq!(history.len() + 1, book.history().expect("read").len());
    book.undo().expect("undone");
    assert_eq!(contents(&book), before);

    let failed: Result<(), StoreError> = book.dry_run(|book| {
        book.use_currency(USD)?;
        Err(StoreError::EmptyAccountName)
    });
    assert!(matches!(failed, Err(StoreError::EmptyAccountName)));
    assert_eq!(contents(&book), before);

    // Inside a user action, only what the dry run did is taken back.
    let jpy = CurrencyCode::new("JPY").expect("valid code");
    book.as_one_action(Action::UseCurrency, |book| {
        book.use_currency(jpy)?;
        book.dry_run(|book| book.use_currency(USD))?;
        Ok::<_, StoreError>(())
    })
    .expect("changed");
    assert_eq!(book.currency_minor_digits(jpy).expect("used"), 0);
    assert!(matches!(
        book.currency_minor_digits(USD),
        Err(StoreError::UnknownCurrency(_))
    ));
    book.undo().expect("undone");
    assert_eq!(contents(&book), before);
}

#[test]
fn an_edit_records_only_the_rows_it_changed() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let purchase = imported_purchase(checking);
    book.add_transaction(&purchase).expect("added");
    // The user picks a category for the part without one. The store writes
    // all parts again, but only that one changed.
    let categorized = with_parts(
        &purchase,
        vec![
            purchase.splits()[0].clone(),
            Split {
                account: Some(groceries),
                ..purchase.splits()[1].clone()
            },
        ],
    );
    book.update_transaction(&categorized).expect("changed");
    let change = book.change_to_undo().expect("read").expect("the edit");
    let mut kept = Vec::new();
    for table in &RECORDED {
        let rows: i64 = book
            .conn
            .query_row(
                &format!(
                    "SELECT count(*) FROM main.changed_{} WHERE change_id = ?1",
                    table.name
                ),
                [change.id.to_bytes()],
                |row| row.get(0),
            )
            .expect("rows counted");
        if rows > 0 {
            kept.push((table.name, rows));
        }
    }
    // The part's values before and after.
    assert_eq!(kept, [("splits", 2)]);
}

#[test]
fn a_summary_says_which_transactions_a_change_touched() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    let summary = |book: &Book| {
        let change = book.change_to_undo().expect("read").expect("a change");
        book.change_summary(change.id).expect("summary read")
    };
    let sorted = |mut accounts: Vec<AccountId>| {
        accounts.sort();
        accounts
    };
    // Adding a category touches no transaction.
    assert_eq!(summary(&book), ChangeSummary::default());

    let purchase = imported_purchase(checking);
    let pay = transaction(
        date(2026, 9, 30),
        "Employer",
        &[(checking, eur(250_000)), (salary, eur(-250_000))],
    );
    book.as_one_action(Action::ImportStatement, |book| {
        book.add_transaction(&purchase)?;
        book.add_transaction(&pay)
    })
    .expect("imported");
    let import = book.change_to_undo().expect("read").expect("the import");
    // The purchase's part without a category is booked to no account.
    let imported = ChangeSummary {
        added: 2,
        accounts: sorted(vec![checking, salary]),
        dates: Some((date(2026, 9, 30), date(2026, 10, 5))),
        ..ChangeSummary::default()
    };
    assert_eq!(summary(&book), imported);

    // Only the part without a category changes, but the summary names
    // every account of the transaction.
    let categorized = with_parts(
        &purchase,
        vec![
            purchase.splits()[0].clone(),
            Split {
                account: Some(groceries),
                ..purchase.splits()[1].clone()
            },
        ],
    );
    book.update_transaction(&categorized).expect("changed");
    let edit = book.change_to_undo().expect("read").expect("the edit");
    let edited = ChangeSummary {
        changed: 1,
        accounts: sorted(vec![checking, groceries]),
        dates: Some((date(2026, 10, 5), date(2026, 10, 5))),
        ..ChangeSummary::default()
    };
    assert_eq!(summary(&book), edited);

    book.remove_transaction(pay.id()).expect("removed");
    let removal = book.change_to_undo().expect("read").expect("the removal");
    let removed = ChangeSummary {
        removed: 1,
        accounts: sorted(vec![checking, salary]),
        dates: Some((date(2026, 9, 30), date(2026, 9, 30))),
        ..ChangeSummary::default()
    };
    assert_eq!(summary(&book), removed);

    // Undone, each says the same, so redo can name it too.
    for (change, expected) in [(removal, removed), (edit, edited), (import, imported)] {
        book.undo().expect("undone");
        assert_eq!(
            book.change_to_redo().expect("read"),
            Some(Change {
                undone: true,
                ..change.clone()
            })
        );
        assert_eq!(
            book.change_summary(change.id).expect("summary read"),
            expected
        );
    }
}

#[test]
fn a_change_keeps_the_name_of_the_file_it_read() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let mut book = Book::create(&path, &computer).expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let import = |book: &mut Book, file: &str| {
        let purchase = imported_purchase(checking);
        book.as_one_action(Action::ImportStatement, |book| {
            book.note_file(Path::new(file))?;
            book.add_transaction(&purchase)
        })
    };
    let file_name = |book: &Book| {
        book.change_to_undo()
            .expect("read")
            .expect("a change")
            .file_name
    };
    // Changes that read no file have none.
    assert_eq!(file_name(&book), None);
    // Only the name: the folder can tell more about the user than the
    // history needs.
    import(&mut book, "/home/someone/Bank statements/october.xml").expect("imported");
    assert_eq!(file_name(&book).as_deref(), Some("october.xml"));
    drop(book);

    let mut book = Book::open(&path, &computer).expect("book opened");
    assert_eq!(file_name(&book).as_deref(), Some("october.xml"));
    // Kept in memory with the rest of the history, and back in the book.
    book.set_history_retention(HistoryRetention::Days(0))
        .expect("kept in memory");
    assert_eq!(file_name(&book).as_deref(), Some("october.xml"));
    book.set_history_retention(HistoryRetention::Days(30))
        .expect("kept in the book");
    assert_eq!(file_name(&book).as_deref(), Some("october.xml"));
    let undone = book.undo().expect("undone").expect("the import");
    assert_eq!(undone.file_name.as_deref(), Some("october.xml"));

    // An action that changes nothing, a dry run and a failed action keep
    // no name.
    let history = book.history().expect("history read");
    book.as_one_action(Action::ImportStatement, |book| {
        book.note_file(Path::new("november.xml"))
    })
    .expect("nothing imported");
    book.dry_run(|book| import(book, "november.xml"))
        .expect("previewed");
    let failed: Result<(), StoreError> = book.as_one_action(Action::ImportStatement, |book| {
        book.note_file(Path::new("november.xml"))?;
        Err(StoreError::Corrupt(Corruption::HistoryMismatch))
    });
    assert!(failed.is_err());
    assert_eq!(book.history().expect("history read"), history);
    // The next action names its own file, or none.
    book.rename_account(checking, "Main account")
        .expect("renamed");
    assert_eq!(file_name(&book), None);
}

#[test]
#[should_panic(expected = "only a user action can note the file it reads")]
fn only_a_user_action_notes_a_file() {
    let Sample { mut book, .. } = sample();
    let _ = book.note_file(Path::new("october.xml"));
}

#[test]
fn a_change_outside_a_user_action_is_refused() {
    let Sample { book, checking, .. } = sample();
    let before = contents(&book);
    let refused = book.conn.execute(
        "UPDATE accounts SET name = 'Renamed' WHERE id = ?1",
        [checking.to_bytes()],
    );
    let Err(error) = refused else {
        panic!("a change escaped the history");
    };
    assert!(
        error
            .to_string()
            .contains("KassenSturz changed a book outside a user action"),
        "{error}"
    );
    assert_eq!(contents(&book), before);
}

#[test]
fn undo_refuses_a_book_that_another_program_changed() {
    let Sample {
        mut book, checking, ..
    } = sample();
    book.rename_account(checking, "Main account")
        .expect("renamed");
    // Another program renames it again, outside the history (D-046).
    book.as_another_program()
        .execute(
            "UPDATE accounts SET name = 'Other name' WHERE id = ?1",
            [checking.to_bytes()],
        )
        .expect("renamed behind KassenSturz's back");
    let before = contents(&book);
    let history = book.history().expect("history read");
    assert!(matches!(
        book.undo(),
        Err(StoreError::Corrupt(Corruption::HistoryMismatch))
    ));
    // Nothing was changed, and foreign keys are checked again.
    assert_eq!(contents(&book), before);
    assert_eq!(book.history().expect("history read"), history);
    let foreign_keys: i64 = book
        .conn
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .expect("pragma read");
    assert_eq!(foreign_keys, 1);

    // Cleaning up the history removes what doesn't fit; the book stays as
    // it is and takes new changes that can be undone.
    book.clean_up_history().expect("cleaned up");
    assert_eq!(book.undo().expect("nothing to undo"), None);
    assert_eq!(contents(&book), before);
    book.rename_account(checking, "Main account")
        .expect("renamed");
    book.undo().expect("undone");
    assert_eq!(contents(&book), before);
}

#[test]
fn undo_refuses_to_break_a_reference() {
    let Sample { mut book, .. } = sample();
    let food = category(&mut book, "Food", None);
    // Another program adds a category inside it, outside the history.
    book.as_another_program()
        .execute(
            "INSERT INTO accounts (id, name, kind, currency, closed, parent_id)
             VALUES (?1, 'Groceries', 'expense', 'EUR', 0, ?2)",
            [AccountId::generate().to_bytes(), food.to_bytes()],
        )
        .expect("added behind KassenSturz's back");
    let before = contents(&book);
    // Undoing would remove Food, which Groceries sits inside.
    assert!(matches!(
        book.undo(),
        Err(StoreError::Corrupt(Corruption::HistoryMismatch))
    ));
    assert_eq!(contents(&book), before);
    assert_eq!(
        book.change_to_undo().expect("read").map(|c| c.action),
        Some(Action::AddCategory)
    );
}

#[test]
fn redo_refuses_to_break_a_rule_of_the_book() {
    let Sample { mut book, .. } = sample();
    book.use_currency(USD).expect("currency used");
    book.add_rate(&euros_in_dollars("1.0856"))
        .expect("rate added");
    book.undo().expect("undone");
    // Another program adds a rate for the same currencies and day, which
    // the book allows only once.
    let other = euros_in_dollars("1.0900");
    book.as_another_program()
        .execute(
            "INSERT INTO prices
                 (id, date, from_currency, to_currency, rate_digits, rate_decimal_places, source)
             VALUES (?1, '2026-10-01', 'EUR', 'USD', 10900, 4, 'manual')",
            [other.id.to_bytes()],
        )
        .expect("added behind KassenSturz's back");
    let before = contents(&book);
    assert!(matches!(
        book.redo(),
        Err(StoreError::Corrupt(Corruption::HistoryMismatch))
    ));
    assert_eq!(contents(&book), before);
    assert_eq!(
        book.change_to_redo().expect("read").map(|c| c.action),
        Some(Action::AddRate)
    );
}

#[test]
#[should_panic(expected = "undo and redo can't be part of a user action")]
fn undo_is_not_part_of_a_user_action() {
    let Sample { mut book, .. } = sample();
    let _ = book.as_one_action(Action::RenameAccount, Book::undo);
}

#[test]
fn the_history_is_kept_30_days_unless_the_user_chooses_otherwise() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let mut book = Book::create(&path, &computer).expect("book created");
    assert_eq!(
        book.history_retention().expect("read"),
        HistoryRetention::Days(30)
    );
    assert_eq!(HistoryRetention::default(), HistoryRetention::Days(30));
    for keep in [
        HistoryRetention::Forever,
        HistoryRetention::Days(0),
        HistoryRetention::Days(1),
        HistoryRetention::Days(u32::MAX),
        HistoryRetention::Days(30),
    ] {
        book.set_history_retention(keep).expect("set");
        assert_eq!(book.history_retention().expect("read"), keep);
        // Stored in the book, so every computer keeps the same history.
        drop(book);
        book = Book::open(&path, &computer).expect("book opened");
        assert_eq!(book.history_retention().expect("read"), keep);
    }
}

#[test]
fn changes_older_than_the_book_keeps_them_are_deleted() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let mut book = Book::create(&path, &computer).expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    book.rename_account(checking, "Main account")
        .expect("renamed");
    let renamed = contents(&book);
    // As if the account had been added long ago, and the currency used on a
    // computer whose clock was far ahead.
    book.conn
        .execute_batch(
            "UPDATE main.changes SET made_at = '2999-01-01T00:00:00Z' WHERE sequence = 1;
             UPDATE main.changes SET made_at = '2000-01-01T00:00:00Z' WHERE sequence = 2;",
        )
        .expect("times changed");
    drop(book);

    // Deleted when the book is opened, from the oldest change up to the
    // newest one that is too old, so the history has no gaps.
    let mut book = Book::open(&path, &computer).expect("book opened");
    assert_eq!(actions(&book), [(Action::RenameAccount, false)]);
    book.undo().expect("undone");
    assert_eq!(book.undo().expect("nothing older"), None);
    book.redo().expect("redone");
    assert_eq!(contents(&book), renamed);

    // And when the user keeps it for a shorter time.
    book.conn
        .execute(
            "UPDATE main.changes SET made_at = '2000-01-01T00:00:00Z'",
            [],
        )
        .expect("time changed");
    book.set_history_retention(HistoryRetention::Forever)
        .expect("set");
    assert_eq!(book.history().expect("history read").len(), 1);
    book.set_history_retention(HistoryRetention::Days(1))
        .expect("set");
    assert_eq!(book.history().expect("history read"), Vec::new());
}

#[test]
fn with_0_days_the_history_is_kept_in_memory_until_the_book_is_closed() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let mut book = Book::create(&path, &computer).expect("book created");
    book.use_currency(EUR).expect("currency used");
    book.set_history_retention(HistoryRetention::Days(0))
        .expect("set");
    // Moved out of the book into memory, never into a temporary file.
    assert_eq!(changes_in_the_file(&book), 0);
    assert_eq!(actions(&book), [(Action::UseCurrency, false)]);
    let temp_store: i64 = book
        .conn
        .pragma_query_value(None, "temp_store", |row| row.get(0))
        .expect("pragma read");
    assert_eq!(temp_store, 2, "MEMORY");

    // Changes can still be undone and redone while the book is open.
    let before = contents(&book);
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let after = contents(&book);
    assert_eq!(changes_in_the_file(&book), 0);
    book.undo().expect("undone");
    assert_eq!(contents(&book), before);
    book.redo().expect("redone");
    assert_eq!(contents(&book), after);
    drop(book);

    let mut book = Book::open(&path, &computer).expect("book opened");
    assert_eq!(book.history().expect("history read"), Vec::new());
    book.rename_account(checking, "Main account")
        .expect("renamed");
    // Back into the book when it is kept again.
    book.set_history_retention(HistoryRetention::Days(30))
        .expect("set");
    assert_eq!(changes_in_the_file(&book), 1);
    drop(book);
    let mut book = Book::open(&path, &computer).expect("book opened again");
    assert_eq!(actions(&book), [(Action::RenameAccount, false)]);
    book.undo().expect("undone");
    assert_eq!(contents(&book), after);
}

#[test]
fn an_erased_history_is_gone_from_the_file() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let file_holds = |text: &str| {
        let bytes = std::fs::read(&path).expect("book read");
        bytes
            .windows(text.len())
            .any(|window| window == text.as_bytes())
    };
    let payee = "Example Payee To Be Erased";
    let mut book = Book::create(&path, &computer).expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let groceries = account(&mut book, "Groceries", AccountKind::Expense, EUR);
    let shopping = transaction(
        date(2026, 10, 5),
        payee,
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&shopping).expect("added");
    book.remove_transaction(shopping.id()).expect("removed");
    drop(book);
    // The removal can be undone, so the history still holds the payee.
    assert!(file_holds(payee));

    let mut book = Book::open(&path, &computer).expect("book opened");
    book.clean_up_history().expect("cleaned up");
    assert_eq!(book.history().expect("history read"), Vec::new());
    assert_eq!(book.undo().expect("nothing to undo"), None);
    drop(book);
    // SQLite overwrites what is deleted, so it is gone from the file.
    assert!(!file_holds(payee));

    // With 0 days, the history never reaches the file.
    let mut book = Book::open(&path, &computer).expect("book opened again");
    book.set_history_retention(HistoryRetention::Days(0))
        .expect("set");
    book.add_transaction(&shopping).expect("added");
    book.remove_transaction(shopping.id()).expect("removed");
    assert_eq!(book.history().expect("history read").len(), 2);
    drop(book);
    assert!(!file_holds(payee));
}

#[test]
fn the_history_records_every_table_with_every_column() {
    let book = Book::create_in_memory().expect("book created");
    let columns = |schema: &str, table: &str| -> Vec<(String, String, i64)> {
        let mut statement = book
            .conn
            .prepare("SELECT name, type, pk FROM pragma_table_info(?1, ?2) ORDER BY cid")
            .expect("columns read");
        statement
            .query_map([table, schema], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .expect("columns read")
            .collect::<Result<_, _>>()
            .expect("columns read")
    };
    let mut statement = book
        .conn
        .prepare(
            "SELECT name FROM main.sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .expect("tables read");
    let tables: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .expect("tables read")
        .collect::<Result<_, _>>()
        .expect("tables read");
    // The history's own tables aren't recorded.
    let data: Vec<&String> = tables
        .iter()
        .filter(|name| {
            !["changes", "history_settings"].contains(&name.as_str())
                && !name.starts_with("changed_")
        })
        .collect();
    let mut recorded: Vec<&str> = RECORDED.iter().map(|table| table.name).collect();
    recorded.sort_unstable();
    assert_eq!(data, recorded);
    // The list of changes in memory is like the one in the book.
    assert_eq!(columns("temp", "changes"), columns("main", "changes"));

    for table in &RECORDED {
        let name = table.name;
        let own: Vec<(String, String, i64)> = table
            .columns
            .iter()
            .map(|&(column, kind)| (column.into(), kind.into(), i64::from(column == table.key)))
            .collect();
        assert_eq!(columns("main", name), own, "{name}");
        // Copies of its rows, kept with the change and the side, in the
        // book and in memory.
        let mut copies = vec![
            ("change_id".to_owned(), "BLOB".to_owned(), 1),
            ("side".to_owned(), "TEXT".to_owned(), 3),
        ];
        copies.extend(
            own.iter()
                .map(|(column, kind, key)| (column.clone(), kind.clone(), key * 2)),
        );
        let changed = format!("changed_{name}");
        assert_eq!(columns("main", &changed), copies, "{changed}");
        assert_eq!(columns("temp", &changed), copies, "temp.{changed}");
        let mut staged = vec![("side".to_owned(), "TEXT".to_owned(), 2)];
        staged.extend(own);
        assert_eq!(columns("temp", &format!("staged_{name}")), staged);
    }
}

#[test]
fn every_action_can_be_stored_and_no_other() {
    let book = Book::create_in_memory().expect("book created");
    for schema in ["main", "temp"] {
        let store = |sequence: i64, action: &str| {
            book.conn.execute(
                &format!(
                    "INSERT INTO {schema}.changes (id, sequence, action, made_at, undone)
                     VALUES (randomblob(16), ?1, ?2, '2026-10-19T12:00:00Z', 0)"
                ),
                params![sequence, action],
            )
        };
        for (sequence, action) in (1..).zip(Action::ALL) {
            store(sequence, action_to_text(action)).expect("stored");
            assert_eq!(action_from_text(action_to_text(action)).ok(), Some(action));
        }
        assert!(store(100, "import_file").is_err(), "{schema}");
    }
}

#[test]
fn a_damaged_history_is_reported() {
    let Sample { mut book, .. } = sample();
    // As if another program had changed the book (D-046).
    book.conn
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE main.changes SET action = 'import_file' WHERE sequence = 1;",
        )
        .expect("changed");
    assert!(matches!(
        book.history(),
        Err(StoreError::Corrupt(Corruption::BadAction(text))) if text == "import_file"
    ));
    book.conn
        .execute_batch(
            "UPDATE main.changes SET action = 'use_currency' WHERE sequence = 1;
             UPDATE main.changes SET made_at = '2026-13-45T25:61:61Z' WHERE sequence = 1;",
        )
        .expect("changed");
    assert!(matches!(
        book.history(),
        Err(StoreError::Corrupt(Corruption::BadTimestamp(text))) if text == "2026-13-45T25:61:61Z"
    ));
    book.conn
        .execute("UPDATE main.history_settings SET keep_days = -1", [])
        .expect("changed");
    assert!(matches!(
        book.history_retention(),
        Err(StoreError::Corrupt(Corruption::BadHistoryRetention(-1)))
    ));
    // KassenSturz doesn't guess how long the user wants it kept: nothing is
    // recorded, so nothing is changed.
    assert!(matches!(
        book.set_rounding_mode(RoundingMode::HalfEven),
        Err(StoreError::Corrupt(Corruption::BadHistoryRetention(-1)))
    ));
    assert_eq!(
        book.rounding_mode().expect("read"),
        RoundingMode::HalfAwayFromZero
    );
    book.conn
        .execute("DELETE FROM main.history_settings", [])
        .expect("deleted");
    assert!(matches!(
        book.history_retention(),
        Err(StoreError::Corrupt(Corruption::MissingSettings))
    ));
}

#[test]
fn a_book_without_its_history_setting_is_reported_when_opened() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let book = Book::create(&path, &computer).expect("book created");
    book.conn
        .execute("DELETE FROM main.history_settings", [])
        .expect("deleted");
    drop(book);
    assert!(matches!(
        Book::open(&path, &computer),
        Err(StoreError::Corrupt(Corruption::MissingSettings))
    ));
    assert!(!lock_path(&path).exists());
}

/// How long `count` more transactions take to add, each as an action of its
/// own.
fn time_to_add(book: &mut Book, checking: AccountId, food: AccountId, count: u32) -> f64 {
    let started = std::time::Instant::now();
    for index in 0..count {
        let day = i8::try_from(index % 28 + 1).expect("a day");
        let amount = 100 + i64::from(day);
        book.add_transaction(&transaction(
            date(2026, 1, day),
            "Shop",
            &[(checking, eur(-amount)), (food, eur(amount))],
        ))
        .expect("added");
    }
    started.elapsed().as_secs_f64()
}

#[test]
fn an_action_costs_the_same_however_long_the_history_is() {
    // #84: every action looked through the whole history for undone changes,
    // so a book filled one action at a time slowed down with every action.
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency");
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR);
    let food = account(&mut book, "Food", AccountKind::Expense, EUR);
    let short = (0..3)
        .map(|_| time_to_add(&mut book, checking, food, 40))
        .fold(f64::MAX, f64::min);
    // A long history, made directly: as if 20,000 actions had come before.
    book.conn
        .execute(
            "WITH RECURSIVE number (n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM number WHERE n < 20000)
             INSERT INTO main.changes (id, sequence, action, made_at, undone)
             SELECT randomblob(16), 1000000 + n, 'add_transaction', ?1, 0 FROM number",
            [whole_seconds(Timestamp::now()).to_string()],
        )
        .expect("history made");
    let long = (0..3)
        .map(|_| time_to_add(&mut book, checking, food, 40))
        .fold(f64::MAX, f64::min);
    // Before the fix, it was ten times as long; noise doesn't reach three.
    assert!(
        long < short * 3.0,
        "40 actions took {short:.3} s with a short history and {long:.3} s with a long one"
    );
}
