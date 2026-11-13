// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenario B5 of the outside security review: a hostile book file
//! that carries SQL of its own (a trigger, a view, an index) or a planted
//! change history. All data is made up (AGENTS.md §3.1 rule 2).
//!
//! Each test writes into a synthetic book with SQLite directly, the way a
//! sync folder, a partner or "Start a book from a file" could hand it over,
//! and then opens it with KassenSturz. The pass condition is the review's:
//! the book is refused at open, or the planted data is refused, and nothing
//! of the user's data is lost.

use std::path::{Path, PathBuf};

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::{Book, Computer};
use rusqlite::Connection;

/// A folder of its own for one test, removed again afterwards.
struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("kassensturz-attack-{}", AccountId::generate()));
        std::fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn computer(&self) -> Computer {
        Computer {
            name: "Laptop".into(),
            backup_folder: self.join("Backups"),
        }
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").expect("valid code")
}

fn account(name: &str, kind: AccountKind) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: eur(),
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    }
}

/// What a synthetic book holds after [`fill`].
struct Filled {
    checking: AccountId,
    groceries: AccountId,
    shopping: TransactionId,
}

/// A new book at `path` with one account, one category and two
/// transactions, closed again.
fn fill(path: &Path, computer: &Computer) -> Filled {
    let mut book = Book::create(path, computer).expect("book created");
    book.use_currency(eur()).expect("currency used");
    let checking = account("Checking account", AccountKind::Asset);
    let groceries = account("Groceries", AccountKind::Expense);
    book.add_account(&checking).expect("account added");
    book.add_account(&groceries).expect("category added");
    let mut shopping = None;
    for (day, cents) in [(5, 5320), (12, 1999)] {
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::new(2026, 10, day).expect("valid date"),
            Some("Corner Grocery".into()),
            vec![
                Split::new(checking.id, Money::from_minor_units(-cents, eur())),
                Split::new(groceries.id, Money::from_minor_units(cents, eur())),
            ],
        )
        .expect("balanced");
        book.add_transaction(&transaction)
            .expect("transaction added");
        shopping.get_or_insert(transaction.id());
    }
    Filled {
        checking: checking.id,
        groceries: groceries.id,
        shopping: shopping.expect("one added"),
    }
}

/// Runs `sql` on the book at `path` with SQLite alone, as another program
/// would.
fn plant(path: &Path, sql: &str) {
    let conn = Connection::open(path).expect("book opened by SQLite");
    conn.execute_batch(sql).expect("planted");
}

fn split_count(path: &Path) -> i64 {
    let conn = Connection::open(path).expect("book opened by SQLite");
    conn.query_row("SELECT count(*) FROM splits", [], |row| row.get(0))
        .expect("counted")
}

/// Opens the book with KassenSturz and, if that works, makes one ordinary
/// change: the payee of `shopping` is corrected.
fn open_and_edit(path: &Path, computer: &Computer, filled: &Filled) -> Result<(), String> {
    let mut book = Book::open(path, computer).map_err(|error| format!("{error:?}"))?;
    let stored = book.transaction(filled.shopping).expect("transaction read");
    let corrected = Transaction::new(
        stored.id(),
        stored.booking_date(),
        Some("Corner Grocery Ltd".into()),
        stored.splits().to_vec(),
    )
    .expect("balanced")
    .with_details_of(&stored);
    book.update_transaction(&corrected)
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}

/// The review's pass condition for SQL planted in the book: refused at open,
/// and every part of every transaction still there.
fn assert_refused_and_kept(path: &Path, computer: &Computer, filled: &Filled, what: &str) {
    let parts = split_count(path);
    let opened = Book::open(path, computer);
    let refused = opened.is_err();
    drop(opened);
    if !refused {
        // Show what the planted SQL does once the book is used.
        let _ = open_and_edit(path, computer, filled);
    }
    let after = split_count(path);
    assert!(
        refused,
        "a book with {what} was opened; parts before {parts}, after one edit {after}"
    );
    assert_eq!(after, parts, "parts were lost");
}

#[test]
fn b5_a_book_with_a_trigger_that_deletes_parts_is_refused() {
    let folder = TempFolder::new();
    let path = folder.join("Hostile.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    plant(
        &path,
        "CREATE TRIGGER t AFTER UPDATE ON transactions BEGIN DELETE FROM splits; END;",
    );
    assert_refused_and_kept(&path, &laptop, &filled, "a trigger that deletes every part");
}

