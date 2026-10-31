// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what the user has, owes and is worth and of the money in and out
//! of their accounts (M6c.1, D-348, D-349): on a real book, against the
//! store's own balances and conversions, with a joined Transfer, a part in
//! another currency, a credit card, a closed account and a bank booking day;
//! and properties on exact totals.

use std::collections::HashMap;

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, DatedRate, DatedRateId,
    ForeignAmount, ImportedDetails, Money, RateSource, Split, Transaction, TransactionId,
};
use kassensturz_store::Book;
use proptest::prelude::*;

use super::*;
use crate::balances::{Holdings, Movement};
use crate::tests::{EUR, USD, Words, account, date, money, period, sample};

fn dollars(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, USD)
}

fn rate(day: Date, from: CurrencyCode, to: CurrencyCode, rate: &str) -> DatedRate {
    DatedRate {
        id: DatedRateId::generate(),
        date: day,
        from,
        to,
        rate: rate.parse().expect("valid rate"),
        source: RateSource::Manual,
    }
}

fn add(book: &mut Book, day: Date, splits: Vec<Split>) -> TransactionId {
    let transaction =
        Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction.id()
}

struct Financial {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    card: AccountId,
}

/// The report tests' sample book (starting balance, a joined Transfer from
/// checking to savings on 20 and 21 October, a dollar account) and: a credit
/// card bought on and paid from checking, money changed from euros into
/// dollars as one transaction with a part in another currency, a purchase
/// the bank booked two days after its date, an account closed after it was
/// emptied, and exchange rates in both directions.
fn financial() -> Financial {
    let base = sample();
    let mut book = base.book;
    let card = account(&mut book, "Credit card", AccountKind::Liability, EUR, None);
    let old = account(&mut book, "Old savings", AccountKind::Asset, EUR, None);
    let opening = book
        .all_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.kind == AccountKind::Equity)
        .expect("the sample's starting balances")
        .id;
    // A starting balance, emptied into checking, then closed.
    add(
        &mut book,
        date(2026, 7, 31),
        vec![
            Split::new(old, money(40_000)),
            Split::new(opening, money(-40_000)),
        ],
    );
    add(
        &mut book,
        date(2026, 8, 10),
        vec![
            Split::new(old, money(-40_000)),
            Split::new(base.checking, money(40_000)),
        ],
    );
    book.close_account(old).expect("closed");
    // Bought with the card, paid off from checking.
    add(
        &mut book,
        date(2026, 9, 15),
        vec![
            Split::new(card, money(-20_000)),
            Split::new(base.groceries, money(20_000)),
        ],
    );
    add(
        &mut book,
        date(2026, 10, 5),
        vec![
            Split::new(base.checking, money(-20_000)),
            Split::new(card, money(20_000)),
        ],
    );
    add(
        &mut book,
        date(2026, 10, 18),
        vec![
            Split::new(card, money(-3_000)),
            Split::new(base.groceries, money(3_000)),
        ],
    );
    // 92.50 EUR out of checking became 100.00 USD in the dollar account.
    let mut changed = Split::new(base.dollars, money(9_250));
    changed.foreign = Some(ForeignAmount {
        amount: dollars(10_000),
        rate: None,
    });
    add(
        &mut book,
        date(2026, 10, 26),
        vec![Split::new(base.checking, money(-9_250)), changed],
    );
    // Dated 31 October, booked by the bank on 2 November.
    let mut late = Split::new(base.checking, money(-500));
    late.imported = Some(ImportedDetails {
        booking_date: Some(date(2026, 11, 2)),
        ..ImportedDetails::default()
    });
    add(
        &mut book,
        date(2026, 10, 31),
        vec![late, Split::new(base.groceries, money(500))],
    );
    book.add_rate(&rate(date(2026, 10, 1), USD, EUR, "0.92"))
        .unwrap();
    book.add_rate(&rate(date(2026, 10, 28), EUR, USD, "1.1"))
        .unwrap();
    Financial {
        book,
        checking: base.checking,
        savings: base.savings,
        card,
    }
}

fn july_to_november() -> Period {
    period(date(2026, 7, 1), date(2026, 11, 30))
}

fn only(accounts: &[AccountId]) -> Filter {
    Filter {
        accounts: Some(accounts.to_vec()),
        categories: None,
    }
}

