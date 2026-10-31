// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of how a budget counts (M6c.4, D-346, D-347, D-378 to D-380): on
//! the report tests' real book with budgets made by the store's own
//! functions (M6c.L), with a category inside another, a refund, a part in
//! another currency, hidden categories, a changed amount and the carried
//! chain; and properties on exact figures.

use kassensturz_core::{
    AccountId, AccountKind, BudgetEvery, BudgetId, Date, ForeignAmount, Money, RoundingMode, Split,
    Transaction, TransactionId,
};
use kassensturz_store::Book;
use proptest::prelude::*;

use super::*;
use crate::tests::{EUR, USD, Words, account, date, money, period, sample};

fn dollars(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, USD)
}

fn add(book: &mut Book, day: Date, splits: Vec<Split>) {
    let transaction =
        Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced");
    book.add_transaction(&transaction).expect("added");
}

fn ask(
    book: &Book,
    shown: Period,
    today: Date,
    grouping: BudgetGrouping,
    filter: &Filter,
) -> Vec<BudgetUtilization> {
    let source = Source::read_with_budgets(book, shown, today).expect("read");
    budget_utilization(&source, shown, today, grouping, filter).expect("budgets")
}

fn october() -> Period {
    period(date(2026, 10, 1), date(2026, 10, 31))
}

fn find(results: &[BudgetUtilization], budget: BudgetId) -> &BudgetUtilization {
    results
        .iter()
        .find(|result| result.budget == budget)
        .expect("the budget is shown")
}

