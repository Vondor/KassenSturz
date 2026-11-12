// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The transactions tile (US4.7, D-716), ready for display: the list comes
//! from `kassensturz-report` (`listed_transactions`), and every amount and
//! date is written here as text in the user's regional format (D-065). No
//! Qt in this file; `transactions_tile_model.rs` hands it to QML.

use std::collections::HashMap;

use kassensturz_core::{Account, AccountId};
use kassensturz_report::{LISTED_ROWS, Listed, OtherSide, Source, listed_transactions};
use kassensturz_store::Book;

use crate::charts::{ChartsError, period};
use crate::format::Locale;
use crate::register::{AmountWriter, Shown};
use crate::scope::{FilterView, Scope};

/// One row as the tile shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileRow {
    /// The transaction's ID as text.
    pub id: String,
    /// The ID of the account it is listed from, to open it from there.
    pub account_id: String,
    /// That account's name.
    pub account: String,
    /// The date that account shows it with.
    pub date: String,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// What it is, as the register's key the UI translates (`EntryKind`):
    /// `simple` (one category, or none yet), `transfer` or `parts`.
    pub kind: &'static str,
    /// The category's name, or for a Transfer the other account's; empty
    /// for Parts and for money not sorted yet.
    pub other: String,
    /// The amount as seen from the account, in its currency.
    pub amount: Shown,
    /// Whether money came in.
    pub incoming: bool,
}

/// What the tile shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TileView {
    /// The newest rows.
    pub rows: Vec<TileRow>,
    /// How many transactions fit, those not shown included.
    pub total: usize,
    /// Whether the rows can come from more than one account, so each names
    /// its account.
    pub several_accounts: bool,
}

/// How many rows a `params.rows` word asks for; the default for any other.
pub fn rows_of(word: &str) -> usize {
    let word = if LISTED_ROWS.contains(&word) {
        word
    } else {
        LISTED_ROWS[0]
    };
    word.parse().unwrap_or(20)
}

/// The tile for `period_key` (a stored period word, as for every tile),
/// `rows` (a `params.rows` word) and the dashboard's filters `scope`.
pub fn tile(
    book: &Book,
    locale: &impl Locale,
    period_key: &str,
    rows: &str,
    scope: &Scope,
) -> Result<(TileView, FilterView), ChartsError> {
    let period = period(book, period_key, locale.today())?;
    let source = Source::read(book, period)?;
    let (filter, view) = scope.resolve(book, &source)?;
    if view.nothing {
        return Ok((TileView::default(), view));
    }
    let listed = listed_transactions(&source, period, &filter, rows_of(rows))?;
    let accounts: HashMap<AccountId, Account> = book
        .all_accounts()
        .map_err(kassensturz_report::ReportError::from)?
        .into_iter()
        .map(|account| (account.id, account))
        .collect();
    let several_accounts = match &filter.accounts {
        Some(chosen) => chosen.len() > 1,
        None => {
            accounts
                .values()
                .filter(|account| account.kind.is_account() && !account.closed)
                .count()
                > 1
        }
    };
    let rows = listed
        .rows
        .iter()
        .map(|row| tile_row(book, locale, &accounts, row))
        .collect::<Result<_, _>>()?;
    Ok((
        TileView {
            rows,
            total: listed.total,
            several_accounts,
        },
        view,
    ))
}

fn tile_row(
    book: &Book,
    locale: &impl Locale,
    accounts: &HashMap<AccountId, Account>,
    row: &Listed,
) -> Result<TileRow, ChartsError> {
    let name = |id: &AccountId| {
        accounts
            .get(id)
            .map(|account| account.name.clone())
            .unwrap_or_default()
    };
    let (kind, other) = match &row.other {
        OtherSide::Category(id) => ("simple", name(id)),
        OtherSide::Transfer(id) => ("transfer", name(id)),
        OtherSide::Parts => ("parts", String::new()),
        OtherSide::NotSorted => ("simple", String::new()),
    };
    let writer = AmountWriter::new(book, row.amount.currency(), locale)
        .map_err(kassensturz_report::ReportError::from)?;
    Ok(TileRow {
        id: row.transaction.to_string(),
        account_id: row.account.to_string(),
        account: name(&row.account),
        date: locale.short_date(row.date),
        other_party: row.other_party.clone(),
        kind,
        other,
        amount: writer.text(row.amount),
        incoming: row.amount.minor_units() > 0,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use kassensturz_core::{AccountKind, CurrencyCode, Money, Split, Transaction, TransactionId};

    use super::*;
    use crate::register::tests::TestLocale;

    pub(crate) const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    pub(crate) struct Fixture {
        pub(crate) book: Book,
        pub(crate) checking: AccountId,
        pub(crate) savings: AccountId,
        pub(crate) groceries: AccountId,
    }

    fn own(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: name.into(),
            kind,
            currency: EUR,
            account_type: (kind == AccountKind::Asset)
                .then_some(kassensturz_core::AccountType::Checking),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
        id
    }

    pub(crate) fn fixture() -> Fixture {
        let mut book = Book::create_in_memory().expect("book created");
        book.use_currency(EUR).expect("currency used");
        let checking = own(&mut book, "Checking", AccountKind::Asset);
        let savings = own(&mut book, "Savings", AccountKind::Asset);
        let groceries = own(&mut book, "Groceries", AccountKind::Expense);
        let today = TestLocale.today();
        let mut add = |splits: Vec<Split>, payee: &str| {
            let transaction =
                Transaction::new(TransactionId::generate(), today, Some(payee.into()), splits)
                    .expect("balanced");
            book.add_transaction(&transaction).expect("added");
        };
        let money = |minor| Money::from_minor_units(minor, EUR);
        add(
            vec![
                Split::new(checking, money(-2_340)),
                Split::new(groceries, money(2_340)),
            ],
            "Corner shop",
        );
        add(
            vec![
                Split::new(checking, money(-10_000)),
                Split::new(savings, money(10_000)),
            ],
            "",
        );
        Fixture {
            book,
            checking,
            savings,
            groceries,
        }
    }

    #[test]
    fn rows_are_text_with_their_kind_and_the_account_they_are_listed_from() {
        let f = fixture();
        let (view, filter) =
            tile(&f.book, &TestLocale, "this-month", "20", &Scope::default()).expect("tile");
        assert!(!filter.nothing);
        assert_eq!(view.total, 2);
        assert!(view.several_accounts);
        // The Transfer, added last, first; from the account money reached.
        let moved = &view.rows[0];
        assert_eq!(moved.kind, "transfer");
        assert_eq!(moved.account_id, f.savings.to_string());
        assert_eq!(moved.account, "Savings");
        assert_eq!(moved.other, "Checking");
        assert!(moved.incoming);
        let shop = &view.rows[1];
        assert_eq!(shop.kind, "simple");
        assert_eq!(shop.other, "Groceries");
        assert_eq!(shop.other_party, "Corner shop");
        assert_eq!(shop.account_id, f.checking.to_string());
        assert!(!shop.incoming);
        assert!(shop.amount.exact.contains("23"), "{:?}", shop.amount);
        let _ = f.groceries;
    }

    #[test]
    fn the_row_count_word_falls_back_to_twenty() {
        assert_eq!(rows_of("10"), 10);
        assert_eq!(rows_of("200"), 200);
        assert_eq!(rows_of(""), 20);
        assert_eq!(rows_of("1000"), 20);
    }
}