#[test]
fn each_balance_is_the_stores_balance_on_that_day() {
    let f = financial();
    let source = Source::read_with_balances(&f.book, july_to_november()).unwrap();
    for last in [
        date(2026, 7, 31),
        date(2026, 8, 9),
        date(2026, 9, 30),
        date(2026, 10, 31),
        date(2026, 11, 2),
    ] {
        let balances = account_balances(
            &source,
            period(date(2026, 7, 1), last),
            &Filter::default(),
            None,
        )
        .unwrap();
        assert_eq!(balances.day, last);
        for result in &balances.results {
            let mut net = Money::zero(result.currency);
            for row in &result.rows {
                assert_eq!(
                    row.balance,
                    f.book.balance_on(row.account, last).unwrap(),
                    "{} on {last}",
                    row.name
                );
                net = net.checked_add(row.balance).unwrap();
            }
            assert_eq!(result.worth.net_worth, net);
            assert_eq!(
                result.worth.assets.checked_sub(result.worth.liabilities),
                Ok(net)
            );
        }
    }
}

#[test]
fn accounts_count_by_kind_and_a_closed_empty_one_is_left_out() {
    let f = financial();
    let source = Source::read_with_balances(&f.book, july_to_november()).unwrap();
    let names = |last| {
        let balances = account_balances(
            &source,
            period(date(2026, 7, 1), last),
            &Filter::default(),
            None,
        )
        .unwrap();
        balances
            .results
            .iter()
            .map(|result| {
                let rows: Vec<(String, bool, i64)> = result
                    .rows
                    .iter()
                    .map(|row| (row.name.clone(), row.owed, row.balance.minor_units()))
                    .collect();
                (result.currency, rows, result.worth)
            })
            .collect::<Vec<_>>()
    };
    // While it still held money, the closed account is listed.
    let august = names(date(2026, 8, 9));
    assert!(august[0].1.iter().any(|(name, _, _)| name == "Old savings"));
    let september = names(date(2026, 9, 30));
    let (currency, rows, worth) = &september[0];
    assert_eq!(*currency, EUR);
    assert_eq!(
        rows.iter()
            .map(|(name, ..)| name.as_str())
            .collect::<Vec<_>>(),
        ["Checking account", "Savings", "Credit card"]
    );
    // The card's debt is negative as booked and positive as owed.
    assert_eq!(rows[2], ("Credit card".into(), true, -20_000));
    assert_eq!(worth.liabilities, money(20_000));
    assert_eq!(
        worth.net_worth,
        worth.assets.checked_sub(money(20_000)).unwrap()
    );
    // The dollar account is its own result, overdrawn by the trip until
    // the money was changed.
    let october = names(date(2026, 10, 25));
    assert_eq!(october[1].0, USD);
    assert_eq!(october[1].2.assets, dollars(-9_900));
}

#[test]
fn the_day_the_bank_booked_counts_not_the_date() {
    let f = financial();
    let source = Source::read_with_balances(&f.book, july_to_november()).unwrap();
    let on = |last| {
        account_balances(
            &source,
            period(date(2026, 10, 1), last),
            &only(&[f.checking]),
            None,
        )
        .unwrap()
        .results[0]
            .rows[0]
            .balance
    };
    let difference = on(date(2026, 11, 2)).checked_sub(on(date(2026, 10, 31)));
    assert_eq!(difference, Ok(money(-500)));
    assert_eq!(on(date(2026, 11, 1)), on(date(2026, 10, 31)));
}

