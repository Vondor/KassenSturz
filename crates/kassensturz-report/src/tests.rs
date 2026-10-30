// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the datasets: one on a real book, with the Transfer that the
//! store's own detection finds, and properties on exact totals.

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Split, Transaction,
    TransactionId,
};
use kassensturz_store::{Book, CategoryEntry};
use proptest::prelude::*;

use super::*;

pub(crate) const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
pub(crate) const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

pub(crate) fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("valid date")
}

pub(crate) fn money(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

pub(crate) fn account(
    book: &mut Book,
    name: &str,
    kind: AccountKind,
    currency: CurrencyCode,
    parent: Option<AccountId>,
) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency,
        account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

/// A transaction of `parts`; a part without an account has no category yet.
fn transaction(day: Date, parts: &[(Option<AccountId>, Money)]) -> Transaction {
    let splits = parts
        .iter()
        .map(|&(account, amount)| match account {
            Some(account) => Split::new(account, amount),
            None => Split::uncategorized(amount),
        })
        .collect();
    Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced")
}

/// Words for the tests, so the output shows what the dataset gave.
pub(crate) struct Words;

impl TextFormat for Words {
    fn amount(&self, money: Money) -> String {
        format!("{} {}", money.minor_units(), money.currency())
    }

    fn month(&self, first_day: Date) -> String {
        format!("{}-{:02}", first_day.year(), first_day.month())
    }

    fn percent(&self, percent: Percent) -> String {
        format!(
            "{}.{} %",
            percent.tenths() / 10,
            (percent.tenths() % 10).abs()
        )
    }

    fn number(&self, hundredths: i64) -> String {
        format!("{hundredths} hundredths")
    }

    fn unsorted(&self) -> String {
        "(unsorted)".into()
    }
}

pub(crate) struct Sample {
    pub(crate) book: Book,
    pub(crate) checking: AccountId,
    pub(crate) savings: AccountId,
    pub(crate) dollars: AccountId,
    pub(crate) food: AccountId,
    pub(crate) groceries: AccountId,
    pub(crate) rent: AccountId,
    pub(crate) salary: AccountId,
    pub(crate) travel: AccountId,
}

/// A book with three months of everyday money, a starting balance, a refund,
/// money not sorted yet, a Transfer found by the store's own detection and
/// one account in another currency.
pub(crate) fn sample() -> Sample {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    book.use_currency(USD).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR, None);
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD, None);
    let opening = account(&mut book, "Opening", AccountKind::Equity, EUR, None);
    let food = account(&mut book, "Food", AccountKind::Expense, EUR, None);
    let groceries = account(
        &mut book,
        "Groceries",
        AccountKind::Expense,
        EUR,
        Some(food),
    );
    let rent = account(&mut book, "Rent", AccountKind::Expense, EUR, None);
    let travel = account(&mut book, "Travel", AccountKind::Expense, USD, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let add = |book: &mut Book, day, parts: &[(Option<AccountId>, Money)]| {
        book.add_transaction(&transaction(day, parts))
            .expect("transaction added");
    };
    let dollars_of = |minor_units| Money::from_minor_units(minor_units, USD);

    add(
        &mut book,
        date(2026, 7, 31),
        &[
            (Some(checking), money(100_000)),
            (Some(opening), money(-100_000)),
        ],
    );
    for (month, rent_day) in [(8, 3), (9, 3), (10, 3)] {
        add(
            &mut book,
            date(2026, month, 1),
            &[
                (Some(checking), money(250_000)),
                (Some(salary), money(-250_000)),
            ],
        );
        add(
            &mut book,
            date(2026, month, rent_day),
            &[
                (Some(checking), money(-80_000)),
                (Some(rent), money(80_000)),
            ],
        );
    }
    add(
        &mut book,
        date(2026, 8, 31),
        &[
            (Some(checking), money(-5_320)),
            (Some(groceries), money(5_320)),
        ],
    );
    add(
        &mut book,
        date(2026, 9, 1),
        &[
            (Some(checking), money(-4_000)),
            (Some(groceries), money(4_000)),
        ],
    );
    // Booked to the top-level category itself.
    add(
        &mut book,
        date(2026, 9, 10),
        &[(Some(checking), money(-1_000)), (Some(food), money(1_000))],
    );
    // A refund: money back in a spending category.
    add(
        &mut book,
        date(2026, 9, 12),
        &[
            (Some(checking), money(1_500)),
            (Some(groceries), money(-1_500)),
        ],
    );
    // Paid, but not sorted into a category yet; and received.
    add(
        &mut book,
        date(2026, 10, 13),
        &[(Some(checking), money(-1_250)), (None, money(1_250))],
    );
    add(
        &mut book,
        date(2026, 10, 14),
        &[(Some(checking), money(700)), (None, money(-700))],
    );
    // Both sides of a Transfer as two banks delivered them, found and joined
    // by the store, as the user confirms a suggestion.
    add(
        &mut book,
        date(2026, 10, 20),
        &[(Some(checking), money(-30_000)), (None, money(30_000))],
    );
    add(
        &mut book,
        date(2026, 10, 21),
        &[(Some(savings), money(30_000)), (None, money(-30_000))],
    );
    let suggestions = book.transfer_suggestions().expect("suggestions");
    let [found] = suggestions.as_slice() else {
        panic!("the one Transfer is found, not {suggestions:?}");
    };
    book.join_transfer(found.outgoing.transaction, found.incoming.transaction)
        .expect("transfer joined");
    // The other currency.
    add(
        &mut book,
        date(2026, 10, 25),
        &[
            (Some(dollars), dollars_of(-9_900)),
            (Some(travel), dollars_of(9_900)),
        ],
    );
    Sample {
        book,
        checking,
        savings,
        dollars,
        food,
        groceries,
        rent,
        salary,
        travel,
    }
}

