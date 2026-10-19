// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of transactions and their parts, on books in memory. All data is
//! made up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{
    AccountKind, ExchangeRate, RateSource, RoundingMode, Split, TransactionError,
};
use rusqlite::params;

use super::*;
use crate::tests::{EUR, Sample, USD, account, date, eur, sample, transaction};

fn usd(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, USD)
}

/// `transaction` with other parts, as an edit in a Parts editor makes it.
fn with_parts(transaction: &Transaction, parts: Vec<Split>) -> Transaction {
    Transaction::new(
        transaction.id(),
        transaction.booking_date(),
        transaction.payee().map(Into::into),
        parts,
    )
    .expect("balanced")
}

/// What a bank might deliver for a card payment. The IBAN is the example
/// for Germany in SWIFT's IBAN Registry, never one that could belong to
/// someone (D-055).
fn delivered() -> ImportedDetails {
    ImportedDetails {
        bank_id: Some("2026100500017".into()),
        booking_date: Some(date(2026, 10, 5)),
        value_date: Some(date(2026, 10, 6)),
        other_party_name: Some("CORNER GROCERY 0815".into()),
        other_party_iban: Some("DE89370400440532013000".into()),
        remittance_information: Some("Card payment 2026-10-04 Müllerstr.".into()),
    }
}

/// A card payment as an import (M2) would add it: the checking account's
/// part with what the bank delivered, and a part without a category yet.
fn imported_purchase(book: &mut Book, checking: AccountId) -> Transaction {
    let bank_part = Split {
        imported: Some(delivered()),
        ..Split::new(checking, eur(-5320))
    };
    let purchase = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        Some("Corner Grocery".into()),
        vec![bank_part, Split::uncategorized(eur(5320))],
    )
    .expect("balanced");
    book.add_transaction(&purchase).expect("added");
    purchase
}

fn rows(book: &Book, table: &str) -> i64 {
    book.conn
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("rows counted")
}

fn rate(day: i8, from: CurrencyCode, to: CurrencyCode, rate: &str) -> DatedRate {
    DatedRate {
        id: DatedRateId::generate(),
        date: date(2026, 10, day),
        from,
        to,
        rate: rate.parse().expect("valid rate"),
        source: RateSource::Manual,
    }
}

/// A book in euros and dollars with a euro checking account and a dollar
/// account, e.g. one kept for trips.
struct TwoCurrencies {
    book: Book,
    checking: AccountId,
    dollars: AccountId,
}

fn two_currencies() -> TwoCurrencies {
    let Sample {
        mut book, checking, ..
    } = sample();
    book.use_currency(USD).expect("currency used");
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD);
    TwoCurrencies {
        book,
        checking,
        dollars,
    }
}

/// 1000.00 EUR moved into the dollar account, which received 1085.60 USD.
fn transfer(checking: AccountId, dollars: AccountId, rate: Option<DatedRate>) -> Transaction {
    let received = Split {
        foreign: Some(ForeignAmount {
            amount: usd(108_560),
            rate,
        }),
        ..Split::new(dollars, eur(100_000))
    };
    Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 7),
        Some("To the dollar account".into()),
        vec![Split::new(checking, eur(-100_000)), received],
    )
    .expect("balanced in euros")
}

#[test]
fn a_part_without_a_category_counts_in_no_balance() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    assert_eq!(book.transaction(purchase.id()).expect("read"), purchase);
    assert_eq!(book.balance(checking).expect("balance"), eur(-5320));
    assert_eq!(book.balance(groceries).expect("balance"), eur(0));
    assert_eq!(book.register(checking).expect("register").len(), 1);
}