#[test]
fn net_worth_over_time_is_the_balances_at_each_months_end() {
    let f = financial();
    let whole = july_to_november();
    let source = Source::read_with_balances(&f.book, whole).unwrap();
    let over_time = net_worth_over_time(&source, whole, &Filter::default(), None).unwrap();
    assert!(over_time.converted.is_empty());
    let currencies: Vec<CurrencyCode> = over_time.results.iter().map(|r| r.currency).collect();
    assert_eq!(currencies, [EUR, USD]);
    for result in &over_time.results {
        assert_eq!(result.rows.len(), 5);
        for row in &result.rows {
            assert_eq!(row.day, row.month.last_of_month());
            let balances = account_balances(
                &source,
                period(whole.first(), row.day),
                &Filter::default(),
                None,
            )
            .unwrap();
            let same = balances
                .results
                .iter()
                .find(|r| r.currency == result.currency)
                .map_or(
                    Worth {
                        currency: result.currency,
                        assets: Money::zero(result.currency),
                        liabilities: Money::zero(result.currency),
                        net_worth: Money::zero(result.currency),
                    },
                    |r| r.worth,
                );
            assert_eq!(row.worth, same, "{}", row.day);
        }
    }
    // A period that ends inside a month ends its last row on that day.
    let cut = period(date(2026, 9, 1), date(2026, 10, 15));
    let short = net_worth_over_time(&source, cut, &Filter::default(), None).unwrap();
    assert_eq!(short.results[0].rows[1].day, date(2026, 10, 15));
    let table = short.results[0].to_text(&Words);
    assert_eq!(
        table.columns,
        ["month", "assets", "liabilities", "net_worth"]
    );
    assert_eq!(table.rows[0][0], "2026-09");
}

#[test]
fn one_currency_converts_with_the_stores_rule_and_never_guesses() {
    let f = financial();
    let whole = july_to_november();
    let source = Source::read_with_balances(&f.book, whole).unwrap();
    // 26 October: the dollar-to-euro rate of 1 October is multiplied by.
    // 28 October and later: the newer euro-to-dollar rate is divided by.
    for last in [date(2026, 10, 26), date(2026, 10, 28), date(2026, 11, 30)] {
        let balances = account_balances(
            &source,
            period(whole.first(), last),
            &Filter::default(),
            Some(EUR),
        )
        .unwrap();
        let converted = balances.converted.expect("asked for euros");
        let dollar = balances
            .results
            .iter()
            .find(|r| r.currency == USD)
            .unwrap()
            .worth;
        assert!(!dollar.assets.is_zero());
        let (assets, used) = f.book.convert(dollar.assets, EUR, last).unwrap();
        let euro = balances.results[0].worth;
        assert_eq!(converted.day, last);
        assert_eq!(converted.worth.currency, EUR);
        assert_eq!(
            converted.worth.assets,
            euro.assets.checked_add(assets).unwrap(),
            "{last}"
        );
        assert_eq!(converted.worth.liabilities, euro.liabilities);
        assert_eq!(
            converted.worth.net_worth,
            converted
                .worth
                .assets
                .checked_sub(converted.worth.liabilities)
                .unwrap()
        );
        assert_eq!(converted.rates, [used]);
        assert!(converted.left_out.is_empty());
    }
    // Each month converts on its own last day.
    let months = net_worth_over_time(
        &source,
        period(date(2026, 9, 1), date(2026, 10, 31)),
        &Filter::default(),
        Some(EUR),
    )
    .unwrap();
    let [september, october] = months.converted.as_slice() else {
        panic!("two months");
    };
    // Nothing in dollars in September: nothing to convert or leave out.
    assert!(september.rates.is_empty() && september.left_out.is_empty());
    assert_eq!(
        september.worth, months.results[0].rows[0].worth,
        "the euros alone"
    );
    assert_eq!(october.day, date(2026, 10, 31));
    assert_eq!(october.rates.len(), 1);
    // In dollars on 30 September there is no rate yet: the euros are named
    // with their own figures and left out, never guessed.
    let before = account_balances(
        &source,
        period(date(2026, 9, 1), date(2026, 9, 30)),
        &Filter::default(),
        Some(USD),
    )
    .unwrap();
    let converted = before.converted.unwrap();
    assert_eq!(before.results[0].currency, EUR);
    assert_eq!(converted.worth.assets, dollars(0));
    assert_eq!(converted.left_out, [before.results[0].worth]);
    assert!(converted.rates.is_empty());
}