pub(crate) fn period(first: Date, last: Date) -> Period {
    Period::new(first, last).expect("a period")
}

pub(crate) fn autumn() -> Period {
    period(date(2026, 8, 1), date(2026, 10, 31))
}

fn rows(result: &SpendingByCategory) -> Vec<(String, i64)> {
    result
        .rows
        .iter()
        .map(|row| {
            let name = if row.category.is_some() {
                row.path.join(" › ")
            } else {
                "-".into()
            };
            (name, row.total.minor_units())
        })
        .collect()
}

#[test]
fn spending_by_category_counts_what_went_out_and_nothing_else() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("source");
    let results =
        spending_by_category(&source, autumn(), &Filter::default(), Level::Each).expect("dataset");
    let [euros, dollars] = results.as_slice() else {
        panic!("one result per currency, not {results:?}");
    };
    assert_eq!((euros.currency, dollars.currency), (EUR, USD));
    // Largest first. The starting balance, the salary and the Transfer are
    // not spending; the refund reduces Groceries; the unsorted payment sits
    // among the categories by its size.
    assert_eq!(
        rows(euros),
        [
            ("Rent".to_string(), 240_000),
            ("Food › Groceries".to_string(), 7_820),
            ("-".to_string(), 1_250),
            ("Food".to_string(), 1_000),
        ]
    );
    assert_eq!(euros.total, money(250_070));
    assert_eq!(rows(dollars), [("Travel".to_string(), 9_900)]);
    assert_eq!(dollars.total, Money::from_minor_units(9_900, USD));
}

#[test]
fn top_level_adds_subcategories_to_their_top_level_category() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("source");
    let results = spending_by_category(&source, autumn(), &Filter::default(), Level::TopLevel)
        .expect("dataset");
    assert_eq!(
        rows(&results[0]),
        [
            ("Rent".to_string(), 240_000),
            ("Food".to_string(), 8_820),
            ("-".to_string(), 1_250),
        ]
    );
    assert_eq!(results[0].total, money(250_070));
    assert_eq!(results[0].rows[1].category, Some(sample.food));
}