#[test]
fn a_purchase_is_broken_down_into_parts_after_the_fact() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let household = account(&mut book, "Household", AccountKind::Expense, EUR);
    let fast_food = account(&mut book, "Fast food", AccountKind::Expense, EUR);
    let purchase = imported_purchase(&mut book, checking);

    // 40.00 Food and 8.00 Household are entered; Fast food gets what is
    // left, as the Parts editor calculates it (D-017).
    let mut parts = vec![
        purchase.splits()[0].clone(),
        Split::new(groceries, eur(4000)),
        Split::new(household, eur(800)),
        Split::new(fast_food, eur(0)),
    ];
    let open = parts[3].id;
    parts[3].amount = Transaction::remainder(&parts, open).expect("remainder");
    let broken_down = with_parts(&purchase, parts);
    book.update_transaction(&broken_down).expect("updated");

    assert_eq!(book.transaction(purchase.id()).expect("read"), broken_down);
    assert_eq!(book.balance(fast_food).expect("balance"), eur(520));
    assert_eq!(book.balance(household).expect("balance"), eur(800));
    // The bank's side didn't change.
    assert_eq!(book.balance(checking).expect("balance"), eur(-5320));
    assert_eq!(rows(&book, "splits"), 4);
    assert_eq!(rows(&book, "split_imports"), 1);
}

#[test]
fn a_transaction_can_be_changed_after_the_fact() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&shopping).expect("added");

    // A typo in the amount, the date and the payee, all fixed at once.
    let fixed = Transaction::new(
        shopping.id(),
        date(2026, 10, 4),
        Some("Corner Grocer".into()),
        vec![
            Split {
                amount: eur(-3520),
                ..shopping.splits()[0].clone()
            },
            Split {
                amount: eur(3520),
                ..shopping.splits()[1].clone()
            },
        ],
    )
    .expect("balanced");
    book.update_transaction(&fixed).expect("updated");
    assert_eq!(book.transaction(shopping.id()).expect("read"), fixed);
    assert_eq!(
        book.register(checking).expect("register"),
        [RegisterEntry {
            transaction: shopping.id(),
            booking_date: date(2026, 10, 4),
            payee: Some("Corner Grocer".into()),
            amount: eur(-3520),
        }]
    );

    // It was income, not shopping: the category changes too.
    let refund = with_parts(
        &fixed,
        vec![
            Split::new(checking, eur(3520)),
            Split::new(salary, eur(-3520)),
        ],
    );
    book.update_transaction(&refund).expect("updated");
    assert_eq!(book.balance(groceries).expect("balance"), eur(0));
    assert_eq!(book.balance(checking).expect("balance"), eur(3520));
    assert_eq!(rows(&book, "splits"), 2);
}

#[test]
fn changes_are_checked_like_new_transactions() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    assert!(matches!(
        book.update_transaction(&shopping),
        Err(StoreError::UnknownTransaction(id)) if id == shopping.id()
    ));
    book.add_transaction(&shopping).expect("added");
    let unknown = AccountId::generate();
    let changed = with_parts(
        &shopping,
        vec![
            Split::new(checking, eur(-5320)),
            Split::new(unknown, eur(5320)),
        ],
    );
    assert!(matches!(
        book.update_transaction(&changed),
        Err(StoreError::UnknownAccount(id)) if id == unknown
    ));
    assert_eq!(book.transaction(shopping.id()).expect("read"), shopping);
}

#[test]
fn ids_in_use_are_refused() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&shopping).expect("added");
    assert!(matches!(
        book.add_transaction(&shopping),
        Err(StoreError::TransactionIdInUse(id)) if id == shopping.id()
    ));

    // A new transaction can't take over a part of another one.
    let taken = shopping.splits()[1].clone();
    let other = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 6),
        None,
        vec![Split::new(checking, eur(-5320)), taken.clone()],
    )
    .expect("balanced");
    assert!(matches!(
        book.add_transaction(&other),
        Err(StoreError::SplitIdInUse(id)) if id == taken.id
    ));
    let second = transaction(
        date(2026, 10, 6),
        "Corner Grocery",
        &[(checking, eur(-100)), (groceries, eur(100))],
    );
    book.add_transaction(&second).expect("added");
    let changed = with_parts(
        &second,
        vec![Split::new(checking, eur(-5320)), taken.clone()],
    );
    assert!(matches!(
        book.update_transaction(&changed),
        Err(StoreError::SplitIdInUse(id)) if id == taken.id
    ));
    assert_eq!(rows(&book, "splits"), 4);
}

