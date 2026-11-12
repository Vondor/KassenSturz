// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the transactions tile's list (US4.7, D-716) on a real book made
//! with the store's own functions. Synthetic data only.

use kassensturz_core::{AccountKind, ForeignAmount, Split, Transaction, TransactionId};
use kassensturz_store::Book;

use super::*;
use crate::tests::{EUR, USD, account, date, money};

struct Fixture {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    dollars: AccountId,
    food: AccountId,
    groceries: AccountId,
    rent: AccountId,
    salary: AccountId,
}

fn fixture() -> Fixture {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    book.use_currency(USD).expect("currency used");
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR, None);
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD, None);
    let food = account(&mut book, "Food", AccountKind::Expense, EUR, None);
    let groceries = account(
        &mut book,
        "Groceries",
        AccountKind::Expense,
        EUR,
        Some(food),
    );
    let rent = account(&mut book, "Rent", AccountKind::Expense, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    Fixture {
        book,
        checking,
        savings,
        dollars,
        food,
        groceries,
        rent,
        salary,
    }
}

fn add(book: &mut Book, day: Date, payee: &str, splits: Vec<Split>) -> TransactionId {
    let transaction = Transaction::new(
        TransactionId::generate(),
        day,
        Some(payee.to_owned()),
        splits,
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction.id()
}

fn list(fixture: &Fixture, filter: &Filter, limit: usize) -> ListedTransactions {
    let period = Period::new(date(2026, 11, 1), date(2026, 11, 30)).expect("period");
    let source = Source::read(&fixture.book, period).expect("read");
    listed_transactions(&source, period, filter, limit).expect("listed")
}

#[test]
fn rows_are_newest_first_with_their_other_side() {
    let mut f = fixture();
    let (checking, savings, groceries, rent, salary) =
        (f.checking, f.savings, f.groceries, f.rent, f.salary);
    let pay = add(
        &mut f.book,
        date(2026, 11, 1),
        "Employer",
        vec![
            Split::new(checking, money(250_000)),
            Split::new(salary, money(-250_000)),
        ],
    );
    let shop = add(
        &mut f.book,
        date(2026, 11, 5),
        "Corner shop",
        vec![
            Split::new(checking, money(-2_340)),
            Split::new(groceries, money(2_340)),
        ],
    );
    let unsorted = add(
        &mut f.book,
        date(2026, 11, 5),
        "Card payment",
        vec![
            Split::new(checking, money(-990)),
            Split::uncategorized(money(990)),
        ],
    );
    let parts = add(
        &mut f.book,
        date(2026, 11, 7),
        "Market",
        vec![
            Split::new(checking, money(-80_000)),
            Split::new(groceries, money(5_000)),
            Split::new(rent, money(75_000)),
        ],
    );
    let moved = add(
        &mut f.book,
        date(2026, 11, 9),
        "",
        vec![
            Split::new(checking, money(-10_000)),
            Split::new(savings, money(10_000)),
        ],
    );
    // Outside the period.
    add(
        &mut f.book,
        date(2026, 10, 31),
        "Corner shop",
        vec![
            Split::new(checking, money(-100)),
            Split::new(groceries, money(100)),
        ],
    );
    let listed = list(&f, &Filter::default(), MAX_LISTED);
    assert_eq!(listed.total, 5);
    let ids: Vec<_> = listed.rows.iter().map(|row| row.transaction).collect();
    // On 5 November the one added last comes first (D-067).
    assert_eq!(ids, [moved, parts, unsorted, shop, pay]);
    let others: Vec<_> = listed.rows.iter().map(|row| row.other).collect();
    assert_eq!(
        others,
        [
            // The Transfer is listed once, from the account it reached.
            OtherSide::Transfer(checking),
            OtherSide::Parts,
            OtherSide::NotSorted,
            OtherSide::Category(groceries),
            OtherSide::Category(salary),
        ]
    );
    assert_eq!(listed.rows[0].account, savings);
    assert_eq!(listed.rows[0].amount, money(10_000));
    assert_eq!(listed.rows[1].amount, money(-80_000));
    assert_eq!(listed.rows[3].other_party, "Corner shop");
    assert_eq!(listed.rows[3].date, date(2026, 11, 5));
}

#[test]
fn a_category_filter_keeps_a_split_with_one_fitting_part_and_drops_transfers() {
    let mut f = fixture();
    let (checking, savings, food, groceries, rent) =
        (f.checking, f.savings, f.food, f.groceries, f.rent);
    let parts = add(
        &mut f.book,
        date(2026, 11, 7),
        "Market",
        vec![
            Split::new(checking, money(-80_000)),
            Split::new(groceries, money(5_000)),
            Split::new(rent, money(75_000)),
        ],
    );
    add(
        &mut f.book,
        date(2026, 11, 8),
        "Landlord",
        vec![
            Split::new(checking, money(-75_000)),
            Split::new(rent, money(75_000)),
        ],
    );
    add(
        &mut f.book,
        date(2026, 11, 9),
        "",
        vec![
            Split::new(checking, money(-10_000)),
            Split::new(savings, money(10_000)),
        ],
    );
    add(
        &mut f.book,
        date(2026, 11, 9),
        "Card payment",
        vec![
            Split::new(checking, money(-990)),
            Split::uncategorized(money(990)),
        ],
    );
    // Food holds Groceries: a category counts with what is inside it.
    let filter = Filter {
        accounts: None,
        categories: Some(vec![food]),
    };
    let listed = list(&f, &filter, MAX_LISTED);
    assert_eq!(listed.total, 1);
    assert_eq!(listed.rows[0].transaction, parts);
    // The row shows the whole amount of the account, not only the part.
    assert_eq!(listed.rows[0].amount, money(-80_000));
    // A filter that chooses nothing lists nothing.
    let nothing = Filter {
        accounts: None,
        categories: Some(Vec::new()),
    };
    assert_eq!(list(&f, &nothing, MAX_LISTED).total, 0);
}

#[test]
fn an_accounts_filter_lists_only_their_sides() {
    let mut f = fixture();
    let (checking, savings, groceries) = (f.checking, f.savings, f.groceries);
    add(
        &mut f.book,
        date(2026, 11, 5),
        "Corner shop",
        vec![
            Split::new(checking, money(-2_340)),
            Split::new(groceries, money(2_340)),
        ],
    );
    let moved = add(
        &mut f.book,
        date(2026, 11, 9),
        "",
        vec![
            Split::new(checking, money(-10_000)),
            Split::new(savings, money(10_000)),
        ],
    );
    let filter = Filter {
        accounts: Some(vec![checking]),
        categories: None,
    };
    let listed = list(&f, &filter, MAX_LISTED);
    assert_eq!(listed.total, 2);
    // Only the checking side of the Transfer, though money reached savings.
    assert_eq!(listed.rows[0].transaction, moved);
    assert_eq!(listed.rows[0].account, checking);
    assert_eq!(listed.rows[0].amount, money(-10_000));
    assert_eq!(listed.rows[0].other, OtherSide::Transfer(savings));
}

#[test]
fn a_transfer_is_listed_from_the_side_with_the_later_date() {
    let mut f = fixture();
    let (checking, savings) = (f.checking, f.savings);
    let mut reached = Split::new(savings, money(10_000));
    reached.own_date = Some(date(2026, 11, 3));
    reached.own_payee = Some("From checking".to_owned());
    let mut left = Split::new(checking, money(-10_000));
    left.own_date = Some(date(2026, 11, 4));
    add(
        &mut f.book,
        date(2026, 11, 3),
        "To savings",
        vec![left, reached],
    );
    let listed = list(&f, &Filter::default(), MAX_LISTED);
    assert_eq!(listed.total, 1);
    assert_eq!(listed.rows[0].account, checking);
    assert_eq!(listed.rows[0].date, date(2026, 11, 4));
    assert_eq!(listed.rows[0].other_party, "To savings");
}

#[test]
fn each_row_keeps_its_accounts_currency() {
    let mut f = fixture();
    let (dollars, groceries) = (f.dollars, f.groceries);
    // Paid from the dollar account, counted in euros: the row shows the
    // dollars the account lost.
    let mut paid = Split::new(dollars, money(-9_250));
    paid.foreign = Some(ForeignAmount {
        amount: Money::from_minor_units(-10_000, USD),
        rate: None,
    });
    add(
        &mut f.book,
        date(2026, 11, 6),
        "Diner",
        vec![paid, Split::new(groceries, money(9_250))],
    );
    let listed = list(&f, &Filter::default(), MAX_LISTED);
    assert_eq!(listed.rows[0].amount, Money::from_minor_units(-10_000, USD));
}

#[test]
fn starting_balances_and_closed_accounts_are_left_out_unless_chosen() {
    let mut f = fixture();
    let checking = f.checking;
    let opening = account(
        &mut f.book,
        "Opening balances",
        AccountKind::Equity,
        EUR,
        None,
    );
    add(
        &mut f.book,
        date(2026, 11, 1),
        "",
        vec![
            Split::new(checking, money(5_000)),
            Split::new(opening, money(-5_000)),
        ],
    );
    // A card used once and refunded, then closed with nothing left.
    let old = account(&mut f.book, "Old card", AccountKind::Asset, EUR, None);
    let groceries = f.groceries;
    for amount in [-100, 100] {
        add(
            &mut f.book,
            date(2026, 11, 2),
            "Corner shop",
            vec![
                Split::new(old, money(amount)),
                Split::new(groceries, money(-amount)),
            ],
        );
    }
    f.book.close_account(old).expect("closed");
    assert_eq!(list(&f, &Filter::default(), MAX_LISTED).total, 0);
    let chosen = Filter {
        accounts: Some(vec![old]),
        categories: None,
    };
    assert_eq!(list(&f, &chosen, MAX_LISTED).total, 2);
}

#[test]
fn the_list_is_cut_at_the_limit_and_counts_the_rest() {
    let mut f = fixture();
    let (checking, groceries) = (f.checking, f.groceries);
    for day in 1..=30 {
        for _ in 0..8 {
            add(
                &mut f.book,
                date(2026, 11, day),
                "Corner shop",
                vec![
                    Split::new(checking, money(-100)),
                    Split::new(groceries, money(100)),
                ],
            );
        }
    }
    let ten = list(&f, &Filter::default(), 10);
    assert_eq!((ten.rows.len(), ten.total), (10, 240));
    assert_eq!(ten.rows[0].date, date(2026, 11, 30));
    // Never more than the most the tile shows, whatever is asked.
    let all = list(&f, &Filter::default(), usize::MAX);
    assert_eq!((all.rows.len(), all.total), (MAX_LISTED, 240));
}

#[test]
fn a_period_the_source_was_not_read_for_is_refused() {
    let f = fixture();
    let read = Period::new(date(2026, 11, 1), date(2026, 11, 30)).expect("period");
    let source = Source::read(&f.book, read).expect("read");
    let wider = Period::new(date(2026, 10, 1), date(2026, 11, 30)).expect("period");
    assert!(matches!(
        listed_transactions(&source, wider, &Filter::default(), 20),
        Err(ReportError::PeriodNotRead)
    ));
}