#[test]
fn cash_flow_adds_up_and_moves_between_chosen_accounts_count_as_nothing() {
    let f = financial();
    let whole = july_to_november();
    let source = Source::read_with_balances(&f.book, whole).unwrap();
    let flows = cash_flow_by_month(&source, whole, &Filter::default()).unwrap();
    let currencies: Vec<CurrencyCode> = flows.iter().map(|r| r.currency).collect();
    assert_eq!(currencies, [EUR, USD]);
    for result in &flows {
        for row in result.rows.iter().chain([&result.total]) {
            let end = row
                .start
                .checked_add(row.money_in)
                .and_then(|m| m.checked_sub(row.money_out))
                .and_then(|m| m.checked_add(row.starting));
            assert_eq!(end, Ok(row.end), "{} {}", result.currency, row.month);
            assert_eq!(row.money_in.checked_sub(row.money_out), Ok(row.net_change));
        }
    }
    let euros = &flows[0];
    let months: Vec<(i64, i64, i64, i64)> = euros
        .rows
        .iter()
        .map(|row| {
            (
                row.money_in.minor_units(),
                row.money_out.minor_units(),
                row.starting.minor_units(),
                row.end.minor_units(),
            )
        })
        .collect();
    // July: the two starting balances. August: the old account emptied into
    // checking counts as nothing. October: the Transfer to savings counts as
    // nothing, paying the card (not chosen) and changing money into dollars
    // are money out, and the purchase the bank booked in November is not
    // there.
    assert_eq!(months[0], (0, 0, 140_000, 140_000));
    assert_eq!(months[1].0, 250_000);
    assert_eq!(months[1].1, 80_000 + 5_320);
    assert_eq!(months[3].0, 250_000 + 700);
    assert_eq!(months[3].1, 80_000 + 1_250 + 20_000 + 9_250);
    assert_eq!(months[4].1, 500);
    // The dollars: the trip out, the changed money in.
    let october = &flows[1].rows[3];
    assert_eq!(
        (october.money_in, october.money_out),
        (dollars(10_000), dollars(9_900))
    );
    // The cash flow's end is what the accounts hold.
    let balances = account_balances(&source, whole, &Filter::default(), None).unwrap();
    assert_eq!(euros.total.end, balances.results[0].worth.assets);
    let table = euros.to_text(&Words);
    assert_eq!(
        table.columns,
        ["month", "start", "money_in", "money_out", "starting", "end"]
    );
    assert_eq!(table.rows.len(), 5);
}

#[test]
fn a_chosen_account_sees_the_transfer_and_the_card_its_own_payments() {
    let f = financial();
    let october = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_balances(&f.book, october).unwrap();
    let checking = cash_flow_by_month(&source, october, &only(&[f.checking])).unwrap();
    let row = checking[0].rows[0];
    assert_eq!(
        row.money_out.minor_units(),
        80_000 + 1_250 + 30_000 + 20_000 + 9_250
    );
    // The card: paid off (in) and bought on (out), a debt at the start.
    let card = cash_flow_by_month(&source, october, &only(&[f.card])).unwrap();
    let row = card[0].rows[0];
    assert_eq!(
        (row.start, row.money_in, row.money_out, row.end),
        (money(-20_000), money(20_000), money(3_000), money(-3_000))
    );
    // Checking and the card together: the payment counts as nothing.
    let both = cash_flow_by_month(&source, october, &only(&[f.checking, f.card])).unwrap();
    assert_eq!(
        both[0].rows[0].money_out.minor_units(),
        80_000 + 1_250 + 30_000 + 9_250 + 3_000
    );
    // A list that chooses nothing gives nothing.
    assert!(
        cash_flow_by_month(&source, october, &only(&[]))
            .unwrap()
            .is_empty()
    );
    // Categories are not accounts: choosing only one gives nothing.
    let categories = Filter {
        accounts: None,
        categories: Some(vec![f.savings]),
    };
    assert_eq!(
        cash_flow_by_month(&source, october, &categories).unwrap(),
        cash_flow_by_month(&source, october, &Filter::default()).unwrap()
    );
}

#[test]
fn a_source_read_for_too_few_days_refuses_the_balances() {
    let f = financial();
    let october = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_balances(&f.book, october).unwrap();
    let autumn = period(date(2026, 9, 1), date(2026, 10, 31));
    assert!(matches!(
        account_balances(&source, autumn, &Filter::default(), None),
        Err(ReportError::PeriodNotRead)
    ));
    assert!(matches!(
        net_worth_over_time(&source, autumn, &Filter::default(), None),
        Err(ReportError::PeriodNotRead)
    ));
    assert!(matches!(
        cash_flow_by_month(&source, autumn, &Filter::default()),
        Err(ReportError::PeriodNotRead)
    ));
}