#[test]
fn income_and_expenses_by_month_fill_every_month_of_the_period() {
    let sample = sample();
    let wide = period(date(2026, 7, 15), date(2026, 11, 20));
    let source = Source::read(&sample.book, wide).expect("source");
    let results = income_expense_by_month(&source, wide, &Filter::default()).expect("dataset");
    let [euros, dollars] = results.as_slice() else {
        panic!("one result per currency, not {results:?}");
    };
    let table = euros.to_text(&Words);
    assert_eq!(table.currency, "EUR");
    assert_eq!(table.columns, ["month", "income", "expenses", "net"]);
    // July's starting balance (booked on the 31st) is neither; November is
    // empty but listed. October: 2500,00 + 7,00 in; 800,00 + 12,50 out.
    assert_eq!(
        table.rows,
        [
            ["2026-07", "0 EUR", "0 EUR", "0 EUR"],
            ["2026-08", "250000 EUR", "85320 EUR", "164680 EUR"],
            ["2026-09", "250000 EUR", "83500 EUR", "166500 EUR"],
            ["2026-10", "250700 EUR", "81250 EUR", "169450 EUR"],
            ["2026-11", "0 EUR", "0 EUR", "0 EUR"],
        ]
    );
    assert_eq!(euros.income, money(750_700));
    assert_eq!(euros.expenses, money(250_070));
    assert_eq!(euros.net, money(500_630));
    assert_eq!(dollars.currency, USD);
    assert_eq!(
        dollars.rows[3].expenses,
        Money::from_minor_units(9_900, USD)
    );
    assert_eq!(dollars.rows[3].net, Money::from_minor_units(-9_900, USD));
}

#[test]
fn the_period_cuts_partial_months_at_its_days() {
    let sample = sample();
    let cut = period(date(2026, 9, 2), date(2026, 9, 30));
    let source = Source::read(&sample.book, cut).expect("source");
    let [euros] = income_expense_by_month(&source, cut, &Filter::default())
        .expect("dataset")
        .try_into()
        .expect("one currency");
    // Not the 1st: no salary, no 40,00 of groceries; the rent of the 3rd,
    // the 10,00 and the refund of the 12th are.
    assert_eq!(euros.rows.len(), 1);
    assert_eq!(euros.income, money(0));
    assert_eq!(euros.expenses, money(80_000 + 1_000 - 1_500));
}

#[test]
fn the_account_filter_keeps_whole_transactions_of_the_chosen_accounts() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("source");
    let only = |accounts: Vec<AccountId>| Filter {
        accounts: Some(accounts),
        categories: None,
    };
    let dollars = spending_by_category(&source, autumn(), &only(vec![sample.dollars]), Level::Each)
        .expect("dataset");
    assert_eq!(dollars.len(), 1);
    assert_eq!(dollars[0].currency, USD);
    // Savings only took the Transfer: nothing is spent from there.
    let savings = spending_by_category(&source, autumn(), &only(vec![sample.savings]), Level::Each)
        .expect("dataset");
    assert!(savings.is_empty());
    let nothing =
        spending_by_category(&source, autumn(), &only(vec![]), Level::Each).expect("dataset");
    assert!(nothing.is_empty());
    let checking = spending_by_category(
        &source,
        autumn(),
        &only(vec![sample.checking, sample.savings]),
        Level::Each,
    )
    .expect("dataset");
    assert_eq!(checking.len(), 1);
    assert_eq!(checking[0].currency, EUR);
    assert_eq!(checking[0].total, money(250_070));
}

#[test]
fn the_category_filter_takes_subcategories_along_and_leaves_unsorted_out() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("source");
    let only = |categories: Vec<AccountId>| Filter {
        accounts: None,
        categories: Some(categories),
    };
    let food = spending_by_category(&source, autumn(), &only(vec![sample.food]), Level::Each)
        .expect("dataset");
    assert_eq!(
        rows(&food[0]),
        [
            ("Food › Groceries".to_string(), 7_820),
            ("Food".to_string(), 1_000)
        ]
    );
    let groceries = spending_by_category(
        &source,
        autumn(),
        &only(vec![sample.groceries]),
        Level::Each,
    )
    .expect("dataset");
    assert_eq!(groceries[0].total, money(7_820));
    // Income can be chosen too: salary is not spending, but it is income.
    let [salary] = income_expense_by_month(&source, autumn(), &only(vec![sample.salary]))
        .expect("dataset")
        .try_into()
        .expect("one currency");
    assert_eq!(salary.income, money(750_000));
    assert_eq!(salary.expenses, money(0));
    let rent = spending_by_category(
        &source,
        autumn(),
        &only(vec![sample.rent, sample.travel]),
        Level::Each,
    )
    .expect("dataset");
    assert_eq!(rent.len(), 2);
}

