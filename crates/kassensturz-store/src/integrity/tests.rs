// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the checks made when a book is opened (B-291). All data is made
//! up (AGENTS.md §3.1 rule 2).

use std::path::Path;

use kassensturz_core::AccountKind;

use crate::test_folder::TempFolder;
use crate::tests::{EUR, account, date, eur, transaction};
use crate::{Book, Computer, Corruption, StoreError};

fn computer(folder: &TempFolder) -> Computer {
    Computer {
        name: "Laptop".to_owned(),
        backup_folder: folder.join("Backups"),
    }
}

/// A book in a file with a few transactions, closed again.
fn book_file(folder: &TempFolder) -> std::path::PathBuf {
    let path = folder.join("Household.ksz");
    let mut book = Book::create(&path, &computer(folder)).expect("book made");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let food = account(&mut book, "Groceries", AccountKind::Expense, EUR);
    for day in 1..=20 {
        let parts = [(checking, eur(-1250)), (food, eur(1250))];
        book.add_transaction(&transaction(date(2026, 3, day), "Corner shop", &parts))
            .expect("added");
    }
    path
}

/// Another program changes a number in the file, as a raw SQL statement
/// does: past every check of KassenSturz.
fn change_an_amount(path: &Path) {
    let conn = rusqlite::Connection::open(path).expect("file opened");
    let changed = conn
        .execute(
            "UPDATE splits SET amount = amount + 1 WHERE rowid = (SELECT min(rowid) FROM splits)",
            [],
        )
        .expect("changed");
    assert_eq!(changed, 1);
}

#[test]
fn a_book_that_is_all_right_opens() {
    let folder = TempFolder::new();
    let path = book_file(&folder);
    Book::open(&path, &computer(&folder)).expect("opens");
    Book::open_to_read(&path).expect("reads");
}

/// SQLite's `integrity_check` is content with the book after one amount
/// changed; the balance check is what stops it.
#[test]
fn a_changed_amount_is_found_and_the_book_stays_as_it_was() {
    let folder = TempFolder::new();
    let path = book_file(&folder);
    change_an_amount(&path);
    let conn = rusqlite::Connection::open(&path).expect("file opened");
    let check: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("checked");
    assert_eq!(check, "ok", "the premise: SQLite sees nothing wrong");
    drop(conn);
    let before = std::fs::read(&path).expect("read");

    let refused = Book::open(&path, &computer(&folder));
    assert!(
        matches!(
            refused,
            Err(StoreError::Corrupt(Corruption::UnbalancedTransaction))
        ),
        "{:?}",
        refused.err()
    );
    let refused = Book::open_to_read(&path);
    assert!(matches!(
        refused,
        Err(StoreError::Corrupt(Corruption::UnbalancedTransaction))
    ));
    assert_eq!(std::fs::read(&path).expect("read"), before, "changed");
    assert!(
        !crate::lock::lock_path(&path).exists(),
        "the lock stayed after a refusal"
    );
}

/// A page SQLite can't parse: the book is refused with a clear code, not
/// opened to fail at the next save.
#[test]
fn a_damaged_page_is_found() {
    let folder = TempFolder::new();
    let path = book_file(&folder);
    let mut bytes = std::fs::read(&path).expect("read");
    // Overwrite the start of a table page in the middle of the file.
    let page = 4096;
    assert!(bytes.len() > 6 * page, "{}", bytes.len());
    for byte in &mut bytes[3 * page..3 * page + 64] {
        *byte = 0xFF;
    }
    std::fs::write(&path, &bytes).expect("written");

    let refused = Book::open(&path, &computer(&folder));
    let code = refused.as_ref().err().map(|error| {
        use kassensturz_core::Report;
        error.report().code
    });
    assert!(
        matches!(refused, Err(StoreError::Corrupt(Corruption::DamagedFile))),
        "{code:?}"
    );
    assert!(!crate::lock::lock_path(&path).exists());
}

#[test]
fn a_cut_short_file_is_a_damaged_book() {
    let folder = TempFolder::new();
    let path = book_file(&folder);
    let bytes = std::fs::read(&path).expect("read");
    for keep in [bytes.len() / 2, bytes.len() - 100, 4096 * 2, 200] {
        std::fs::write(&path, &bytes[..keep]).expect("written");
        let refused = Book::open(&path, &computer(&folder));
        let shown = refused.as_ref().err().map(|error| {
            use kassensturz_core::Report;
            error.report().code
        });
        assert!(
            matches!(refused, Err(StoreError::Corrupt(Corruption::DamagedFile))),
            "kept {keep}: {shown:?}"
        );
        assert!(!crate::lock::lock_path(&path).exists());
        // The reader beside the app (MCP server, CLI) says the same.
        assert!(
            matches!(
                Book::open_to_read(&path),
                Err(StoreError::Corrupt(Corruption::DamagedFile))
            ),
            "kept {keep}"
        );
    }
}

