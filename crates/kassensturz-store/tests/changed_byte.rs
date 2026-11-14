// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Regression test for B-291 and its neighbours: a byte of a book file that
//! changes, whether by a failing drive, a sync tool or another program, must
//! never make a book open with another balance. A book that can't be trusted
//! is refused; one whose changed byte is in a text, such as a payee's name,
//! may open, but every account keeps its balance. All data is made up
//! (AGENTS.md §3.1 rule 2).

use std::path::{Path, PathBuf};

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, DatedRate, DatedRateId, ForeignAmount,
    Money, RateSource, Split, Transaction, TransactionId,
};
use kassensturz_store::{Book, Computer};

struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kassensturz-test-{}", AccountId::generate()));
        std::fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    fn computer(&self) -> Computer {
        Computer {
            name: "Laptop".into(),
            backup_folder: self.0.join("Backups"),
        }
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn code(text: &str) -> CurrencyCode {
    CurrencyCode::new(text).expect("valid code")
}

fn account(book: &mut Book, name: &str, kind: AccountKind, currency: CurrencyCode) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency,
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

/// A book with euro and dollar accounts: ordinary transactions, and a
/// conversion that keeps the rate it was made with.
fn make_book(path: &Path, computer: &Computer) -> Vec<AccountId> {
    let (eur, usd) = (code("EUR"), code("USD"));
    let mut book = Book::create(path, computer).expect("book made");
    book.use_currency(eur).expect("currency used");
    book.use_currency(usd).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, eur);
    let food = account(&mut book, "Groceries", AccountKind::Expense, eur);
    let pay = account(&mut book, "Salary", AccountKind::Income, eur);
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, usd);
    let money = |units: i64| Money::from_minor_units(units, eur);
    for (day, amount) in [(3, 1250), (9, 4710), (17, 899)] {
        let parts = vec![
            Split::new(checking, money(-amount)),
            Split::new(food, money(amount)),
        ];
        let date = Date::new(2026, 3, day).expect("valid date");
        book.add_transaction(
            &Transaction::new(
                TransactionId::generate(),
                date,
                Some("Corner shop".into()),
                parts,
            )
            .expect("balanced"),
        )
        .expect("added");
    }
    let salary = vec![
        Split::new(checking, money(250_000)),
        Split::new(pay, money(-250_000)),
    ];
    book.add_transaction(
        &Transaction::new(
            TransactionId::generate(),
            Date::new(2026, 3, 28).expect("valid date"),
            Some("Employer".into()),
            salary,
        )
        .expect("balanced"),
    )
    .expect("added");
    let rate = DatedRate {
        id: DatedRateId::generate(),
        date: Date::new(2026, 3, 20).expect("valid date"),
        from: eur,
        to: usd,
        rate: "1.0856".parse().expect("valid rate"),
        source: RateSource::Manual,
    };
    let received = Split {
        foreign: Some(ForeignAmount {
            amount: Money::from_minor_units(108_560, usd),
            rate: Some(rate),
        }),
        ..Split::new(dollars, money(100_000))
    };
    book.add_transaction(
        &Transaction::new(
            TransactionId::generate(),
            Date::new(2026, 3, 20).expect("valid date"),
            Some("To the dollar account".into()),
            vec![Split::new(checking, money(-100_000)), received],
        )
        .expect("balanced"),
    )
    .expect("added");
    vec![checking, food, pay, dollars]
}

/// What a person sees of the money: the balance of every account.
fn balances(book: &Book, accounts: &[AccountId]) -> Vec<String> {
    accounts
        .iter()
        .map(|&id| format!("{:?}", book.balance(id).expect("balance")))
        .collect()
}

#[test]
fn one_changed_byte_never_changes_a_balance_silently() {
    let folder = TempFolder::new();
    let path = folder.0.join("Household.ksz");
    let computer = folder.computer();
    let accounts = make_book(&path, &computer);
    let original = std::fs::read(&path).expect("read");
    let expected = {
        let book = Book::open(&path, &computer).expect("opens");
        balances(&book, &accounts)
    };

    let mut silent = Vec::new();
    let (mut refused, mut opened) = (0, 0);
    // The file is mostly empty pages, so only bytes that hold something are
    // changed, spread evenly over the file, about 500 of them: opening a book
    // is not cheap, and CI has other work.
    let used: Vec<usize> = (0..original.len())
        .filter(|&at| original[at] != 0)
        .collect();
    let step = (used.len() / 500).max(1);
    let offsets = used.into_iter().step_by(step);
    for at in offsets {
        let mut bytes = original.clone();
        bytes[at] ^= 0x01;
        std::fs::write(&path, &bytes).expect("written");
        match Book::open(&path, &computer) {
            Err(_) => refused += 1,
            Ok(book) => {
                opened += 1;
                match balances_of_opened(&book, &accounts) {
                    // A balance that can't be read says so; it is no silent change.
                    None => {}
                    Some(found) if found == expected => {}
                    other => silent.push((at, other)),
                }
            }
        }
    }
    assert!(
        silent.is_empty(),
        "{} byte changes opened with other balances, e.g. at {:?} ({refused} refused, {opened} opened)",
        silent.len(),
        &silent[..silent.len().min(5)]
    );
    assert!(
        refused > 0,
        "no change was ever refused: the test saw nothing"
    );
}

/// The balances of an opened book; `None` if reading them fails, which is no
/// silent change either, but is noted.
fn balances_of_opened(book: &Book, accounts: &[AccountId]) -> Option<Vec<String>> {
    let mut found = Vec::new();
    for &id in accounts {
        found.push(format!("{:?}", book.balance(id).ok()?));
    }
    Some(found)
}