#[test]
fn a_part_in_another_currency_counts_in_its_accounts_currency() {
    let TwoCurrencies {
        mut book,
        checking,
        dollars,
    } = two_currencies();
    let moved = transfer(checking, dollars, None);
    book.add_transaction(&moved).expect("added");
    assert_eq!(book.transaction(moved.id()).expect("read"), moved);
    assert_eq!(book.balance(dollars).expect("balance"), usd(108_560));
    assert_eq!(book.balance(checking).expect("balance"), eur(-100_000));
    let register = book.register(dollars).expect("register");
    assert_eq!(register.len(), 1);
    assert_eq!(register[0].amount, usd(108_560));
}

#[test]
fn a_category_can_count_in_another_currency() {
    // The hotel bill of D-043: 100 USD on the bill, 92.50 EUR charged to the
    // euro card, booked to a Travel category kept in dollars.
    let TwoCurrencies {
        mut book, checking, ..
    } = two_currencies();
    let travel = account(&mut book, "Travel", AccountKind::Expense, USD);
    let hotel = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 8),
        Some("Example Hotel".into()),
        vec![
            Split::new(checking, eur(-9250)),
            Split {
                foreign: Some(ForeignAmount {
                    amount: usd(10_000),
                    rate: None,
                }),
                ..Split::new(travel, eur(9250))
            },
        ],
    )
    .expect("balanced in euros");
    book.add_transaction(&hotel).expect("added");
    assert_eq!(book.balance(travel).expect("balance"), usd(10_000));
    assert_eq!(book.transaction(hotel.id()).expect("read"), hotel);
}

#[test]
fn each_part_must_be_in_its_accounts_currency() {
    let TwoCurrencies {
        mut book,
        checking,
        dollars,
    } = two_currencies();
    // Euros booked to the dollar account without its amount in dollars.
    let wrong = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 7),
        None,
        vec![
            Split::new(checking, eur(-100_000)),
            Split::new(dollars, eur(100_000)),
        ],
    )
    .expect("balanced");
    assert!(matches!(
        book.add_transaction(&wrong),
        Err(StoreError::SplitCurrencyMismatch {
            account,
            account_currency: USD,
            split_currency: EUR,
        }) if account == dollars
    ));
    // Yen booked to the dollar account.
    let yen = CurrencyCode::new("JPY").expect("valid code");
    let mut parts = transfer(checking, dollars, None).splits().to_vec();
    parts[1].foreign = Some(ForeignAmount {
        amount: Money::from_minor_units(160_000, yen),
        rate: None,
    });
    let wrong = Transaction::new(TransactionId::generate(), date(2026, 10, 7), None, parts)
        .expect("balanced");
    assert!(matches!(
        book.add_transaction(&wrong),
        Err(StoreError::SplitCurrencyMismatch {
            split_currency,
            ..
        }) if split_currency == yen
    ));
    assert_eq!(rows(&book, "transactions"), 0);
}

#[test]
fn a_conversion_takes_the_newest_rate_in_either_direction() {
    let TwoCurrencies { mut book, .. } = two_currencies();
    let euro_rate = rate(1, EUR, USD, "1.25");
    let dollar_rate = rate(10, USD, EUR, "0.8500");
    book.add_rate(&euro_rate).expect("rate added");
    book.add_rate(&dollar_rate).expect("rate added");

    // Before the 10th, only the euro's rate applies: dollars are divided by
    // it. 100.00 USD ÷ 1.25 = 80.00 EUR.
    assert_eq!(
        book.convert(usd(10_000), EUR, date(2026, 10, 5))
            .expect("converted"),
        (eur(8000), euro_rate)
    );
    // From the 10th, the dollar's rate is newer: 100.00 USD × 0.85.
    assert_eq!(
        book.convert(usd(10_000), EUR, date(2026, 10, 12))
            .expect("converted"),
        (eur(8500), dollar_rate)
    );
    // Euros are multiplied by the euro's rate: 80.00 EUR × 1.25.
    assert_eq!(
        book.convert(eur(8000), USD, date(2026, 10, 9))
            .expect("converted"),
        (usd(10_000), euro_rate)
    );

    // Two rates on the same day: the one from the amount's currency counts.
    let euro_again = rate(20, EUR, USD, "1.20");
    let dollar_again = rate(20, USD, EUR, "0.80");
    book.add_rate(&euro_again).expect("rate added");
    book.add_rate(&dollar_again).expect("rate added");
    let day = date(2026, 10, 20);
    assert_eq!(
        book.convert(usd(10_000), EUR, day).expect("converted"),
        (eur(8000), dollar_again)
    );
    assert_eq!(
        book.convert(eur(10_000), USD, day).expect("converted"),
        (usd(12_000), euro_again)
    );
}

