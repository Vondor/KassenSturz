// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Made-up sample data for the walking skeleton (D-068). It is never real
//! data (AGENTS.md §3.1 rule 2) and never saved: the book lives in memory.

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, SplitId, Transaction,
    TransactionId,
};
use kassensturz_store::{Book, StoreError};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

/// A sample book and the account the window shows.
pub struct SampleBook {
    /// The book, in memory.
    pub book: Book,
    /// Its checking account.
    pub checking: AccountId,
}

/// Creates the sample book: a checking account with a month's salary, rent
/// and two trips to the grocery store.
pub fn create() -> Result<SampleBook, StoreError> {
    let mut book = Book::create_in_memory()?;
    book.use_currency(EUR)?;
    let checking = add_account(&mut book, "Checking account", AccountKind::Asset)?;
    let salary = add_account(&mut book, "Salary", AccountKind::Income)?;
    let rent = add_account(&mut book, "Rent", AccountKind::Expense)?;
    let groceries = add_account(&mut book, "Groceries", AccountKind::Expense)?;

    // Day in October 2026, who, which category, cents in or out.
    let transactions = [
        (1, "Example Employer Ltd.", salary, 240_000),
        (2, "Example Housing Co.", rent, -95_000),
        (5, "Corner Grocery", groceries, -5_320),
        (12, "Corner Grocery", groceries, -2_745),
    ];
    for (day, payee, category, minor_units) in transactions {
        let amount = Money::from_minor_units(minor_units, EUR);
        let other_side = amount
            .checked_neg()
            .expect("small sample amounts can be negated");
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, day),
            Some(payee.into()),
            vec![
                Split {
                    id: SplitId::generate(),
                    account: checking,
                    amount,
                },
                Split {
                    id: SplitId::generate(),
                    account: category,
                    amount: other_side,
                },
            ],
        )
        .expect("both parts cancel out, so the transaction balances");
        book.add_transaction(&transaction)?;
    }
    Ok(SampleBook { book, checking })
}

fn add_account(book: &mut Book, name: &str, kind: AccountKind) -> Result<AccountId, StoreError> {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
    })?;
    Ok(id)
}