#[test]
fn b5_a_book_with_a_trigger_that_changes_amounts_is_refused() {
    let folder = TempFolder::new();
    let path = folder.join("Hostile.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    // Quietly falsifies money instead of deleting it: harder to notice.
    plant(
        &path,
        "CREATE TRIGGER t AFTER UPDATE ON transactions BEGIN \
           UPDATE splits SET amount = amount * 10 WHERE transaction_id = NEW.id; END;",
    );
    let opened = Book::open(&path, &laptop);
    let refused = opened.is_err();
    drop(opened);
    if !refused {
        let _ = open_and_edit(&path, &laptop, &filled);
    }
    let book = Book::open(&path, &laptop);
    if let Ok(book) = book {
        assert_eq!(
            book.balance(filled.checking).expect("balance"),
            Money::from_minor_units(-7319, eur()),
            "the planted trigger changed the balance"
        );
    }
    assert!(
        refused,
        "a book with a trigger that changes amounts was opened"
    );
}

#[test]
fn b5_a_book_with_an_extra_view_is_refused() {
    let folder = TempFolder::new();
    let path = folder.join("Hostile.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    plant(&path, "CREATE VIEW leak AS SELECT * FROM splits;");
    assert_refused_and_kept(&path, &laptop, &filled, "a view that isn't in the layout");
}

#[test]
fn b5_a_book_with_an_extra_index_is_refused() {
    let folder = TempFolder::new();
    let path = folder.join("Hostile.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    plant(&path, "CREATE INDEX extra ON splits(amount);");
    assert_refused_and_kept(&path, &laptop, &filled, "an index that isn't in the layout");
}

#[test]
fn b5_a_book_with_an_extra_table_is_refused() {
    let folder = TempFolder::new();
    let path = folder.join("Hostile.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    plant(&path, "CREATE TABLE stash (x BLOB);");
    assert_refused_and_kept(&path, &laptop, &filled, "a table that isn't in the layout");
}

#[test]
fn b5_a_book_whose_own_trigger_was_dropped_is_refused() {
    // The layout's own guards (D-375) are part of what the book must hold;
    // a book without them could let an original be changed unnoticed.
    let folder = TempFolder::new();
    let path = folder.join("Hostile.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    let conn = Connection::open(&path).expect("book opened by SQLite");
    let guards: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'trigger' ORDER BY name")
        .expect("prepared")
        .query_map([], |row| row.get(0))
        .expect("read")
        .collect::<Result<_, _>>()
        .expect("names");
    drop(conn);
    let Some(guard) = guards.first() else {
        // The layout has no triggers of its own: nothing to drop.
        return;
    };
    plant(&path, &format!("DROP TRIGGER \"{guard}\";"));
    let opened = Book::open(&path, &laptop);
    let refused = opened.is_err();
    drop(opened);
    let _ = filled;
    assert!(refused, "a book without its own trigger {guard} was opened");
}

/// Edits `shopping` once with KassenSturz, so the history holds a change
/// whose `before` copies Undo would write back.
fn edit_once(path: &Path, computer: &Computer, filled: &Filled) {
    open_and_edit(path, computer, filled).expect("ordinary edit");
}

/// Opens the book and presses Undo once; returns whether Undo refused.
fn undo_refused(path: &Path, computer: &Computer) -> bool {
    let mut book = Book::open(path, computer).expect("book opened");
    book.undo().is_err()
}

#[test]
fn b5_undo_refuses_a_planted_part_that_points_to_no_account() {
    let folder = TempFolder::new();
    let path = folder.join("Planted.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    edit_once(&path, &laptop, &filled);
    // The edit changed the payee only, so plant a `before` copy of a part
    // as if the edit had also changed it, pointing to an account that
    // doesn't exist.
    plant(
        &path,
        "INSERT INTO changed_splits
           SELECT c.id, side.name, s.*
           FROM (SELECT id FROM changes ORDER BY sequence DESC LIMIT 1) AS c,
                (SELECT * FROM splits WHERE account_id IS NOT NULL LIMIT 1) AS s,
                (SELECT 'before' AS name UNION SELECT 'after') AS side;
         UPDATE changed_splits SET account_id = randomblob(16) WHERE side = 'before';",
    );
    let before = split_count(&path);
    assert!(
        undo_refused(&path, &laptop),
        "Undo wrote back a part of an account that doesn't exist"
    );
    assert_eq!(split_count(&path), before);
    let book = Book::open(&path, &laptop).expect("book opened");
    assert_eq!(
        book.balance(filled.checking).expect("balance"),
        Money::from_minor_units(-7319, eur())
    );
    assert_eq!(
        book.balance(filled.groceries).expect("balance"),
        Money::from_minor_units(7319, eur())
    );
}

#[test]
fn b5_undo_refuses_a_planted_unbalanced_transaction() {
    let folder = TempFolder::new();
    let path = folder.join("Planted.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    edit_once(&path, &laptop, &filled);
    // A `before` and an `after` copy of one part, the `after` one equal to
    // the book and the `before` one with a garbage amount, so Undo would
    // leave the transaction unbalanced.
    plant(
        &path,
        "INSERT INTO changed_splits
           SELECT c.id, side.name, s.*
           FROM (SELECT id FROM changes ORDER BY sequence DESC LIMIT 1) AS c,
                (SELECT * FROM splits WHERE account_id IS NOT NULL LIMIT 1) AS s,
                (SELECT 'before' AS name UNION SELECT 'after') AS side;
         UPDATE changed_splits SET amount = amount + 999999 WHERE side = 'before';",
    );
    let refused = undo_refused(&path, &laptop);
    let book = Book::open(&path, &laptop).expect("book opened");
    let checking = book.balance(filled.checking).expect("balance");
    let groceries = book.balance(filled.groceries).expect("balance");
    assert!(
        refused,
        "Undo wrote back a transaction that doesn't balance: checking {checking:?}, groceries {groceries:?}"
    );
    assert_eq!(checking, Money::from_minor_units(-7319, eur()));
}

#[test]
fn b5_undo_brings_back_what_a_planted_trigger_deleted() {
    // While the schema check is missing, at least the history must see what
    // a planted trigger did inside a user action, so Undo is a way back.
    let folder = TempFolder::new();
    let path = folder.join("Hostile.ksz");
    let laptop = folder.computer();
    let filled = fill(&path, &laptop);
    let parts = split_count(&path);
    plant(
        &path,
        "CREATE TRIGGER t AFTER UPDATE ON transactions BEGIN DELETE FROM splits; END;",
    );
    let Ok(()) = open_and_edit(&path, &laptop, &filled) else {
        // Refused at open or at the edit: nothing was deleted.
        assert_eq!(split_count(&path), parts);
        return;
    };
    plant(&path, "DROP TRIGGER t;");
    let mut book = Book::open(&path, &laptop).expect("book opened");
    book.undo().expect("undone");
    drop(book);
    assert_eq!(
        split_count(&path),
        parts,
        "Undo didn't bring every part back"
    );
}