#[test]
fn a_source_read_without_balances_refuses_them_instead_of_showing_zero() {
    let f = financial();
    let october = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read(&f.book, october).unwrap();
    assert!(matches!(
        account_balances(&source, october, &Filter::default(), None),
        Err(ReportError::PeriodNotRead)
    ));
    assert!(matches!(
        net_worth_over_time(&source, october, &Filter::default(), None),
        Err(ReportError::PeriodNotRead)
    ));
    assert!(matches!(
        cash_flow_by_month(&source, october, &Filter::default()),
        Err(ReportError::PeriodNotRead)
    ));
    // The other datasets read the same either way.
    let with = Source::read_with_balances(&f.book, october).unwrap();
    assert_eq!(
        income_expense_by_month(&with, october, &Filter::default()).unwrap(),
        income_expense_by_month(&source, october, &Filter::default()).unwrap()
    );
}

#[test]
fn a_key_figure_offers_the_figures_of_its_dataset() {
    let measure = ComponentType::KpiCard
        .properties()
        .iter()
        .find(|property| property.key == "measure")
        .unwrap();
    let words = |id| {
        let info = dataset(id).unwrap();
        measure.words(info).to_vec()
    };
    // Unchanged for the monthly figures; the default stays the first.
    assert_eq!(
        words(INCOME_EXPENSE_BY_MONTH),
        ["income", "expenses", "net", "savings-rate", FORMULA_MEASURE]
    );
    assert_eq!(
        words(ACCOUNT_BALANCES),
        ["net_worth", "assets", "liabilities", FORMULA_MEASURE]
    );
    let info = *dataset(ACCOUNT_BALANCES).unwrap();
    assert_eq!(
        measure.default_setting(&info),
        Setting::Word("net_worth".into())
    );
    assert!(
        measure
            .check(&info, &Setting::Word("assets".into()))
            .is_ok()
    );
    assert!(
        measure
            .check(&info, &Setting::Word("income".into()))
            .is_err()
    );
    // Every component the financial datasets suit exists.
    for id in [ACCOUNT_BALANCES, NET_WORTH_OVER_TIME, CASH_FLOW_BY_MONTH] {
        let info = dataset(id).unwrap();
        for suit in info.suits {
            assert!(
                ComponentType::ALL.iter().any(|kind| kind.suit() == *suit),
                "{id} {suit}"
            );
        }
    }
    // A progress bar is not offered what it cannot show (D-370).
    assert!(
        !ComponentType::ProgressBar
            .datasets()
            .any(|info| info.id == ACCOUNT_BALANCES)
    );
}

#[test]
fn a_figure_that_does_not_fit_is_an_error_not_a_wrong_figure() {
    let mut accounts = own_accounts();
    accounts.truncate(2);
    let covers = period(date(2026, 1, 1), date(2026, 1, 31));
    let opening: HashMap<AccountId, Money> = accounts
        .iter()
        .map(|a| (a.id, Money::from_minor_units(i64::MAX - 1, a.currency)))
        .collect();
    let source = Source::from_parts(covers, accounts, Vec::new(), Vec::new())
        .with_holdings(Holdings::new(opening, Vec::new(), Vec::new()));
    assert!(matches!(
        account_balances(&source, covers, &Filter::default(), None),
        Err(ReportError::Store(_))
    ));
}