#[test]
fn a_conversion_needs_a_rate_and_two_currencies() {
    let TwoCurrencies { mut book, .. } = two_currencies();
    book.add_rate(&rate(10, EUR, USD, "1.0856"))
        .expect("rate added");
    let day = date(2026, 10, 9);
    assert!(matches!(
        book.convert(eur(100), USD, day),
        Err(StoreError::NoRate { from: EUR, to: USD, date }) if date == day
    ));
    assert!(matches!(
        book.convert(eur(100), EUR, day),
        Err(StoreError::RateWithinOneCurrency(EUR))
    ));
    let yen = CurrencyCode::new("JPY").expect("valid code");
    assert!(matches!(
        book.convert(eur(100), yen, date(2026, 10, 12)),
        Err(StoreError::NoRate { .. })
    ));
}

#[test]
fn a_conversion_rounds_once_in_the_books_rounding_mode() {
    let TwoCurrencies { mut book, .. } = two_currencies();
    book.add_rate(&rate(1, EUR, USD, "0.5"))
        .expect("rate added");
    let day = date(2026, 10, 2);
    // 0.05 EUR × 0.5 is 2.5 cents.
    assert_eq!(book.convert(eur(5), USD, day).expect("converted").0, usd(3));
    book.set_rounding_mode(RoundingMode::HalfEven)
        .expect("rounding mode set");
    assert_eq!(book.convert(eur(5), USD, day).expect("converted").0, usd(2));
}

#[test]
fn a_part_keeps_the_rate_it_was_converted_with() {
    let TwoCurrencies {
        mut book,
        checking,
        dollars,
    } = two_currencies();
    book.add_rate(&rate(1, EUR, USD, "1.0856"))
        .expect("rate added");
    let (received, used) = book
        .convert(eur(100_000), USD, date(2026, 10, 7))
        .expect("converted");
    assert_eq!(received, usd(108_560));
    let moved = transfer(checking, dollars, Some(used));
    book.add_transaction(&moved).expect("added");
    assert_eq!(rows(&book, "split_rates"), 1);

    // Removing the rate from the price table leaves the part's copy alone.
    book.remove_rate(used.id).expect("rate removed");
    let stored = book.transaction(moved.id()).expect("read");
    assert_eq!(stored, moved);
    let foreign = stored.splits()[1].foreign.expect("in dollars");
    assert_eq!(foreign.rate, Some(used));
    assert_eq!(
        foreign.rate.map(|rate| rate.rate),
        Some(ExchangeRate::from_digits(10856, 4).expect("valid rate"))
    );
}

#[test]
fn what_a_bank_delivered_comes_back_unchanged() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let stored = book.transaction(purchase.id()).expect("read");
    assert_eq!(stored.splits()[0].imported, Some(delivered()));
    assert_eq!(stored.splits()[1].imported, None);

    // Fields the bank left out stay empty, and empty text stays empty text.
    let sparse = ImportedDetails {
        remittance_information: Some(String::new()),
        ..ImportedDetails::default()
    };
    let transfer_in = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 6),
        None,
        vec![
            Split {
                imported: Some(sparse.clone()),
                ..Split::new(checking, eur(2000))
            },
            Split::uncategorized(eur(-2000)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&transfer_in).expect("added");
    let stored = book.transaction(transfer_in.id()).expect("read");
    assert_eq!(stored.splits()[0].imported, Some(sparse));
}

