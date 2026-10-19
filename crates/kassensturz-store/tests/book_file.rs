// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests on book files. All data is made up (AGENTS.md §3.1 rule 2).

use std::path::{Path, PathBuf};

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, DatedRate, DatedRateId, Money, RateSource,
    RoundingMode, Split, SplitId, Timestamp, Transaction, TransactionId,
};
use kassensturz_store::{Book, Computer, StoreError};

/// A folder of its own for one test, removed again afterwards.
struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kassensturz-test-{}", AccountId::generate()));
        std::fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// A computer called `name` that keeps its backups in this folder.
    fn computer(&self, name: &str) -> Computer {
        Computer {
            name: name.into(),
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

fn side_file(book: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", book.display()))
}

/// The files that sit next to `book` while it is open or after it was closed.
fn side_files(book: &Path) -> Vec<PathBuf> {
    ["-journal", "-wal", "-shm", ".lock"]
        .iter()
        .map(|suffix| side_file(book, suffix))
        .filter(|path| path.exists())
        .collect()
}

/// The error opening `path` on `computer` gives. Books can't be printed, so
/// `expect_err` doesn't work.
fn open_error(path: &Path, computer: &Computer) -> StoreError {
    match Book::open(path, computer) {
        Ok(_) => panic!("{} was opened", path.display()),
        Err(error) => error,
    }
}

#[test]
fn a_book_keeps_its_data_after_closing() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let laptop = folder.computer("Laptop");
    let checking = {
        let mut book = Book::create(&path, &laptop).expect("book created");
        fill(&mut book)
    };
    let book = Book::open(&path, &laptop).expect("book opened again");
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
    let laptop = folder.computer("Laptop");
    {
        let mut book = Book::create(&path, &laptop).expect("book created");
        book.set_rounding_mode(RoundingMode::HalfEven)
            .expect("rounding mode set");
    }
    let book = Book::open(&path, &laptop).expect("book opened again");
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
    let laptop = folder.computer("Laptop");
    {
        let mut book = Book::create(&path, &laptop).expect("book created");
        book.use_currency(eur()).expect("currency used");
        book.use_currency(usd).expect("currency used");
        book.add_rate(&rate).expect("rate added");
    }
    let book = Book::open(&path, &laptop).expect("book opened again");
    let stored = book.rate(rate.id).expect("rate found");
    assert_eq!(stored, rate);
    assert_eq!(stored.rate.to_string(), "1.0850");
}

#[test]
fn only_the_lock_file_stays_next_to_an_open_book() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let laptop = folder.computer("Laptop");
    let mut book = Book::create(&path, &laptop).expect("book created");
    fill(&mut book);
    // The rollback journal only exists during a write (D-033); WAL would
    // leave two files next to the book for as long as it is open. The lock
    // file stays while the book is open (D-038).
    assert_eq!(side_files(&path), vec![side_file(&path, ".lock")]);
    drop(book);
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
    let book = Book::open(&path, &laptop).expect("book opened again");
    assert_eq!(side_files(&path), vec![side_file(&path, ".lock")]);
    drop(book);
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
    // A book with the current layout needs no backup.
    assert!(!laptop.backup_folder.exists());
}

#[test]
fn a_book_file_says_it_is_a_kassensturz_book() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    drop(Book::create(&path, &folder.computer("Laptop")).expect("book created"));
    let bytes = std::fs::read(&path).expect("book read");
    // SQLite's header, with KassenSturz's application ID at bytes 68 to 71
    // (D-037).
    assert_eq!(&bytes[..16], b"SQLite format 3\0");
    assert_eq!(&bytes[68..72], b"KSZB");
}

#[test]
fn creating_a_book_never_touches_an_existing_file() {
    let folder = TempFolder::new();
    let path = folder.join("Notes.ksz");
    std::fs::write(&path, "not a book").expect("file written");
    assert!(matches!(
        Book::create(&path, &folder.computer("Laptop")),
        Err(StoreError::AlreadyExists)
    ));
    assert_eq!(
        std::fs::read_to_string(&path).expect("file read"),
        "not a book"
    );
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
}

#[test]
fn opening_a_missing_book_says_so() {
    let folder = TempFolder::new();
    let path = folder.join("Missing.ksz");
    assert!(matches!(
        open_error(&path, &folder.computer("Laptop")),
        StoreError::NotFound
    ));
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
}

#[test]
fn files_that_are_not_books_are_turned_away_untouched() {
    let folder = TempFolder::new();
    let laptop = folder.computer("Laptop");

    let text = folder.join("Notes.ksz");
    std::fs::write(&text, "These are not the books you're looking for.").expect("file written");
    let empty = folder.join("Empty.ksz");
    std::fs::write(&empty, "").expect("file written");
    // Another program's SQLite file, with the same layout version as a book
    // and in WAL mode, which opening it as a book would switch off.
    let other_database = folder.join("Other.sqlite");
    rusqlite::Connection::open(&other_database)
        .and_then(|conn| {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE notes (text TEXT);
                 PRAGMA user_version = 3;",
            )
        })
        .expect("other database written");

    for path in [&text, &empty, &other_database] {
        let before = std::fs::read(path).expect("file read");
        assert!(
            matches!(open_error(path, &laptop), StoreError::NotABook),
            "{}",
            path.display()
        );
        assert_eq!(std::fs::read(path).expect("file read"), before);
        assert_eq!(side_files(path), Vec::<PathBuf>::new());
    }
    assert!(matches!(
        open_error(folder.path(), &laptop),
        StoreError::NotABook
    ));
}

