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
        tags: None,
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
        tags: None,
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
        tags: None,
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
        tags: None,
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

/// The rows of `filter` in the days `first` to `last`, read the way the tile
/// reads them.
fn tile_rows(f: &Fixture, first: Date, last: Date, filter: &Filter) -> Vec<Listed> {
    let period = Period::new(first, last).expect("period");
    let source = Source::read_by_side_dates(&f.book, period).expect("read");
    let listed = listed_transactions(&source, period, filter, MAX_LISTED).expect("listed");
    assert_eq!(listed.total, listed.rows.len());
    listed.rows
}

fn only(accounts: &[AccountId]) -> Filter {
    Filter {
        accounts: Some(accounts.to_vec()),
        ..Filter::default()
    }
}

#[test]
fn a_transfer_with_sides_on_two_days_is_on_each_day_of_its_own_account_only() {
    // B-348: Checking Nov 1, Savings Nov 2, booked Nov 1.
    let mut f = fixture();
    let (checking, savings) = (f.checking, f.savings);
    let mut left = Split::new(checking, money(-4_200));
    left.own_date = Some(date(2026, 11, 1));
    let mut reached = Split::new(savings, money(4_200));
    reached.own_date = Some(date(2026, 11, 2));
    let id = add(&mut f.book, date(2026, 11, 1), "Move", vec![left, reached]);
    let (d1, d2) = (date(2026, 11, 1), date(2026, 11, 2));

    // Savings only: nothing on Nov 1, one row on Nov 2 with Nov 2.
    assert!(tile_rows(&f, d1, d1, &only(&[savings])).is_empty());
    let rows = tile_rows(&f, d2, d2, &only(&[savings]));
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].transaction, rows[0].date), (id, d2));
    assert_eq!(rows[0].amount, money(4_200));
    // Checking only: the other way round.
    assert!(tile_rows(&f, d2, d2, &only(&[checking])).is_empty());
    let rows = tile_rows(&f, d1, d1, &only(&[checking]));
    assert_eq!((rows.len(), rows[0].date), (1, d1));
    // Both accounts: one row a day, from the side that shows that day.
    for (day, account) in [(d1, checking), (d2, savings)] {
        let rows = tile_rows(&f, day, day, &Filter::default());
        assert_eq!(
            (rows.len(), rows[0].account, rows[0].date),
            (1, account, day)
        );
    }
    // Both days: one row, from the later side as before.
    let rows = tile_rows(&f, d1, d2, &only(&[checking, savings]));
    assert_eq!(
        (rows.len(), rows[0].account, rows[0].date),
        (1, savings, d2)
    );
}

#[test]
fn a_transfer_over_a_month_boundary_shows_no_date_outside_the_period() {
    let mut f = fixture();
    let (checking, savings) = (f.checking, f.savings);
    let mut left = Split::new(checking, money(-1_000));
    left.own_date = Some(date(2026, 10, 31));
    let mut reached = Split::new(savings, money(1_000));
    reached.own_date = Some(date(2026, 11, 1));
    add(&mut f.book, date(2026, 10, 31), "Move", vec![left, reached]);
    let november = (date(2026, 11, 1), date(2026, 11, 30));
    let october = (date(2026, 10, 1), date(2026, 10, 31));
    let rows = tile_rows(&f, november.0, november.1, &only(&[checking, savings]));
    assert_eq!((rows.len(), rows[0].date), (1, date(2026, 11, 1)));
    assert!(tile_rows(&f, november.0, november.1, &only(&[checking])).is_empty());
    let rows = tile_rows(&f, october.0, october.1, &only(&[checking, savings]));
    assert_eq!((rows.len(), rows[0].date), (1, date(2026, 10, 31)));
    assert!(tile_rows(&f, october.0, october.1, &only(&[savings])).is_empty());
}