/// A book in a file with one euro transaction that moved 1000.00 EUR into a
/// dollar account, which received 1085.60 USD at the stored rate `rate`
/// (from `from_euro`: euros into dollars, or the other way round).
fn converted_book(folder: &TempFolder, rate: &str, from_euro: bool) -> std::path::PathBuf {
    use crate::tests::USD;
    use kassensturz_core::{
        DatedRate, DatedRateId, ForeignAmount, Money, RateSource, Split, Transaction, TransactionId,
    };

    let path = folder.join("Trips.ksz");
    let mut book = Book::create(&path, &computer(folder)).expect("book made");
    book.use_currency(EUR).expect("currency used");
    book.use_currency(USD).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD);
    let (from, to) = if from_euro { (EUR, USD) } else { (USD, EUR) };
    let rate = DatedRate {
        id: DatedRateId::generate(),
        date: date(2026, 10, 7),
        from,
        to,
        rate: rate.parse().expect("valid rate"),
        source: RateSource::Manual,
    };
    let received = Split {
        foreign: Some(ForeignAmount {
            amount: Money::from_minor_units(108_560, USD),
            rate: Some(rate),
        }),
        ..Split::new(dollars, eur(100_000))
    };
    let transaction = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 7),
        Some("To the dollar account".into()),
        vec![Split::new(checking, eur(-100_000)), received],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    path
}

fn change_foreign_amount(path: &Path, to: i64) {
    let conn = rusqlite::Connection::open(path).expect("file opened");
    let changed = conn
        .execute(
            "UPDATE splits SET foreign_amount = ?1 WHERE foreign_amount IS NOT NULL",
            [to],
        )
        .expect("changed");
    assert_eq!(changed, 1);
}

#[test]
fn a_converted_part_that_agrees_with_its_rate_opens_either_way_round() {
    for (rate, from_euro) in [("1.0856", true), ("0.92116", false)] {
        let folder = TempFolder::new();
        let path = converted_book(&folder, rate, from_euro);
        Book::open(&path, &computer(&folder)).unwrap_or_else(|error| {
            panic!("{rate} refused: {error:?}");
        });
    }
}

#[test]
fn a_changed_foreign_amount_is_found() {
    for (rate, from_euro) in [("1.0856", true), ("0.92116", false)] {
        let folder = TempFolder::new();
        let path = converted_book(&folder, rate, from_euro);
        change_foreign_amount(&path, 108_560 + 4_000);
        let refused = Book::open(&path, &computer(&folder));
        assert!(
            matches!(
                refused,
                Err(StoreError::Corrupt(Corruption::UnbalancedTransaction))
            ),
            "{rate}: {:?}",
            refused.err()
        );
        assert!(matches!(
            Book::open_to_read(&path),
            Err(StoreError::Corrupt(Corruption::UnbalancedTransaction))
        ));
    }
}

/// Typed-in amounts have no stored rate: nothing to compare with.
#[test]
fn a_part_without_a_stored_rate_is_not_judged() {
    let folder = TempFolder::new();
    let path = converted_book(&folder, "1.0856", true);
    let conn = rusqlite::Connection::open(&path).expect("file opened");
    conn.execute("DELETE FROM split_rates", [])
        .expect("deleted");
    drop(conn);
    change_foreign_amount(&path, 99_999);
    Book::open(&path, &computer(&folder)).expect("opens");
}

/// A part whose account is no longer there counts in no balance: refused.
#[test]
fn a_part_whose_account_is_gone_is_found() {
    let folder = TempFolder::new();
    let path = book_file(&folder);
    let conn = rusqlite::Connection::open(&path).expect("file opened");
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("off");
    let changed = conn
        .execute(
            "UPDATE splits SET account_id = zeroblob(16) WHERE rowid = (SELECT min(rowid) FROM splits)",
            [],
        )
        .expect("changed");
    assert_eq!(changed, 1);
    drop(conn);
    for refused in [
        Book::open(&path, &computer(&folder)).err(),
        Book::open_to_read(&path).err(),
    ] {
        assert!(
            matches!(
                refused,
                Some(StoreError::Corrupt(Corruption::BrokenReferences))
            ),
            "{refused:?}"
        );
    }
}