#[test]
fn a_part_a_bank_delivered_stays_as_it_was() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let bank_part = purchase.splits()[0].clone();
    let refused = |parts: Vec<Split>| with_parts(&purchase, parts);

    let other_amount = refused(vec![
        Split {
            amount: eur(-5300),
            ..bank_part.clone()
        },
        Split::new(groceries, eur(5300)),
    ]);
    let other_account = refused(vec![
        Split {
            account: Some(salary),
            ..bank_part.clone()
        },
        Split::new(groceries, eur(5320)),
    ]);
    let other_details = refused(vec![
        Split {
            imported: Some(ImportedDetails {
                other_party_name: Some("Someone else".into()),
                ..delivered()
            }),
            ..bank_part.clone()
        },
        Split::new(groceries, eur(5320)),
    ]);
    let without_it = refused(vec![
        Split::new(checking, eur(-5320)),
        Split::new(groceries, eur(5320)),
    ]);
    for changed in [other_amount, other_account, other_details, without_it] {
        assert!(matches!(
            book.update_transaction(&changed),
            Err(StoreError::ImportedSplitChanged(id)) if id == bank_part.id
        ));
    }
    // Nor can an edit claim a part came from the bank.
    let claimed = Split {
        imported: Some(delivered()),
        ..Split::new(groceries, eur(5320))
    };
    let claimed_id = claimed.id;
    assert!(matches!(
        book.update_transaction(&refused(vec![bank_part.clone(), claimed])),
        Err(StoreError::ImportedSplitChanged(id)) if id == claimed_id
    ));
    assert_eq!(book.transaction(purchase.id()).expect("read"), purchase);

    // Everything else can change: the date, the payee and the other parts.
    let categorized = Transaction::new(
        purchase.id(),
        date(2026, 10, 4),
        Some("My grocery store".into()),
        vec![bank_part, Split::new(groceries, eur(5320))],
    )
    .expect("balanced");
    book.update_transaction(&categorized).expect("updated");
    assert_eq!(book.transaction(purchase.id()).expect("read"), categorized);
}

#[test]
fn a_delivered_part_can_change_its_transactions_currency() {
    // The euro card's statement says 92.50 EUR. The user enters the bill's
    // 100 USD for a Travel category in dollars, which makes the transaction
    // count in dollars; the bank's part keeps its 92.50 EUR (D-043, D-067).
    let TwoCurrencies {
        mut book, checking, ..
    } = two_currencies();
    let travel = account(&mut book, "Travel", AccountKind::Expense, USD);
    let charged = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 8),
        Some("Example Hotel".into()),
        vec![
            Split {
                imported: Some(delivered()),
                ..Split::new(checking, eur(-9250))
            },
            Split::uncategorized(eur(9250)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&charged).expect("added");
    let bank_part = Split {
        amount: usd(-10_000),
        foreign: Some(ForeignAmount {
            amount: eur(-9250),
            rate: None,
        }),
        ..charged.splits()[0].clone()
    };
    let in_dollars = with_parts(&charged, vec![bank_part, Split::new(travel, usd(10_000))]);
    book.update_transaction(&in_dollars).expect("updated");
    assert_eq!(book.transaction(charged.id()).expect("read"), in_dollars);
    assert_eq!(book.balance(checking).expect("balance"), eur(-9250));
    assert_eq!(book.balance(travel).expect("balance"), usd(10_000));
}