fn tag(book: &mut Book, name: &str) -> kassensturz_core::TagId {
    let id = kassensturz_core::TagId::generate();
    book.add_tag(&kassensturz_core::Tag {
        id,
        name: name.into(),
    })
    .expect("tag added");
    id
}

fn tagged(split: Split, tags: &[kassensturz_core::TagId]) -> Split {
    Split {
        tags: tags.to_vec(),
        ..split
    }
}

fn add_entry(book: &mut Book, transaction: Transaction) -> TransactionId {
    book.add_transaction(&transaction).expect("added");
    transaction.id()
}

/// The listed transactions' IDs, sorted, so a test names the set.
fn ids(listed: &ListedTransactions) -> Vec<TransactionId> {
    let mut ids: Vec<_> = listed.rows.iter().map(|row| row.transaction).collect();
    ids.sort();
    ids
}

fn sorted(mut ids: Vec<TransactionId>) -> Vec<TransactionId> {
    ids.sort();
    ids
}

/// The book of the tag tests (D-838): a tag on a whole bill, on one part,
/// on a Transfer, on the account side only, and nowhere.
struct Tagged {
    f: Fixture,
    tax: kassensturz_core::TagId,
    holiday: kassensturz_core::TagId,
    /// Rent, the whole transaction tagged Tax.
    rent_bill: TransactionId,
    /// Groceries tagged Tax plus rent untagged.
    groceries_part: TransactionId,
    /// A Transfer tagged Tax.
    transfer: TransactionId,
    /// Groceries untagged plus rent tagged Tax.
    rent_part: TransactionId,
    /// Groceries, only the account's own side tagged Tax.
    account_side: TransactionId,
    /// Groceries tagged Holiday.
    holiday_shop: TransactionId,
}

fn tagged_book() -> Tagged {
    let mut f = fixture();
    let tax = tag(&mut f.book, "Tax");
    let holiday = tag(&mut f.book, "Holiday");
    let (checking, savings, groceries, rent) = (f.checking, f.savings, f.groceries, f.rent);
    let entry = |day: i8, splits: Vec<Split>| {
        Transaction::new(TransactionId::generate(), date(2026, 11, day), None, splits)
            .expect("balanced")
    };
    let rent_bill = add_entry(
        &mut f.book,
        entry(
            2,
            vec![
                Split::new(checking, money(-50_000)),
                Split::new(rent, money(50_000)),
            ],
        )
        .with_tags(vec![tax]),
    );
    let groceries_part = add_entry(
        &mut f.book,
        entry(
            3,
            vec![
                Split::new(checking, money(-5_000)),
                tagged(Split::new(groceries, money(3_000)), &[tax]),
                Split::new(rent, money(2_000)),
            ],
        ),
    );
    let transfer = add_entry(
        &mut f.book,
        entry(
            4,
            vec![
                Split::new(checking, money(-10_000)),
                Split::new(savings, money(10_000)),
            ],
        )
        .with_tags(vec![tax]),
    );
    let rent_part = add_entry(
        &mut f.book,
        entry(
            5,
            vec![
                Split::new(checking, money(-5_000)),
                Split::new(groceries, money(1_000)),
                tagged(Split::new(rent, money(4_000)), &[tax]),
            ],
        ),
    );
    let account_side = add_entry(
        &mut f.book,
        entry(
            6,
            vec![
                tagged(Split::new(checking, money(-700)), &[tax]),
                Split::new(groceries, money(700)),
            ],
        ),
    );
    let holiday_shop = add_entry(
        &mut f.book,
        entry(
            7,
            vec![
                Split::new(checking, money(-900)),
                tagged(Split::new(groceries, money(900)), &[holiday]),
            ],
        ),
    );
    // Nothing tagged: never listed by a tag.
    add_entry(
        &mut f.book,
        entry(
            8,
            vec![
                Split::new(checking, money(-1_100)),
                Split::new(groceries, money(1_100)),
            ],
        ),
    );
    Tagged {
        f,
        tax,
        holiday,
        rent_bill,
        groceries_part,
        transfer,
        rent_part,
        account_side,
        holiday_shop,
    }
}