/// A converted part as `Book::convert` would store it: `amount` (euro
/// minor units) converted at `rate` in `rounding`, either multiplied
/// (`forward`, a rate from euros into dollars) or divided by a rate from
/// dollars into euros.
fn conversion(
    amount: i64,
    rate: kassensturz_core::ExchangeRate,
    forward: bool,
    rounding: kassensturz_core::RoundingMode,
) -> super::Conversion {
    use crate::tests::USD;
    use kassensturz_core::{Currency, Money};
    let euro = Currency::new(EUR, 2).expect("currency");
    let dollar = Currency::new(USD, 2).expect("currency");
    let money = Money::from_minor_units(amount, EUR);
    let converted = if forward {
        money.checked_mul_rate(euro, rate, dollar, rounding)
    } else {
        money.checked_div_rate(euro, rate, dollar, rounding)
    }
    .expect("converted");
    super::Conversion {
        amount,
        currency: ("EUR".to_owned(), 2),
        foreign: converted.minor_units(),
        foreign_currency: ("USD".to_owned(), 2),
        from: if forward { "EUR" } else { "USD" }.to_owned(),
        digits: rate.digits(),
        places: i64::from(rate.decimal_places()),
    }
}

/// B-349: a converted part with a negative amount was refused whenever the
/// amount rounded one way, because the range of what the rate allows was
/// worked out as if every amount were positive. Every amount, of either
/// sign, converted in either direction and in every rounding mode, must
/// agree with its own rate; a foreign amount far off must still be found.
#[test]
fn every_correct_conversion_agrees_whatever_its_sign() {
    use kassensturz_core::{ExchangeRate, RoundingMode};
    let rates = ["1.0856", "0.92116", "1.1", "157.42", "0.0063", "1.0000"];
    // A simple, fixed sequence of further amounts, so the run is the same
    // every time: large and odd ones besides the small ones swept below.
    let mut next: u64 = 0x2545_f491_4f6c_dd1d;
    let mut more = || {
        next = next
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        i64::try_from(next >> 34).expect("fits")
    };
    let amounts: Vec<i64> = (1..=1_200)
        .chain((1..=100).map(|n| n * 10_000))
        .chain((0..500).map(|_| more()))
        .collect();
    for rate in rates {
        let rate: ExchangeRate = rate.parse().expect("valid rate");
        for forward in [true, false] {
            for rounding in RoundingMode::ALL {
                for &amount in &amounts {
                    for amount in [amount, -amount] {
                        let part = conversion(amount, rate, forward, rounding);
                        assert_eq!(
                            part.disagrees(),
                            Some(false),
                            "{amount} at {rate:?} forward={forward} {rounding:?}"
                        );
                        // A tiny amount fits many others within the rate's
                        // last digit; only a clear difference is asked for.
                        if amount.abs() < 1_000 {
                            continue;
                        }
                        let mut off = conversion(amount, rate, forward, rounding);
                        off.foreign += amount.signum() * (off.foreign.abs() / 10 + 50);
                        assert_eq!(
                            off.disagrees(),
                            Some(true),
                            "{amount} at {rate:?} forward={forward} {rounding:?} not found"
                        );
                    }
                }
            }
        }
    }
}

/// B-349 end to end: transfers converted by `Book::convert`, out of the
/// euro account and into it, in both directions of the rate, are stored
/// and the book opens again afterwards, also read-only.
#[test]
fn a_book_with_converted_transfers_of_either_sign_opens_again() {
    use crate::tests::USD;
    use kassensturz_core::{
        DatedRate, DatedRateId, ForeignAmount, RateSource, Split, Transaction, TransactionId,
    };
    for (rate, from_euro) in [("1.0856", true), ("0.92116", false)] {
        let folder = TempFolder::new();
        let path = folder.join("Trips.ksz");
        let mut book = Book::create(&path, &computer(&folder)).expect("book made");
        book.use_currency(EUR).expect("currency used");
        book.use_currency(USD).expect("currency used");
        let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
        let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD);
        let (from, to) = if from_euro { (EUR, USD) } else { (USD, EUR) };
        book.add_rate(&DatedRate {
            id: DatedRateId::generate(),
            date: date(2026, 10, 1),
            from,
            to,
            rate: rate.parse().expect("valid rate"),
            source: RateSource::Manual,
        })
        .expect("rate added");
        let euros = (1..=300).chain((100..=1_000).map(|n| n * 100));
        for minor in euros {
            for minor in [minor, -minor] {
                let (converted, rate) = book
                    .convert(eur(minor), USD, date(2026, 10, 7))
                    .expect("converted");
                let moved = Split {
                    foreign: Some(ForeignAmount {
                        amount: converted,
                        rate: Some(rate),
                    }),
                    ..Split::new(dollars, eur(minor))
                };
                let transaction = Transaction::new(
                    TransactionId::generate(),
                    date(2026, 10, 7),
                    Some("Dollar account".into()),
                    vec![Split::new(checking, eur(-minor)), moved],
                )
                .expect("balanced");
                book.add_transaction(&transaction).expect("added");
            }
        }
        drop(book);
        Book::open(&path, &computer(&folder)).unwrap_or_else(|error| {
            panic!("{rate} refused: {error:?}");
        });
        Book::open_to_read(&path).unwrap_or_else(|error| {
            panic!("{rate} refused read-only: {error:?}");
        });
    }
}