#[test]
fn a_book_without_money_in_the_period_gives_no_results() {
    let sample = sample();
    let before = period(date(2020, 1, 1), date(2020, 12, 31));
    let source = Source::read(&sample.book, before).expect("source");
    assert!(
        spending_by_category(&source, before, &Filter::default(), Level::Each)
            .expect("dataset")
            .is_empty()
    );
    assert!(
        income_expense_by_month(&source, before, &Filter::default())
            .expect("dataset")
            .is_empty()
    );
}

#[test]
fn a_total_that_does_not_fit_is_an_error_not_a_wrong_figure() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR, None);
    let rent = account(&mut book, "Rent", AccountKind::Expense, EUR, None);
    for day in [1, 2] {
        book.add_transaction(&transaction(
            date(2026, 10, day),
            &[
                (Some(checking), money(-i64::MAX)),
                (Some(rent), money(i64::MAX)),
            ],
        ))
        .expect("transaction added");
    }
    let october = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read(&book, october).expect("source");
    assert!(matches!(
        spending_by_category(&source, october, &Filter::default(), Level::Each),
        Err(ReportError::Store(_))
    ));
    assert!(matches!(
        income_expense_by_month(&source, october, &Filter::default()),
        Err(ReportError::Store(_))
    ));
}

#[test]
fn the_text_table_names_the_unsorted_row_in_the_users_words() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("source");
    let results =
        spending_by_category(&source, autumn(), &Filter::default(), Level::Each).expect("dataset");
    let table = results[0].to_text(&Words);
    assert_eq!(table.columns, ["category", "total"]);
    assert_eq!(table.rows[1], ["Food › Groceries", "7820 EUR"]);
    assert_eq!(table.rows[2], ["(unsorted)", "1250 EUR"]);
}

/// A tree of accounts without a book: two asset accounts, an income
/// category and a spending category with a subcategory.
pub(crate) struct Chart {
    pub(crate) accounts: Vec<Account>,
    pub(crate) tree: Vec<CategoryEntry>,
}

pub(crate) fn chart() -> Chart {
    let make = |name: &str, kind, parent| Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: EUR,
        account_type: None,
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    };
    let checking = make("Checking account", AccountKind::Asset, None);
    let savings = make("Savings", AccountKind::Asset, None);
    let salary = make("Salary", AccountKind::Income, None);
    let food = make("Food", AccountKind::Expense, None);
    let groceries = make("Groceries", AccountKind::Expense, Some(food.id));
    let tree = vec![
        CategoryEntry {
            path: vec!["Salary".into()],
            category: salary.clone(),
        },
        CategoryEntry {
            path: vec!["Food".into()],
            category: food.clone(),
        },
        CategoryEntry {
            path: vec!["Food".into(), "Groceries".into()],
            category: groceries.clone(),
        },
    ];
    Chart {
        accounts: vec![checking, savings, salary, food, groceries],
        tree,
    }
}

/// One random transaction: a day of 2026, which two accounts it books, and
/// an amount in minor units.
pub(crate) fn entries() -> impl Strategy<Value = Vec<(i8, i8, usize, usize, i64)>> {
    prop::collection::vec(
        (
            1i8..=12,
            1i8..=28,
            0usize..5,
            0usize..6,
            -5_000_000i64..5_000_000,
        ),
        0..40,
    )
}

