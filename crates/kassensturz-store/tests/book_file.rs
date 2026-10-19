// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests on book files. All data is made up (AGENTS.md §3.1 rule 2).

use std::path::{Path, PathBuf};

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, DatedRate, DatedRateId, Money, RateSource,
    RoundingMode, Split, SplitId, Transaction, TransactionId,
};
use kassensturz_store::{Book, StoreError};

/// A folder of its own for one test, removed again afterwards.
struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kassensturz-test-{}", AccountId::generate()));
        std::fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
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

/// Fills a new book with one account and one transaction; returns the account.
fn fill(book: &mut Book) -> AccountId {
    book.use_currency(eur()).expect("currency used");
    let checking = Account {
        id: AccountId::generate(),
        name: "Checking account".into(),
        kind: AccountKind::Asset,
        currency: eur(),
    };
    let groceries = Account {
        id: AccountId::generate(),
        name: "Groceries".into(),
        kind: AccountKind::Expense,
        currency: eur(),
    };
    book.add_account(&checking).expect("account added");
    book.add_account(&groceries).expect("category added");
    let shopping = Transaction::new(
        TransactionId::generate(),
        Date::new(2026, 10, 5).expect("valid date"),
        Some("Corner Grocery".into()),
        vec![
            Split {
                id: SplitId::generate(),
                account: checking.id,
                amount: Money::from_minor_units(-5320, eur()),
            },
            Split {
                id: SplitId::generate(),
                account: groceries.id,
                amount: Money::from_minor_units(5320, eur()),
            },
        ],
    )
    .expect("balanced");
    book.add_transaction(&shopping).expect("transaction added");
    checking.id
}

fn side_files(book: &Path) -> Vec<PathBuf> {
    ["-journal", "-wal", "-shm"]
        .iter()
        .map(|suffix| PathBuf::from(format!("{}{suffix}", book.display())))
        .filter(|path| path.exists())
        .collect()
}

#[test]
fn a_book_keeps_its_data_after_closing() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let checking = {
        let mut book = Book::create(&path).expect("book created");
        fill(&mut book)
    };
    let book = Book::open(&path).expect("book opened again");
    let account = book.account(checking).expect("account kept");
    assert_eq!(account.name, "Checking account");
    let register = book.register(checking).expect("register");
    assert_eq!(register.len(), 1);
    assert_eq!(register[0].payee.as_deref(), Some("Corner Grocery"));
    assert_eq!(
        book.balance(checking).expect("balance"),
        Money::from_minor_units(-5320, eur())
    );
}

#[test]
fn a_book_keeps_its_rounding_mode() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    {
        let mut book = Book::create(&path).expect("book created");
        book.set_rounding_mode(RoundingMode::HalfEven)
            .expect("rounding mode set");
    }
    let book = Book::open(&path).expect("book opened again");
    assert_eq!(
        book.rounding_mode().expect("rounding mode"),
        RoundingMode::HalfEven
    );
}

#[test]
fn a_book_keeps_its_rates() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let usd = CurrencyCode::new("USD").expect("valid code");
    let rate = DatedRate {
        id: DatedRateId::generate(),
        date: Date::new(2026, 10, 5).expect("valid date"),
        from: eur(),
        to: usd,
        rate: "1.0850".parse().expect("valid rate"),
        source: RateSource::Manual,
    };
    {
        let mut book = Book::create(&path).expect("book created");
        book.use_currency(eur()).expect("currency used");
        book.use_currency(usd).expect("currency used");
        book.add_rate(&rate).expect("rate added");
    }
    let book = Book::open(&path).expect("book opened again");
    let stored = book.rate(rate.id).expect("rate found");
    assert_eq!(stored, rate);
    assert_eq!(stored.rate.to_string(), "1.0850");
}

#[test]
fn books_leave_no_side_files_behind() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let mut book = Book::create(&path).expect("book created");
    fill(&mut book);
    // The rollback journal only exists during a write (D-033); WAL would
    // leave two files next to the book for as long as it is open.
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
    drop(book);
    let book = Book::open(&path).expect("book opened again");
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
    drop(book);
}

#[test]
fn creating_a_book_never_touches_an_existing_file() {
    let folder = TempFolder::new();
    let path = folder.join("Notes.ksz");
    std::fs::write(&path, "not a book").expect("file written");
    assert!(matches!(
        Book::create(&path),
        Err(StoreError::AlreadyExists)
    ));
    assert_eq!(
        std::fs::read_to_string(&path).expect("file read"),
        "not a book"
    );
}

#[test]
fn opening_a_missing_book_says_so() {
    let folder = TempFolder::new();
    assert!(matches!(
        Book::open(&folder.join("Missing.ksz")),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn files_that_are_not_books_are_turned_away() {
    let folder = TempFolder::new();

    let text = folder.join("Notes.ksz");
    std::fs::write(&text, "These are not the books you're looking for.").expect("file written");
    assert!(matches!(Book::open(&text), Err(StoreError::NotABook)));

    let other_database = folder.join("Other.sqlite");
    rusqlite::Connection::open(&other_database)
        .and_then(|conn| conn.execute_batch("CREATE TABLE notes (text TEXT);"))
        .expect("other database written");
    assert!(matches!(
        Book::open(&other_database),
        Err(StoreError::NotABook)
    ));
}

#[test]
fn books_with_another_layout_version_are_turned_away() {
    let folder = TempFolder::new();
    let path = folder.join("Future.ksz");
    drop(Book::create(&path).expect("book created"));
    rusqlite::Connection::open(&path)
        .and_then(|conn| conn.pragma_update(None, "user_version", 99))
        .expect("layout version changed");
    assert!(matches!(
        Book::open(&path),
        Err(StoreError::UnsupportedLayout { found: 99 })
    ));
}