#[test]
fn a_tag_alone_lists_every_transaction_that_carries_it_anywhere_transfers_included() {
    let t = tagged_book();
    let filter = Filter {
        tags: Some(vec![t.tax]),
        ..Filter::default()
    };
    let listed = list(&t.f, &filter, MAX_LISTED);
    assert_eq!(
        ids(&listed),
        sorted(vec![
            t.rent_bill,
            t.groceries_part,
            t.transfer,
            t.rent_part,
            t.account_side
        ])
    );
    assert_eq!(listed.total, 5);
    // A row still shows the whole amount of its account, not the tagged part.
    let row = listed
        .rows
        .iter()
        .find(|row| row.transaction == t.groceries_part)
        .expect("listed");
    assert_eq!(row.amount, money(-5_000));
    assert_eq!(row.other, OtherSide::Parts);
    // The Transfer between two listed accounts stays one row (D-108).
    assert_eq!(
        listed
            .rows
            .iter()
            .filter(|row| row.transaction == t.transfer)
            .count(),
        1
    );
}

#[test]
fn several_tags_are_any_of_them_and_an_empty_list_lists_nothing() {
    let t = tagged_book();
    let either = Filter {
        tags: Some(vec![t.holiday, t.tax]),
        ..Filter::default()
    };
    assert_eq!(list(&t.f, &either, MAX_LISTED).total, 6);
    let holiday = Filter {
        tags: Some(vec![t.holiday]),
        ..Filter::default()
    };
    assert_eq!(ids(&list(&t.f, &holiday, MAX_LISTED)), [t.holiday_shop]);
    let nothing = Filter {
        tags: Some(Vec::new()),
        ..Filter::default()
    };
    assert_eq!(list(&t.f, &nothing, MAX_LISTED).total, 0);
}

#[test]
fn a_tag_with_categories_needs_one_part_in_a_category_that_carries_it() {
    let t = tagged_book();
    // Food holds Groceries. Only the bill whose groceries part is tagged:
    // not the one whose rent part is, nor the one tagged on the account's
    // side, nor the Transfer, which has no category.
    let food = Filter {
        categories: Some(vec![t.f.food]),
        tags: Some(vec![t.tax]),
        ..Filter::default()
    };
    assert_eq!(ids(&list(&t.f, &food, MAX_LISTED)), [t.groceries_part]);
    // Rent: the bill tagged as a whole counts its rent part, and so does
    // the bill whose rent part carries the tag; the untagged rent part of
    // the groceries bill does not.
    let rent = Filter {
        categories: Some(vec![t.f.rent]),
        tags: Some(vec![t.tax]),
        ..Filter::default()
    };
    assert_eq!(
        ids(&list(&t.f, &rent, MAX_LISTED)),
        sorted(vec![t.rent_bill, t.rent_part])
    );
    // OR within each list, AND between them (D-841).
    let both = Filter {
        categories: Some(vec![t.f.food, t.f.rent]),
        tags: Some(vec![t.tax, t.holiday]),
        ..Filter::default()
    };
    assert_eq!(
        ids(&list(&t.f, &both, MAX_LISTED)),
        sorted(vec![
            t.rent_bill,
            t.groceries_part,
            t.rent_part,
            t.holiday_shop
        ])
    );
}

#[test]
fn a_tag_with_an_accounts_filter_lists_only_their_sides() {
    let t = tagged_book();
    let filter = Filter {
        accounts: Some(vec![t.f.savings]),
        tags: Some(vec![t.tax]),
        ..Filter::default()
    };
    let listed = list(&t.f, &filter, MAX_LISTED);
    assert_eq!(ids(&listed), [t.transfer]);
    assert_eq!(listed.rows[0].account, t.f.savings);
    assert_eq!(listed.rows[0].amount, money(10_000));
}