pub(crate) fn build(chart: &Chart, entries: &[(i8, i8, usize, usize, i64)]) -> Vec<Transaction> {
    entries
        .iter()
        .filter(|&&(.., amount)| amount != 0)
        .map(|&(month, day, from, to, amount)| {
            // `to` 5 is "no category yet".
            let to = chart.accounts.get(to).map(|account| account.id);
            let from = chart.accounts[from].id;
            transaction(
                date(2026, month, day),
                &[(Some(from), money(amount)), (to, money(-amount))],
            )
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The three views of one period agree to the cent: the rows add up to
    /// the total, the top-level view has the same total, and the months add
    /// up to the same expenses, whatever order the transactions come in.
    #[test]
    fn the_totals_agree_in_every_view(entries in entries(), cut in 1i8..=12) {
        let chart = chart();
        let transactions = build(&chart, &entries);
        let whole = period(date(2026, 1, 1), date(2026, 12, 31));
        let source = Source::from_parts(
            whole,
            chart.accounts.clone(),
            chart.tree.clone(),
            transactions.clone(),
        );
        let each = spending_by_category(&source, whole, &Filter::default(), Level::Each).unwrap();
        let top = spending_by_category(&source, whole, &Filter::default(), Level::TopLevel).unwrap();
        let months = income_expense_by_month(&source, whole, &Filter::default()).unwrap();

        prop_assert_eq!(each.len() <= 1, true);
        let spent = each.first().map_or(0, |result| result.total.minor_units());
        for result in &each {
            let added: i64 = result.rows.iter().map(|row| row.total.minor_units()).sum();
            prop_assert_eq!(added, result.total.minor_units());
        }
        prop_assert_eq!(top.first().map_or(0, |result| result.total.minor_units()), spent);
        prop_assert_eq!(months.first().map_or(0, |result| result.expenses.minor_units()), spent);
        for result in &months {
            let net: i64 = result.rows.iter().map(|row| row.net.minor_units()).sum();
            prop_assert_eq!(net, result.net.minor_units());
            prop_assert_eq!(result.net.minor_units(), result.income.minor_units() - result.expenses.minor_units());
        }

        // Split the year in two: the halves add up to the whole.
        let cut_day = date(2026, cut, 1);
        if cut > 1 {
            let before = period(date(2026, 1, 1), cut_day.yesterday().unwrap());
            let after = period(cut_day, date(2026, 12, 31));
            let spent_in = |p: Period| -> i64 {
                spending_by_category(&source, p, &Filter::default(), Level::Each)
                    .unwrap()
                    .first()
                    .map_or(0, |result| result.total.minor_units())
            };
            prop_assert_eq!(spent_in(before) + spent_in(after), spent);
        }

        // The order of the transactions changes nothing.
        let mut reversed = transactions;
        reversed.reverse();
        let source = Source::from_parts(whole, chart.accounts.clone(), chart.tree.clone(), reversed);
        prop_assert_eq!(
            spending_by_category(&source, whole, &Filter::default(), Level::Each).unwrap(),
            each
        );
        prop_assert_eq!(
            income_expense_by_month(&source, whole, &Filter::default()).unwrap(),
            months
        );
    }
}

fn october() -> Period {
    period(date(2026, 10, 1), date(2026, 10, 31))
}

#[test]
fn october_against_september_on_the_sample_book() {
    let sample = sample();
    let span = comparison_span(october(), Compare::PreviousPeriod).expect("span");
    assert_eq!(span.first(), date(2026, 9, 1));
    let source = Source::read(&sample.book, span).expect("source");
    let (earlier, changes) = compare_income_expense(
        &source,
        october(),
        &Filter::default(),
        Compare::PreviousPeriod,
    )
    .expect("comparison");
    assert_eq!(earlier, period(date(2026, 9, 1), date(2026, 9, 30)));
    let [euros, dollars] = changes.as_slice() else {
        panic!("one result per currency, not {changes:?}");
    };
    // 250700 against 250000: 0,28 % rounds to 0,3 %.
    assert_eq!(euros.income.previous, money(250_000));
    assert_eq!(euros.income.current, money(250_700));
    assert_eq!(euros.income.amount, money(700));
    assert_eq!(euros.income.percent, Some(Percent::from_tenths(3)));
    // 81250 against 83500: down 2250, which is 2,7 %.
    assert_eq!(euros.expenses.amount, money(-2_250));
    assert_eq!(euros.expenses.percent, Some(Percent::from_tenths(-27)));
    assert_eq!(euros.net.amount, money(2_950));
    assert_eq!(euros.net.percent, Some(Percent::from_tenths(18)));
    // 166500 of 250000 is 66,6 %; 169450 of 250700 is 67,59 %.
    assert_eq!(euros.previous_savings_rate, Some(Percent::from_tenths(666)));
    assert_eq!(euros.savings_rate, Some(Percent::from_tenths(676)));
    assert_eq!(
        euros.savings_rate.map(|rate| rate.to_text(&Words)),
        Some("67.6 %".into())
    );
    // The dollars were spent only now: no percentage of nothing.
    assert_eq!(dollars.currency, USD);
    assert_eq!(dollars.expenses.previous, Money::from_minor_units(0, USD));
    assert_eq!(dollars.expenses.percent, None);
    assert_eq!(dollars.savings_rate, None);
    assert_eq!(dollars.previous_savings_rate, None);
}

#[test]
fn the_same_month_a_year_earlier_is_empty_and_has_no_percentages() {
    let sample = sample();
    let span = comparison_span(october(), Compare::SameLastYear).expect("span");
    let source = Source::read(&sample.book, span).expect("source");
    let (earlier, changes) = compare_income_expense(
        &source,
        october(),
        &Filter::default(),
        Compare::SameLastYear,
    )
    .expect("comparison");
    assert_eq!(earlier, period(date(2025, 10, 1), date(2025, 10, 31)));
    assert_eq!(changes[0].income.previous, money(0));
    assert_eq!(changes[0].income.percent, None);
    assert_eq!(changes[0].previous_savings_rate, None);
    assert_eq!(changes[0].net.amount, money(169_450));
}

#[test]
fn a_source_read_for_too_few_days_is_refused_not_silently_short() {
    let sample = sample();
    let source = Source::read(&sample.book, october()).expect("source");
    // September was never read, so October against September would show
    // September as empty.
    assert!(matches!(
        compare_income_expense(
            &source,
            october(),
            &Filter::default(),
            Compare::PreviousPeriod
        ),
        Err(ReportError::PeriodNotRead)
    ));
    assert!(matches!(
        spending_by_category(&source, autumn(), &Filter::default(), Level::Each),
        Err(ReportError::PeriodNotRead)
    ));
    assert!(spending_by_category(&source, october(), &Filter::default(), Level::Each).is_ok());
}

#[test]
fn the_savings_rate_rounds_half_away_from_zero_and_has_no_value_without_income() {
    let rate = |income, net| savings_rate(money(income), money(net)).expect("a rate");
    // 1 of 8 is 12,5 %; 1 of 16 is 6,25 %, which is 6,3 %: half away.
    assert_eq!(rate(800, 100), Some(Percent::from_tenths(125)));
    assert_eq!(rate(1_600, 100), Some(Percent::from_tenths(63)));
    assert_eq!(rate(1_600, -100), Some(Percent::from_tenths(-63)));
    // Spent more than earned: a negative rate, as it is.
    assert_eq!(rate(1_000, -500), Some(Percent::from_tenths(-500)));
    assert_eq!(rate(1_000, 1_000), Some(Percent::from_tenths(1_000)));
    assert_eq!(rate(0, 500), None);
    assert_eq!(rate(0, -500), None);
    assert_eq!(rate(-100, -500), None);
    // A change of a negative figure keeps the direction: from -100 to -50
    // is a rise of half its size.
    let change = Change::new(money(-100), money(-50)).expect("a change");
    assert_eq!(change.amount, money(50));
    assert_eq!(change.percent, Some(Percent::from_tenths(500)));
    // The extremes still give the exact answer or an error, never a wrong one.
    assert_eq!(
        savings_rate(money(i64::MAX), money(i64::MIN)).expect("a rate"),
        Some(Percent::from_tenths(-1_000))
    );
    assert!(matches!(
        Change::new(money(i64::MIN), money(0)),
        Err(ReportError::Store(_))
    ));
}

#[test]
fn the_comparison_names_round_trip_for_dashboards_and_the_ai() {
    for compare in [Compare::PreviousPeriod, Compare::SameLastYear] {
        assert_eq!(Compare::from_id(compare.id()), Some(compare));
    }
    assert_eq!(Compare::from_id("last_week"), None);
}

proptest! {
    /// The rate is the exact ratio rounded once: it is never further from
    /// the exact value than half a tenth of a percent, and it is the same
    /// as an independent integer calculation.
    #[test]
    fn the_rate_is_the_exact_ratio_rounded_once(income in 1i64..1_000_000_000, net in -1_000_000_000i64..1_000_000_000) {
        let tenths = savings_rate(money(income), money(net)).unwrap().unwrap().tenths();
        // Twice the exact value in tenths, compared without division.
        let exact_twice = i128::from(net) * 2000;
        let rounded_twice = i128::from(tenths) * 2 * i128::from(income);
        let off = (exact_twice - rounded_twice).abs();
        prop_assert!(off <= i128::from(income), "off by more than half a tenth");
    }
}