#[test]
fn a_closed_accounts_total_cannot_change() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    let pay = transaction(
        date(2026, 10, 1),
        "Example Employer",
        &[(checking, eur(5320)), (salary, eur(-5320))],
    );
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&pay).expect("added");
    book.add_transaction(&shopping).expect("added");
    book.close_account(checking).expect("closed at zero");

    let more = with_parts(
        &shopping,
        vec![
            Split {
                amount: eur(-6000),
                ..shopping.splits()[0].clone()
            },
            Split::new(groceries, eur(6000)),
        ],
    );
    assert!(matches!(
        book.update_transaction(&more),
        Err(StoreError::AccountClosed(id)) if id == checking
    ));
    assert!(matches!(
        book.remove_transaction(shopping.id()),
        Err(StoreError::AccountClosed(id)) if id == checking
    ));
    assert_eq!(book.balance(checking).expect("balance"), eur(0));

    // Its history can still be corrected where that keeps its total: the
    // date, the payee, the category.
    let household = account(&mut book, "Household", AccountKind::Expense, EUR);
    let corrected = Transaction::new(
        shopping.id(),
        date(2026, 10, 6),
        Some("Corner Grocery, household goods".into()),
        vec![
            shopping.splits()[0].clone(),
            Split::new(household, eur(5320)),
        ],
    )
    .expect("balanced");
    book.update_transaction(&corrected).expect("updated");
    assert_eq!(book.balance(household).expect("balance"), eur(5320));
    assert_eq!(book.balance(checking).expect("balance"), eur(0));
}

#[test]
fn removing_a_transaction_removes_all_its_parts() {
    let TwoCurrencies {
        mut book,
        checking,
        dollars,
    } = two_currencies();
    book.add_rate(&rate(1, EUR, USD, "1.0856"))
        .expect("rate added");
    let (_, used) = book
        .convert(eur(100_000), USD, date(2026, 10, 7))
        .expect("converted");
    let moved = transfer(checking, dollars, Some(used));
    book.add_transaction(&moved).expect("added");
    let purchase = imported_purchase(&mut book, checking);

    book.remove_transaction(moved.id()).expect("removed");
    book.remove_transaction(purchase.id()).expect("removed");
    for table in ["transactions", "splits", "split_rates", "split_imports"] {
        assert_eq!(rows(&book, table), 0, "{table}");
    }
    assert!(matches!(
        book.transaction(moved.id()),
        Err(StoreError::UnknownTransaction(_))
    ));
    assert!(matches!(
        book.remove_transaction(moved.id()),
        Err(StoreError::UnknownTransaction(id)) if id == moved.id()
    ));
    assert_eq!(book.balance(dollars).expect("balance"), usd(0));
    assert!(book.register(checking).expect("register").is_empty());
    // The rate stays in the price table.
    assert_eq!(rows(&book, "prices"), 1);
}

#[test]
fn a_damaged_transaction_can_still_be_removed() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&shopping).expect("added");
    // As if another program had changed the book (D-046).
    book.as_another_program()
        .execute("UPDATE splits SET amount = 5300 WHERE amount = 5320", [])
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.transaction(shopping.id()),
        Err(StoreError::Corrupt(Corruption::InvalidTransaction { .. }))
    ));
    book.remove_transaction(shopping.id()).expect("removed");
    assert_eq!(rows(&book, "splits"), 0);
}

#[test]
fn dates_before_the_year_1_are_refused_in_parts_too() {
    let TwoCurrencies {
        mut book,
        checking,
        dollars,
    } = two_currencies();
    let ancient = date(0, 12, 31);
    let mut purchase = imported_purchase(&mut book, checking).splits().to_vec();
    purchase[0].id = SplitId::generate();
    purchase[1].id = SplitId::generate();
    purchase[0].imported.as_mut().expect("delivered").value_date = Some(ancient);
    let old_statement =
        Transaction::new(TransactionId::generate(), date(2026, 10, 5), None, purchase)
            .expect("balanced");
    assert!(matches!(
        book.add_transaction(&old_statement),
        Err(StoreError::DateOutOfRange(found)) if found == ancient
    ));
    let old_rate = rate(1, EUR, USD, "1.0856");
    let old_rate = DatedRate {
        date: ancient,
        ..old_rate
    };
    assert!(matches!(
        book.add_transaction(&transfer(checking, dollars, Some(old_rate))),
        Err(StoreError::DateOutOfRange(found)) if found == ancient
    ));
    assert_eq!(rows(&book, "transactions"), 1);
}