/// Two euro accounts you have, a euro credit card and a dollar account.
fn own_accounts() -> Vec<Account> {
    [
        ("A", AccountKind::Asset, EUR),
        ("B", AccountKind::Asset, EUR),
        ("C", AccountKind::Liability, EUR),
        ("D", AccountKind::Asset, USD),
    ]
    .into_iter()
    .map(|(name, kind, currency)| Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency,
        account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .collect()
}

/// Parts of up to 12 transactions on the days of the first four months of
/// 2026: (day of the year 0..120, account, transaction, amount). Transaction
/// 0 sets starting balances.
fn movements() -> impl Strategy<Value = Vec<(i64, usize, usize, i64)>> {
    prop::collection::vec(
        (0i64..120, 0usize..4, 0usize..12, -1_000_000i64..1_000_000),
        0..60,
    )
}

fn holdings_source(
    accounts: &[Account],
    opening: &[i64],
    entries: &[(i64, usize, usize, i64)],
) -> (Source, Vec<Movement>) {
    let first = date(2026, 1, 1);
    let covers = period(first, date(2026, 4, 30));
    let transactions: Vec<TransactionId> = (0..12).map(|_| TransactionId::generate()).collect();
    let movements: Vec<Movement> = entries
        .iter()
        .map(|&(day, account, transaction, amount)| Movement {
            account: accounts[account].id,
            transaction: transactions[transaction],
            day: first.checked_add(jiff::Span::new().days(day)).unwrap(),
            amount: Money::from_minor_units(amount, accounts[account].currency),
            starting: transaction == 0,
        })
        .collect();
    let opening: HashMap<AccountId, Money> = accounts
        .iter()
        .zip(opening)
        .map(|(a, &units)| (a.id, Money::from_minor_units(units, a.currency)))
        .collect();
    let source = Source::from_parts(covers, accounts.to_vec(), Vec::new(), Vec::new())
        .with_holdings(Holdings::new(opening, movements.clone(), Vec::new()));
    (source, movements)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// For every month and currency, start + in − out + starting = end, the
    /// months chain, and the end is what the balances say.
    #[test]
    fn cash_flow_always_adds_up(
        entries in movements(),
        opening in prop::collection::vec(-1_000_000i64..1_000_000, 4),
        from in 0i64..60,
        length in 0i64..60,
        everything in any::<bool>(),
    ) {
        let accounts = own_accounts();
        let (source, _) = holdings_source(&accounts, &opening, &entries);
        let start = date(2026, 1, 1).checked_add(jiff::Span::new().days(from)).unwrap();
        let end = start.checked_add(jiff::Span::new().days(length)).unwrap();
        let shown = period(start, end);
        let filter = if everything {
            only(&accounts.iter().map(|a| a.id).collect::<Vec<_>>())
        } else {
            Filter::default()
        };
        let flows = cash_flow_by_month(&source, shown, &filter).unwrap();
        let worth = net_worth_over_time(&source, shown, &filter, None).unwrap();
        for result in &flows {
            for row in result.rows.iter().chain([&result.total]) {
                let added = row.start.minor_units() + row.money_in.minor_units()
                    - row.money_out.minor_units() + row.starting.minor_units();
                prop_assert_eq!(added, row.end.minor_units());
                prop_assert!(!row.money_in.is_negative() && !row.money_out.is_negative());
            }
            for pair in result.rows.windows(2) {
                prop_assert_eq!(pair[0].end, pair[1].start);
            }
            // With every account chosen, the end is what you are worth;
            // without a choice, what you have.
            let months = &worth
                .results
                .iter()
                .find(|r| r.currency == result.currency)
                .unwrap()
                .rows;
            for (row, month) in result.rows.iter().zip(months) {
                let expected = if everything {
                    month.worth.net_worth
                } else {
                    month.worth.assets
                };
                prop_assert_eq!(row.end, expected);
            }
        }
    }

    /// Each balance is the opening plus every part up to the day, the rows
    /// add up to the currency's figures, and the sums per currency never
    /// mix currencies.
    #[test]
    fn balances_are_the_parts_added_up(
        entries in movements(),
        opening in prop::collection::vec(-1_000_000i64..1_000_000, 4),
        length in 0i64..120,
    ) {
        let accounts = own_accounts();
        let (source, movements) = holdings_source(&accounts, &opening, &entries);
        let first = date(2026, 1, 1);
        let last = first.checked_add(jiff::Span::new().days(length)).unwrap();
        let balances = account_balances(&source, period(first, last), &Filter::default(), None)
            .unwrap();
        let mut seen = 0;
        for result in &balances.results {
            let (mut assets, mut owed) = (0, 0);
            for row in &result.rows {
                let at = accounts.iter().position(|a| a.id == row.account).unwrap();
                let expected = opening[at]
                    + movements
                        .iter()
                        .filter(|m| m.account == row.account && m.day <= last)
                        .map(|m| m.amount.minor_units())
                        .sum::<i64>();
                prop_assert_eq!(row.balance.minor_units(), expected);
                prop_assert_eq!(row.balance.currency(), result.currency);
                if row.owed { owed -= expected } else { assets += expected }
                seen += 1;
            }
            prop_assert_eq!(result.worth.assets.minor_units(), assets);
            prop_assert_eq!(result.worth.liabilities.minor_units(), owed);
            prop_assert_eq!(result.worth.net_worth.minor_units(), assets - owed);
        }
        prop_assert_eq!(seen, accounts.len());
    }
}
