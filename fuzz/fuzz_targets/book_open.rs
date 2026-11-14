// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzz target: a book file with a few changed bytes (a failing drive, a sync
//! tool, another program) is refused, or opens and reads its balances,
//! without panicking or hanging (D-787, B-291). The first byte pair of the
//! input says how many changes follow; each change is a position and a
//! value, applied to a valid small book made once per process.

#![no_main]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::{Book, Computer};
use libfuzzer_sys::fuzz_target;

struct Base {
    folder: PathBuf,
    bytes: Vec<u8>,
}

fn base() -> &'static Base {
    static BASE: OnceLock<Base> = OnceLock::new();
    BASE.get_or_init(|| {
        let folder =
            std::env::temp_dir().join(format!("kassensturz-fuzz-{}", AccountId::generate()));
        std::fs::create_dir(&folder).expect("folder made");
        let path = folder.join("base.ksz");
        let computer = computer(&folder);
        let eur = CurrencyCode::new("EUR").expect("valid code");
        let mut book = Book::create(&path, &computer).expect("book made");
        book.use_currency(eur).expect("currency used");
        let mut account = |name: &str, kind| {
            let id = AccountId::generate();
            book.add_account(&Account {
                id,
                name: name.into(),
                kind,
                currency: eur,
                account_type: None,
                closed: false,
                parent: None,
                number: None,
                notes: None,
                institution: None,
            })
            .expect("account added");
            id
        };
        let (checking, food) = (
            account("Checking account", AccountKind::Asset),
            account("Groceries", AccountKind::Expense),
        );
        for (day, cents) in [(3, 1250), (9, 4710), (17, 899)] {
            let parts = vec![
                Split::new(checking, Money::from_minor_units(-cents, eur)),
                Split::new(food, Money::from_minor_units(cents, eur)),
            ];
            let date = Date::new(2026, 3, day).expect("valid date");
            book.add_transaction(
                &Transaction::new(TransactionId::generate(), date, Some("Shop".into()), parts)
                    .expect("balanced"),
            )
            .expect("added");
        }
        drop(book);
        let bytes = std::fs::read(&path).expect("read");
        Base { folder, bytes }
    })
}

fn computer(folder: &Path) -> Computer {
    Computer {
        name: "Fuzz".into(),
        backup_folder: folder.join("Backups"),
    }
}

fuzz_target!(|data: &[u8]| {
    let base = base();
    let mut bytes = base.bytes.clone();
    let size = bytes.len();
    // Five bytes per change: a position (4) and a value (1).
    for change in data.chunks_exact(5).take(16) {
        let at = u32::from_le_bytes([change[0], change[1], change[2], change[3]]) as usize;
        if let Some(byte) = bytes.get_mut(at % size) {
            *byte = change[4];
        }
    }
    // A cut-short file too: an input of one more byte cuts the file.
    if data.len() % 5 == 1 {
        let keep = usize::from(data[data.len() - 1]) * size / 256;
        bytes.truncate(keep);
    }
    let path = base.folder.join("changed.ksz");
    std::fs::write(&path, &bytes).expect("written");
    if let Ok(book) = Book::open(&path, &computer(&base.folder)) {
        if let Ok(accounts) = book.all_accounts() {
            for account in accounts {
                let _ = book.balance(account.id);
            }
        }
        let _ = book.transactions();
    }
    let _ = std::fs::remove_file(&path);
    let _ = Book::open_to_read(&path);
});