#[test]
fn books_with_another_layout_version_are_turned_away() {
    let folder = TempFolder::new();
    let path = folder.join("Future.ksz");
    let laptop = folder.computer("Laptop");
    drop(Book::create(&path, &laptop).expect("book created"));
    rusqlite::Connection::open(&path)
        .and_then(|conn| conn.pragma_update(None, "user_version", 99))
        .expect("layout version changed");
    assert!(matches!(
        open_error(&path, &laptop),
        StoreError::UnsupportedLayout { found: 99 }
    ));
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
    assert!(!laptop.backup_folder.exists());
}

#[test]
fn a_book_is_open_in_one_place_at_a_time() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let before = Timestamp::now();
    let book = Book::create(&path, &folder.computer("Laptop")).expect("book created");
    let desk = folder.computer("Desk PC");
    let StoreError::Locked(lock) = open_error(&path, &desk) else {
        panic!("not locked");
    };
    assert_eq!(lock.computer.as_deref(), Some("Laptop"));
    // In whole seconds.
    let since = lock.since.expect("time written");
    assert!(before.as_second() <= since.as_second() && since <= Timestamp::now());
    drop(book);
    drop(Book::open(&path, &desk).expect("opened once it was closed"));
}

#[test]
fn a_book_can_be_opened_anyway_after_a_warning() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    // Still open on the laptop, or KassenSturz crashed there.
    let on_laptop = Book::create(&path, &folder.computer("Laptop")).expect("book created");
    let desk = folder.computer("Desk PC");
    let StoreError::Locked(seen) = open_error(&path, &desk) else {
        panic!("not locked");
    };
    let on_desk = Book::open_anyway(&path, &desk, &seen).expect("opened anyway");
    // Closing the book on the laptop leaves the desk's lock alone.
    drop(on_laptop);
    let kitchen = folder.computer("Kitchen");
    let StoreError::Locked(lock) = open_error(&path, &kitchen) else {
        panic!("not locked");
    };
    assert_eq!(lock.computer.as_deref(), Some("Desk PC"));
    drop(on_desk);
    drop(Book::open(&path, &kitchen).expect("opened once it was closed"));
}

#[test]
fn opening_anyway_takes_over_only_the_lock_the_user_was_warned_about() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let laptop = folder.computer("Laptop");
    let on_laptop = Book::create(&path, &laptop).expect("book created");
    let desk = folder.computer("Desk PC");
    let StoreError::Locked(seen) = open_error(&path, &desk) else {
        panic!("not locked");
    };
    // Meanwhile the book is closed on the laptop and opened there again,
    // possibly within the same second.
    drop(on_laptop);
    let on_laptop = Book::open(&path, &laptop).expect("opened again");
    let Err(StoreError::Locked(lock)) = Book::open_anyway(&path, &desk, &seen) else {
        panic!("not locked");
    };
    assert_ne!(lock, seen);
    assert_eq!(lock.computer.as_deref(), Some("Laptop"));
    // Warned about the new lock, the user can still open the book anyway.
    let on_desk = Book::open_anyway(&path, &desk, &lock).expect("opened anyway");
    drop(on_laptop);
    drop(on_desk);
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
}

#[test]
fn a_damaged_lock_file_still_locks_the_book() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let laptop = folder.computer("Laptop");
    drop(Book::create(&path, &laptop).expect("book created"));
    std::fs::write(side_file(&path, ".lock"), b"\xff\xfe\0 garbage").expect("lock file damaged");
    let StoreError::Locked(lock) = open_error(&path, &laptop) else {
        panic!("not locked");
    };
    assert_eq!((lock.computer.as_deref(), lock.since), (None, None));
    drop(Book::open_anyway(&path, &laptop, &lock).expect("opened anyway"));
    assert_eq!(side_files(&path), Vec::<PathBuf>::new());
}

#[test]
fn no_book_is_created_where_a_lock_file_is() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    std::fs::write(side_file(&path, ".lock"), "computer=Laptop\n").expect("lock file written");
    let Err(StoreError::Locked(lock)) = Book::create(&path, &folder.computer("Desk PC")) else {
        panic!("not locked");
    };
    assert_eq!(lock.computer.as_deref(), Some("Laptop"));
    assert!(!path.exists());
}

#[test]
fn a_computer_name_stays_one_line_of_the_lock_file() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let odd_name = folder.computer("Laptop\nsince=2000-01-01T00:00:00Z\n");
    let _book = Book::create(&path, &odd_name).expect("book created");
    let StoreError::Locked(lock) = open_error(&path, &folder.computer("Desk PC")) else {
        panic!("not locked");
    };
    assert_eq!(
        lock.computer.as_deref(),
        Some("Laptop since=2000-01-01T00:00:00Z")
    );
    let text = std::fs::read_to_string(side_file(&path, ".lock")).expect("lock file read");
    assert_eq!(text.lines().count(), 3, "{text:?}");
}