/// The sample book's spending: groceries (inside Food) 53.20 on 31 August,
/// 40.00 on 1 September, Food itself 10.00 on 10 September, a groceries
/// refund of 15.00 on 12 September, rent 800.00 on the 3rd of August,
/// September and October, and 99.00 USD of travel on 25 October.
#[test]
fn a_monthly_budget_with_carry_over_counts_its_category_and_what_is_inside() {
    let mut base = sample();
    let food = base
        .book
        .add_budget(
            base.food,
            BudgetEvery::Month,
            date(2026, 8, 1),
            None,
            money(5_000),
            true,
        )
        .unwrap();
    let results = ask(
        &base.book,
        period(date(2026, 8, 1), date(2026, 10, 31)),
        date(2026, 12, 1),
        BudgetGrouping::Period,
        &Filter::default(),
    );
    let shown = find(&results, food);
    assert_eq!(shown.path, vec!["Food".to_owned()]);
    assert_eq!(shown.currency, EUR);
    let figures: Vec<_> = shown
        .periods
        .iter()
        .map(|row| {
            (
                row.first_day,
                row.planned.minor_units(),
                row.carried_in.minor_units(),
                row.available.minor_units(),
                row.spent.minor_units(),
                row.left.minor_units(),
                row.used.map(Percent::tenths),
            )
        })
        .collect();
    assert_eq!(
        figures,
        vec![
            // 53.20 of 50.00: 3.20 over, 106.4 %.
            (date(2026, 8, 1), 5_000, 0, 5_000, 5_320, -320, Some(1_064)),
            // 40 + 10 − 15 = 35.00 of 46.80 (the overspending is carried):
            // 74.786 % → 74.8 %.
            (
                date(2026, 9, 1),
                5_000,
                -320,
                4_680,
                3_500,
                1_180,
                Some(748)
            ),
            (date(2026, 10, 1), 5_000, 1_180, 6_180, 0, 6_180, Some(0)),
        ]
    );
    assert_eq!(shown.periods[0].over(), Some(money(320)));
    assert_eq!(shown.periods[0].notes(), vec![BudgetNote::Over(money(320))]);
    assert_eq!(
        shown.periods[1].notes(),
        vec![BudgetNote::CarriedIn(money(-320))]
    );
    assert_eq!(shown.periods[2].over(), None);

    // Without a grouping, the month of the day asked for: the shown
    // period's last day, as it stands then.
    let now = ask(
        &base.book,
        october(),
        date(2026, 12, 1),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let only = find(&now, food);
    assert_eq!(only.periods.len(), 1);
    assert_eq!(only.periods[0], shown.periods[2]);
    // Hiding a category changes no budget (Q-225).
    base.book
        .set_hidden_categories(&[base.groceries, base.food])
        .unwrap();
    let hidden = ask(
        &base.book,
        period(date(2026, 8, 1), date(2026, 10, 31)),
        date(2026, 12, 1),
        BudgetGrouping::Period,
        &Filter::default(),
    );
    assert_eq!(find(&hidden, food), shown);
}

#[test]
fn a_budget_inside_another_is_a_figure_of_its_own() {
    let mut base = sample();
    let food = base
        .book
        .add_budget(
            base.food,
            BudgetEvery::Quarter,
            date(2026, 7, 1),
            None,
            money(20_000),
            false,
        )
        .unwrap();
    let groceries = base
        .book
        .add_budget(
            base.groceries,
            BudgetEvery::Quarter,
            date(2026, 7, 1),
            None,
            money(10_000),
            false,
        )
        .unwrap();
    let rent = base
        .book
        .add_budget(
            base.rent,
            BudgetEvery::Year,
            date(2026, 1, 1),
            None,
            money(900_000),
            false,
        )
        .unwrap();
    let results = ask(
        &base.book,
        period(date(2026, 9, 1), date(2026, 9, 30)),
        date(2026, 12, 1),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    // In the order of the category tree.
    let order: Vec<BudgetId> = results.iter().map(|result| result.budget).collect();
    assert_eq!(order, vec![food, groceries, rent]);
    let quarter = (date(2026, 7, 1), date(2026, 9, 30));
    let food_now = &find(&results, food).periods[0];
    assert_eq!((food_now.first_day, food_now.last_day), quarter);
    // 53.20 + 40.00 + 10.00 − 15.00, Food with Groceries inside it.
    assert_eq!(food_now.spent, money(8_820));
    let groceries_now = &find(&results, groceries).periods[0];
    // 53.20 + 40.00 − 15.00: the refund reduces it.
    assert_eq!(groceries_now.spent, money(7_820));
    assert_eq!(groceries_now.used, Some(Percent::from_tenths(782)));
    // A year asked for on 30 September: rent of August and September.
    let rent_now = &find(&results, rent).periods[0];
    assert_eq!(
        (rent_now.first_day, rent_now.last_day, rent_now.counted_to),
        (date(2026, 1, 1), date(2026, 12, 31), date(2026, 9, 30))
    );
    assert_eq!(rent_now.spent, money(160_000));
    // 1,600 of 9,000: 17.777… % → 17.8 %.
    assert_eq!(rent_now.used, Some(Percent::from_tenths(178)));

    // The chosen categories keep their budgets and those inside them.
    let chosen = ask(
        &base.book,
        period(date(2026, 9, 1), date(2026, 9, 30)),
        date(2026, 12, 1),
        BudgetGrouping::Budget,
        &Filter {
            accounts: None,
            categories: Some(vec![base.food]),
        },
    );
    let order: Vec<BudgetId> = chosen.iter().map(|result| result.budget).collect();
    assert_eq!(order, vec![food, groceries]);
    // An accounts filter doesn't narrow a budget's spending.
    let accounts = ask(
        &base.book,
        period(date(2026, 9, 1), date(2026, 9, 30)),
        date(2026, 12, 1),
        BudgetGrouping::Budget,
        &Filter {
            accounts: Some(vec![base.savings]),
            categories: None,
        },
    );
    assert_eq!(accounts, results);
}

#[test]
fn spending_is_counted_in_the_categorys_currency_as_booked() {
    let mut base = sample();
    let trip = base
        .book
        .add_budget(
            base.travel,
            BudgetEvery::Range,
            date(2026, 10, 20),
            Some(date(2026, 10, 30)),
            dollars(25_000),
            false,
        )
        .unwrap();
    // 92.50 EUR from checking were 100.00 USD of travel: the budget counts
    // the dollars it was booked with, never converted again.
    let mut abroad = Split::new(base.travel, money(9_250));
    abroad.foreign = Some(ForeignAmount {
        amount: dollars(10_000),
        rate: None,
    });
    add(
        &mut base.book,
        date(2026, 10, 27),
        vec![Split::new(base.checking, money(-9_250)), abroad],
    );
    let results = ask(
        &base.book,
        october(),
        date(2026, 10, 28),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let shown = find(&results, trip);
    assert_eq!(shown.currency, USD);
    let row = &shown.periods[0];
    assert_eq!(
        (row.first_day, row.last_day, row.counted_to),
        (date(2026, 10, 20), date(2026, 10, 30), date(2026, 10, 28))
    );
    // 99.00 + 100.00 of 250.00.
    assert_eq!(row.spent, dollars(19_900));
    assert_eq!(row.left, dollars(5_100));
    assert_eq!(row.used, Some(Percent::from_tenths(796)));
    assert!(!row.carries_over);
    // After its last day a range is over and not shown; grouped by period
    // it still is.
    let later = date(2026, 11, 2);
    assert!(
        ask(
            &base.book,
            october(),
            later,
            BudgetGrouping::Budget,
            &Filter::default()
        )
        .is_empty()
    );
    let by_period = ask(
        &base.book,
        october(),
        later,
        BudgetGrouping::Period,
        &Filter::default(),
    );
    assert_eq!(find(&by_period, trip).periods.len(), 1);
}

#[test]
fn the_share_used_is_rounded_half_away_from_zero_whatever_the_book_says() {
    let mut base = sample();
    let pets = account(&mut base.book, "Pets", AccountKind::Expense, EUR, None);
    let budget = base
        .book
        .add_budget(
            pets,
            BudgetEvery::Month,
            date(2026, 11, 1),
            None,
            money(1_600),
            false,
        )
        .unwrap();
    // 1.00 of 16.00 is 6.25 %, exactly halfway between 6.2 % and 6.3 %:
    // half away from zero gives 6.3 % in every rounding mode of the book.
    add(
        &mut base.book,
        date(2026, 11, 5),
        vec![
            Split::new(base.checking, money(-100)),
            Split::new(pets, money(100)),
        ],
    );
    let november = period(date(2026, 11, 1), date(2026, 11, 30));
    for rounding in [
        RoundingMode::HalfEven,
        RoundingMode::TowardZero,
        RoundingMode::HalfAwayFromZero,
    ] {
        base.book.set_rounding_mode(rounding).unwrap();
        let shown = ask(
            &base.book,
            november,
            date(2026, 11, 30),
            BudgetGrouping::Budget,
            &Filter::default(),
        );
        assert_eq!(
            find(&shown, budget).periods[0].used,
            Some(Percent::from_tenths(63)),
            "{rounding:?}"
        );
    }
    // A refund of 2.00 makes the spending −1.00: −6.25 % → −6.3 %.
    add(
        &mut base.book,
        date(2026, 11, 6),
        vec![
            Split::new(base.checking, money(200)),
            Split::new(pets, money(-200)),
        ],
    );
    let shown = ask(
        &base.book,
        november,
        date(2026, 11, 30),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let row = &find(&shown, budget).periods[0];
    assert_eq!(row.spent, money(-100));
    assert_eq!(row.used, Some(Percent::from_tenths(-63)));
    assert_eq!(row.left, money(1_700));
}

#[test]
fn nothing_available_has_no_share_and_says_so() {
    let mut base = sample();
    let budget = base
        .book
        .add_budget(
            base.groceries,
            BudgetEvery::Month,
            date(2026, 8, 1),
            None,
            money(1_000),
            true,
        )
        .unwrap();
    let results = ask(
        &base.book,
        period(date(2026, 8, 1), date(2026, 9, 30)),
        date(2026, 9, 30),
        BudgetGrouping::Period,
        &Filter::default(),
    );
    let rows = &find(&results, budget).periods;
    // August: 53.20 of 10.00, 43.20 over, carried into September.
    assert_eq!(rows[0].left, money(-4_320));
    // September: 10.00 − 43.20 = −33.20 available; nothing to spend.
    assert_eq!(rows[1].available, money(-3_320));
    assert_eq!(rows[1].used, None);
    // 40.00 − 15.00 spent: 58.20 over.
    assert_eq!(rows[1].left, money(-5_820));
    assert_eq!(
        rows[1].notes(),
        vec![
            BudgetNote::Over(money(5_820)),
            BudgetNote::NothingToSpend,
            BudgetNote::CarriedIn(money(-4_320)),
        ]
    );
    assert_eq!(
        rows[1]
            .notes()
            .iter()
            .map(|note| note.id())
            .collect::<Vec<_>>(),
        vec!["over", "nothing-to-spend", "carried-in"]
    );
    let table = find(&results, budget).to_text(&Words);
    assert_eq!(table.currency, "EUR");
    assert_eq!(
        table.columns,
        vec![
            "period",
            "planned",
            "carried_in",
            "available",
            "spent",
            "left",
            "used"
        ]
    );
    assert_eq!(
        table.rows,
        vec![
            vec![
                "2026-08",
                "1000 EUR",
                "0 EUR",
                "1000 EUR",
                "5320 EUR",
                "-4320 EUR",
                "532.0 %"
            ],
            vec![
                "2026-09",
                "1000 EUR",
                "-4320 EUR",
                "-3320 EUR",
                "2500 EUR",
                "-5820 EUR",
                ""
            ],
        ]
    );
}

#[test]
fn a_changed_amount_and_the_switch_apply_from_one_period_on() {
    let mut base = sample();
    let budget = base
        .book
        .add_budget(
            base.food,
            BudgetEvery::Month,
            date(2026, 8, 1),
            None,
            money(5_000),
            true,
        )
        .unwrap();
    let shown = period(date(2026, 8, 1), date(2026, 10, 31));
    let before = ask(
        &base.book,
        shown,
        date(2026, 10, 31),
        BudgetGrouping::Period,
        &Filter::default(),
    );
    base.book
        .edit_budget_amount(budget, date(2026, 9, 1), money(8_000), false)
        .unwrap();
    let after = ask(
        &base.book,
        shown,
        date(2026, 10, 31),
        BudgetGrouping::Period,
        &Filter::default(),
    );
    let (before, after) = (
        &find(&before, budget).periods,
        &find(&after, budget).periods,
    );
    assert_eq!(before[0], after[0], "August keeps what it had");
    // September: 80.00 with August's −3.20 carried in; carry over is off
    // from September on, so October starts at zero.
    assert_eq!(after[1].available, money(7_680));
    assert!(!after[1].carries_over);
    assert_eq!(after[2].carried_in, money(0));
    assert_eq!(after[2].available, money(8_000));

    // Stopped after September: not shown in October, but its periods are.
    base.book
        .set_budget_last_day(budget, Some(date(2026, 9, 30)))
        .unwrap();
    assert!(
        ask(
            &base.book,
            october(),
            date(2026, 10, 31),
            BudgetGrouping::Budget,
            &Filter::default()
        )
        .is_empty()
    );
    let stopped = ask(
        &base.book,
        shown,
        date(2026, 10, 31),
        BudgetGrouping::Period,
        &Filter::default(),
    );
    assert_eq!(find(&stopped, budget).periods.len(), 2);
}

#[test]
fn only_periods_up_to_the_day_asked_for_are_shown() {
    let mut base = sample();
    let budget = base
        .book
        .add_budget(
            base.rent,
            BudgetEvery::Month,
            date(2026, 1, 1),
            None,
            money(80_000),
            false,
        )
        .unwrap();
    let later = base
        .book
        .add_budget(
            base.groceries,
            BudgetEvery::Month,
            date(2026, 11, 1),
            None,
            money(1_000),
            false,
        )
        .unwrap();
    let year = period(date(2026, 1, 1), date(2026, 12, 31));
    let results = ask(
        &base.book,
        year,
        date(2026, 10, 15),
        BudgetGrouping::Period,
        &Filter::default(),
    );
    // January to October; November and December have not begun, and the
    // budget that starts in November is not shown yet.
    assert!(results.iter().all(|result| result.budget != later));
    let rows = &find(&results, budget).periods;
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[9].first_day, date(2026, 10, 1));
    assert_eq!(rows[9].counted_to, date(2026, 10, 15));
    assert_eq!(rows[9].spent, money(80_000));
    assert_eq!(rows[0].spent, money(0));
}

#[test]
fn a_source_without_the_budgets_or_their_days_is_refused() {
    let mut base = sample();
    base.book
        .add_budget(
            base.food,
            BudgetEvery::Month,
            date(2026, 8, 1),
            None,
            money(5_000),
            true,
        )
        .unwrap();
    let plain = Source::read(&base.book, october()).unwrap();
    assert!(matches!(
        budget_utilization(
            &plain,
            october(),
            date(2026, 10, 31),
            BudgetGrouping::Budget,
            &Filter::default()
        ),
        Err(ReportError::PeriodNotRead)
    ));
    // Read from August, where the carried chain starts.
    let read = Source::read_with_budgets(&base.book, october(), date(2026, 10, 31)).unwrap();
    assert_eq!(read.covers().first(), date(2026, 8, 1));
    // Without the chain's first months the figures would be wrong, so a
    // `Source` with fewer days is refused.
    let short = Source::read(&base.book, october())
        .unwrap()
        .with_budgets(base.book.budgets().unwrap());
    assert!(matches!(
        budget_utilization(
            &short,
            october(),
            date(2026, 10, 31),
            BudgetGrouping::Budget,
            &Filter::default()
        ),
        Err(ReportError::PeriodNotRead)
    ));
}

#[test]
fn the_dataset_and_its_component_wait_for_the_step_that_draws_them() {
    let [info] = BUDGETS else {
        panic!("one budget dataset");
    };
    assert_eq!(info.id, BUDGET_UTILIZATION);
    assert!(!info.for_ai);
    assert!(dataset(BUDGET_UTILIZATION).is_none());
    let flat = info
        .description
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(flat.contains(
        "Money moved between your own accounts and starting balances are not counted, and \
         amounts in different currencies are never added together: there is one result per \
         currency."
    ));
    assert!(flat.contains("rounded half away from zero to one decimal place"));
    for advice in ["should", "too much", "save"] {
        assert!(!flat.to_lowercase().contains(advice), "{advice}");
    }
    for grouping in info.groupings {
        assert_eq!(
            BudgetGrouping::from_id(grouping).map(BudgetGrouping::id),
            Some(*grouping)
        );
    }
    assert_eq!(info.groupings[0], BudgetGrouping::Budget.id());

    let kind = ComponentType::BudgetProgress;
    assert_eq!(kind.id(), "budget.progress");
    assert!(info.suits.contains(&kind.suit()));
    assert!(!kind.shows_no_data());
    assert!(!ComponentType::ALL.contains(&kind));
    assert!(!ComponentType::LIBRARY.contains(&kind));
    assert_eq!(ComponentType::from_id("budget.progress"), None);
    let keys: Vec<&str> = kind
        .properties()
        .iter()
        .map(|property| property.key)
        .collect();
    assert_eq!(keys, vec!["categories", "title", "period"]);
    assert_eq!(kind.datasets().count(), 0);
}

/// One month of a generated budget: what it plans, whether it carries over,
/// and the spending booked in it (days 1 to 28, amounts in cents, refunds
/// negative).
/// What a generated month plans, whether it carries over, and its
/// spending as days and cents.
type Month = (i64, bool, Vec<(i8, i64)>);

fn months() -> impl Strategy<Value = Vec<Month>> {
    prop::collection::vec(
        (
            1i64..100_000,
            any::<bool>(),
            prop::collection::vec((1i8..=28, -5_000i64..50_000), 0..4),
        ),
        1..8,
    )
}

fn month_start(index: usize) -> Date {
    let months = i64::try_from(index).unwrap() + 2026 * 12;
    date(
        i16::try_from(months / 12).unwrap(),
        i8::try_from(months % 12 + 1).unwrap(),
        1,
    )
}

/// A book with one monthly budget on its own category, made by the store
/// with one amount row per month, and the generated spending.
fn generated(plan: &[Month]) -> (Book, BudgetId, AccountId) {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR, None);
    let category = account(&mut book, "Hobbies", AccountKind::Expense, EUR, None);
    let (first_amount, first_carry, _) = &plan[0];
    let budget = book
        .add_budget(
            category,
            BudgetEvery::Month,
            month_start(0),
            None,
            money(*first_amount),
            *first_carry,
        )
        .unwrap();
    for (index, (amount, carry, spending)) in plan.iter().enumerate() {
        if index > 0 {
            book.edit_budget_amount(budget, month_start(index), money(*amount), *carry)
                .unwrap();
        }
        let start = month_start(index);
        for &(day, cents) in spending {
            if cents == 0 {
                continue;
            }
            add(
                &mut book,
                date(start.year(), start.month(), day),
                vec![
                    Split::new(checking, money(-cents)),
                    Split::new(category, money(cents)),
                ],
            );
        }
    }
    (book, budget, category)
}

fn all_months(count: usize) -> Period {
    period(month_start(0), month_start(count - 1).last_of_month())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Left is available less spent in every period, available is planned
    /// plus carried in, and the carried chain is the step-by-step sum of
    /// the generated months.
    #[test]
    fn the_figures_add_up_and_the_chain_is_a_step_by_step_sum(plan in months()) {
        let (book, budget, _) = generated(&plan);
        let shown = all_months(plan.len());
        let results = ask(&book, shown, shown.last(), BudgetGrouping::Period, &Filter::default());
        let rows = &find(&results, budget).periods;
        prop_assert_eq!(rows.len(), plan.len());
        let mut carried = 0i64;
        for (row, (amount, carry, spending)) in rows.iter().zip(&plan) {
            let spent: i64 = spending.iter().map(|(_, cents)| cents).sum();
            prop_assert_eq!(row.planned.minor_units(), *amount);
            prop_assert_eq!(row.carried_in.minor_units(), carried);
            prop_assert_eq!(row.available, row.planned.checked_add(row.carried_in).unwrap());
            prop_assert_eq!(row.spent.minor_units(), spent);
            prop_assert_eq!(row.left, row.available.checked_sub(row.spent).unwrap());
            prop_assert_eq!(row.used.is_none(), row.available.minor_units() <= 0);
            prop_assert_eq!(row.carries_over, *carry);
            carried = if *carry { amount + carried - spent } else { 0 };
        }
        // Each period as it stands at its end is the same row.
        for (index, row) in rows.iter().enumerate() {
            let month = period(month_start(index), month_start(index).last_of_month());
            let one = ask(&book, month, month.last(), BudgetGrouping::Budget, &Filter::default());
            prop_assert_eq!(&find(&one, budget).periods[0], row);
        }
    }

    /// Changing the amount and the switch from one period on never changes
    /// an earlier period's figures.
    #[test]
    fn a_change_never_alters_an_earlier_period(
        plan in months(),
        from in 0usize..8,
        amount in 1i64..100_000,
        carry in any::<bool>(),
    ) {
        let from = from % plan.len();
        let (mut book, budget, _) = generated(&plan);
        let shown = all_months(plan.len());
        let before = ask(&book, shown, shown.last(), BudgetGrouping::Period, &Filter::default());
        book.edit_budget_amount(budget, month_start(from), money(amount), carry).unwrap();
        let after = ask(&book, shown, shown.last(), BudgetGrouping::Period, &Filter::default());
        let (before, after) = (&find(&before, budget).periods, &find(&after, budget).periods);
        prop_assert_eq!(&before[..from], &after[..from]);
        prop_assert_eq!(after[from].planned, money(amount));
    }
}