#[test]
fn parts_in_the_wrong_currency_in_the_file_are_reported() {
    let TwoCurrencies {
        mut book,
        checking,
        dollars,
    } = two_currencies();
    let moved = transfer(checking, dollars, None);
    book.add_transaction(&moved).expect("added");
    book.use_currency(CurrencyCode::new("JPY").expect("valid code"))
        .expect("currency used");
    // As if another program had changed the book (D-046).
    book.as_another_program()
        .execute(
            "UPDATE splits SET foreign_currency = 'JPY' WHERE foreign_currency = 'USD'",
            [],
        )
        .expect("changed behind KassenSturz's back");
    let wrong_currency = |result: Result<(), StoreError>| {
        matches!(
            result,
            Err(StoreError::Corrupt(Corruption::CurrencyMismatch { transaction }))
                if transaction == moved.id()
        )
    };
    assert!(wrong_currency(book.transaction(moved.id()).map(|_| ())));
    assert!(wrong_currency(book.register(dollars).map(|_| ())));
    assert!(wrong_currency(book.balance(dollars).map(|_| ())));
}

#[test]
fn parts_that_do_not_fit_together_in_the_file_are_reported() {
    let TwoCurrencies {
        mut book,
        checking,
        dollars,
    } = two_currencies();
    let moved = transfer(checking, dollars, None);
    book.add_transaction(&moved).expect("added");
    let received = moved.splits()[1].id;

    // A foreign amount without its currency.
    book.as_another_program()
        .execute_batch("PRAGMA ignore_check_constraints = ON;")
        .expect("checks off");
    book.as_another_program()
        .execute(
            "UPDATE splits SET foreign_currency = NULL WHERE id = ?1",
            [received.to_bytes()],
        )
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.transaction(moved.id()),
        Err(StoreError::Corrupt(Corruption::BadSplit(id))) if id == received
    ));
    assert!(matches!(
        book.balance(dollars),
        Err(StoreError::Corrupt(Corruption::BadSplit(id))) if id == received
    ));

    // A rate for a part that isn't in another currency.
    book.as_another_program()
        .execute(
            "UPDATE splits SET foreign_amount = NULL WHERE id = ?1",
            [received.to_bytes()],
        )
        .expect("changed behind KassenSturz's back");
    let checking_part = moved.splits()[0].id;
    let stray = rate(1, EUR, USD, "1.0856");
    book.as_another_program()
        .execute(
            "INSERT INTO split_rates
                 (split_id, rate_id, date, from_currency, to_currency,
                  rate_digits, rate_decimal_places, source)
             VALUES (?1, ?2, '2026-10-01', 'EUR', 'USD', 10856, 4, 'manual')",
            params![checking_part.to_bytes(), stray.id.to_bytes()],
        )
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.transaction(moved.id()),
        Err(StoreError::Corrupt(Corruption::BadSplit(id))) if id == checking_part
    ));
}

#[test]
fn delivered_details_that_are_wrong_in_the_file_are_reported() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let open_part = purchase.splits()[1].id;
    // What a bank delivered, on the part without a category.
    book.as_another_program()
        .execute(
            "INSERT INTO split_imports (split_id, bank_id) VALUES (?1, 'X')",
            [open_part.to_bytes()],
        )
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.transaction(purchase.id()),
        Err(StoreError::Corrupt(Corruption::InvalidTransaction {
            error: TransactionError::ImportedWithoutAccount(id),
            ..
        })) if id == open_part
    ));
    book.as_another_program()
        .execute(
            "DELETE FROM split_imports WHERE split_id = ?1",
            [open_part.to_bytes()],
        )
        .expect("removed again");
    // A date that looks like one but isn't.
    book.as_another_program()
        .execute("UPDATE split_imports SET value_date = '2026-13-45'", [])
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.transaction(purchase.id()),
        Err(StoreError::Corrupt(Corruption::BadDate(text))) if text == "2026-13-45"
    ));
}
