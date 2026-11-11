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
    budget_utilization(&source, shown, today, grouping, filter, None).expect("budgets")
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
            &[base.food],
            &[],
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
    assert_eq!(shown.paths, vec![vec!["Food".to_owned()]]);
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
            &[base.food],
            &[],
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
            &[base.groceries],
            &[],
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
            &[base.rent],
            &[],
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
            &[base.travel],
            &[],
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
    // After its last day a range is over: it shows its one period,
    // complete and marked ended (D-710); grouped by period it is shown too.
    let later = date(2026, 11, 2);
    let over = ask(
        &base.book,
        october(),
        later,
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let ended = &find(&over, trip).periods;
    assert_eq!(ended.len(), 1);
    assert!(ended[0].ended && !ended[0].starts_later);
    assert_eq!(
        (ended[0].first_day, ended[0].last_day, ended[0].counted_to),
        (date(2026, 10, 20), date(2026, 10, 30), date(2026, 10, 30))
    );
    assert_eq!(ended[0].spent, dollars(19_900));
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
            &[pets],
            &[],
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
            &[base.groceries],
            &[],
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
            &[base.food],
            &[],
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

    // Stopped after September: in October it shows September, complete
    // and marked ended (D-710), with what August carried in; grouped by
    // period its periods are shown.
    base.book
        .set_budget_last_day(budget, Some(date(2026, 9, 30)))
        .unwrap();
    let october_view = ask(
        &base.book,
        october(),
        date(2026, 10, 31),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let last = &find(&october_view, budget).periods;
    assert_eq!(last.len(), 1);
    assert!(last[0].ended);
    assert_eq!(
        BudgetPeriod {
            ended: false,
            ..last[0]
        },
        after[1]
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
            &[base.rent],
            &[],
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
            &[base.groceries],
            &[],
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
            &[base.food],
            &[],
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
            &Filter::default(),
            None
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
            &Filter::default(),
            None
        ),
        Err(ReportError::PeriodNotRead)
    ));
}

#[test]
fn the_dataset_and_its_component_are_offered_since_the_app_draws_them() {
    let info = dataset(BUDGET_UTILIZATION).expect("the dataset is in the catalog (D-381)");
    assert_eq!(info.id, BUDGET_UTILIZATION);
    // The AI sees budgets only with the permission grid's Budgets row
    // (D-358, M6c.11).
    assert!(!info.for_ai);
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
    assert!(ComponentType::LIBRARY.contains(&kind));
    assert_eq!(ComponentType::from_id("budget.progress"), Some(kind));
    let keys: Vec<&str> = kind
        .properties()
        .iter()
        .map(|property| property.key)
        .collect();
    assert_eq!(
        keys,
        vec!["budgets", "categories", "title", "hide_note", "period"]
    );
    // Which budgets (D-642): budget IDs as text, empty for every budget.
    let budgets = &kind.properties()[0];
    assert_eq!(budgets.kind, Kind::Budgets);
    assert_eq!(budgets.default_setting(info), Setting::Ids(Vec::new()));
    let many = vec!["b".to_owned(); MAX_FILTER_IDS + 1];
    assert!(budgets.check(info, &Setting::Ids(many)).is_err());
    assert!(
        budgets
            .check(info, &Setting::Ids(vec!["b".to_owned()]))
            .is_ok()
    );
    let offered: Vec<&str> = kind.datasets().map(|dataset| dataset.id).collect();
    assert_eq!(offered, vec![BUDGET_UTILIZATION]);
    // Besides the bars only a table takes it: the same figures as rows
    // (D-666); no chart draws budgets. A custom chart's description can
    // name every dataset (M6e, Q-353).
    assert!(
        ComponentType::TableRows
            .datasets()
            .any(|dataset| dataset.id == BUDGET_UTILIZATION)
    );
    for other in ComponentType::LIBRARY {
        if ![kind, ComponentType::CustomChart, ComponentType::TableRows].contains(&other) {
            assert!(
                other
                    .datasets()
                    .all(|dataset| dataset.id != BUDGET_UTILIZATION),
                "{}",
                other.id()
            );
        }
    }
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
            &[category],
            &[],
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

/// The generated spending of a second category, month by month.
fn other_spending() -> impl Strategy<Value = Vec<Vec<(i8, i64)>>> {
    prop::collection::vec(
        prop::collection::vec((1i8..=28, -5_000i64..50_000), 0..4),
        8,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Changing what the budget counts from one period on (D-689) never
    /// changes an earlier period's figures; from that period on it counts
    /// the new category only, and the carried chain runs on across the
    /// change as a step-by-step sum, also with an amount changed with it.
    #[test]
    fn a_new_set_never_alters_an_earlier_period_and_the_chain_runs_on(
        plan in months(),
        other in other_spending(),
        from in 0usize..8,
        new_amount in proptest::option::of(1i64..100_000),
    ) {
        let from = from % plan.len();
        let (mut book, budget, _) = generated(&plan);
        let checking = book
            .accounts(false)
            .unwrap()
            .into_iter()
            .find(|account| account.name == "Checking")
            .unwrap()
            .id;
        let gifts = account(&mut book, "Gifts", AccountKind::Expense, EUR, None);
        for (index, spending) in other.iter().take(plan.len()).enumerate() {
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
                        Split::new(gifts, money(cents)),
                    ],
                );
            }
        }
        let shown = all_months(plan.len());
        let before = ask(&book, shown, shown.last(), BudgetGrouping::Period, &Filter::default());
        let carry = plan[from].1;
        book.edit_budget(
            budget,
            month_start(from),
            new_amount.map(|amount| (money(amount), carry)),
            Some((&[gifts], &[])),
            month_start(from),
        )
        .unwrap();
        let after = ask(&book, shown, shown.last(), BudgetGrouping::Period, &Filter::default());
        let (before, after) = (&find(&before, budget).periods, &find(&after, budget).periods);
        prop_assert_eq!(&before[..from], &after[..from]);
        let mut carried = if from == 0 {
            0
        } else {
            let last = &after[from - 1];
            if last.carries_over { last.left.minor_units() } else { 0 }
        };
        for (index, row) in after.iter().enumerate().skip(from) {
            // A new amount replaces what was planned for later periods.
            let (amount, carries) = match new_amount {
                Some(amount) => (amount, carry),
                None => (plan[index].0, plan[index].1),
            };
            let spent: i64 = other[index].iter().map(|(_, cents)| cents).sum();
            prop_assert_eq!(row.planned.minor_units(), amount);
            prop_assert_eq!(row.carried_in.minor_units(), carried);
            prop_assert_eq!(row.spent.minor_units(), spent);
            prop_assert_eq!(row.left, row.available.checked_sub(row.spent).unwrap());
            prop_assert_eq!(row.counts_changed, index == from && from > 0);
            carried = if carries { amount + carried - spent } else { 0 };
        }
    }
}

#[test]
fn a_budget_shows_the_set_of_the_period_asked_for_and_says_when_it_changed() {
    let mut base = sample();
    let id = base
        .book
        .add_budget(
            &[base.groceries],
            &[],
            BudgetEvery::Month,
            date(2026, 8, 1),
            None,
            money(30_000),
            false,
        )
        .unwrap();
    base.book
        .edit_budget(
            id,
            date(2026, 10, 1),
            None,
            Some((&[base.rent], &[base.checking])),
            date(2026, 10, 5),
        )
        .unwrap();
    let all = period(date(2026, 8, 1), date(2026, 10, 31));
    let results = ask(
        &base.book,
        all,
        date(2026, 10, 31),
        BudgetGrouping::Period,
        &Filter::default(),
    );
    let shown = find(&results, id);
    // The row names October's set; September still counted groceries.
    assert_eq!(shown.categories, [base.rent]);
    assert_eq!(shown.accounts, [base.checking]);
    let [august, september, last] = &shown.periods[..] else {
        panic!("three months");
    };
    assert!(!august.counts_changed && !september.counts_changed);
    assert!(last.counts_changed);
    assert!(last.notes().contains(&BudgetNote::CountsChanged));
    assert_eq!(BudgetNote::CountsChanged.id(), "counts-changed");
    // Groceries 40.00 on 1 Sep, refund 15.00 on 12 Sep; rent 800.00 on
    // 3 Oct from checking.
    assert_eq!(september.spent, money(2_500));
    assert_eq!(last.spent, money(80_000));
    // The filter looks at the set of the period asked for.
    let groceries_only = Filter {
        categories: Some(vec![base.groceries]),
        ..Filter::default()
    };
    let september_only = period(date(2026, 9, 1), date(2026, 9, 30));
    let asked = |shown, filter| {
        ask(
            &base.book,
            shown,
            date(2026, 10, 31),
            BudgetGrouping::Budget,
            filter,
        )
    };
    assert!(
        asked(september_only, &groceries_only)
            .iter()
            .any(|row| row.budget == id)
    );
    assert!(
        !asked(october(), &groceries_only)
            .iter()
            .any(|row| row.budget == id)
    );
    // And the row for September names September's set.
    let september_row = asked(september_only, &Filter::default());
    assert_eq!(find(&september_row, id).categories, [base.groceries]);
}

/// What one budget made of `categories` from `accounts` spent in the
/// quarter July to September, asked for on 1 December; the budget is
/// removed again, so the next one can count the same money.
fn quarter_spent(book: &mut Book, categories: &[AccountId], accounts: &[AccountId]) -> Money {
    let id = book
        .add_budget(
            categories,
            accounts,
            BudgetEvery::Quarter,
            date(2026, 7, 1),
            None,
            money(10_000),
            false,
        )
        .unwrap();
    let results = ask(
        book,
        period(date(2026, 9, 1), date(2026, 9, 30)),
        date(2026, 12, 1),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let spent = find(&results, id).periods[0].spent;
    book.remove_budget(id).unwrap();
    spent
}

#[test]
fn a_budget_of_several_categories_counts_each_once_and_only_its_accounts() {
    let mut base = sample();
    let (food_id, groceries_id, rent_id) = (base.food, base.groceries, base.rent);
    let (checking_id, savings_id) = (base.checking, base.savings);
    let book = &mut base.book;
    let groceries = quarter_spent(book, &[groceries_id], &[]);
    let rent = quarter_spent(book, &[rent_id], &[]);
    let food = quarter_spent(book, &[food_id], &[]);
    assert!(!groceries.is_zero() && !rent.is_zero());
    // Categories from two branches of the tree add up.
    let both = quarter_spent(book, &[rent_id, groceries_id], &[]);
    assert_eq!(both, rent.checked_add(groceries).unwrap());
    // A category and one inside it count the inner one once.
    let food_twice = quarter_spent(book, &[groceries_id, food_id], &[]);
    assert_eq!(food_twice, food);
    // Limited to accounts, only their transactions count; every account
    // together is the budget without accounts.
    let checking = quarter_spent(book, &[rent_id, groceries_id], &[checking_id]);
    let savings = quarter_spent(book, &[rent_id, groceries_id], &[savings_id]);
    assert_eq!(checking.checked_add(savings).unwrap(), both);
    let from_both = quarter_spent(book, &[rent_id, groceries_id], &[savings_id, checking_id]);
    assert_eq!(from_both, both);

    // The figures say which categories and accounts they are for.
    let id = base
        .book
        .add_budget(
            &[base.rent, base.groceries],
            &[base.checking],
            BudgetEvery::Quarter,
            date(2026, 7, 1),
            None,
            money(10_000),
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
    let shown = find(&results, id);
    assert_eq!(shown.categories, [base.rent, base.groceries]);
    assert_eq!(shown.accounts, [base.checking]);
    assert_eq!(shown.paths.len(), 2);
    assert_eq!(shown.paths[1].last().map(String::as_str), Some("Groceries"));
    assert_eq!(
        shown.categories_text(" › "),
        format!(
            "{}, {}",
            shown.paths[0].join(" › "),
            shown.paths[1].join(" › ")
        )
    );
}

#[test]
fn only_the_named_budgets_are_shown() {
    let mut base = sample();
    let mut add = |category| {
        base.book
            .add_budget(
                &[category],
                &[],
                BudgetEvery::Month,
                date(2026, 9, 1),
                None,
                money(10_000),
                false,
            )
            .unwrap()
    };
    let (food, rent) = (add(base.food), add(base.rent));
    let source = Source::read_with_budgets(&base.book, october(), date(2026, 10, 31)).unwrap();
    let shown = |only: Option<&[BudgetId]>| -> Vec<BudgetId> {
        budget_utilization(
            &source,
            october(),
            date(2026, 10, 31),
            BudgetGrouping::Budget,
            &Filter::default(),
            only,
        )
        .unwrap()
        .iter()
        .map(|result| result.budget)
        .collect()
    };
    assert_eq!(shown(None), [food, rent]);
    assert_eq!(shown(Some(&[rent])), [rent]);
    // A budget no longer in the book is simply not there.
    assert_eq!(shown(Some(&[BudgetId::generate()])), Vec::<BudgetId>::new());
}

/// A budget the user named is shown by its name, one without by its
/// categories (D-664); the name never changes what it counts.
#[test]
fn a_named_budget_is_labelled_by_its_name() {
    let mut base = sample();
    let food = base
        .book
        .add_named_budget(
            Some("Weekly shop"),
            &[base.food],
            &[],
            BudgetEvery::Month,
            date(2026, 8, 1),
            None,
            money(5_000),
            true,
        )
        .unwrap();
    let results = ask(
        &base.book,
        october(),
        date(2026, 12, 1),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let shown = find(&results, food);
    assert_eq!(shown.name.as_deref(), Some("Weekly shop"));
    assert_eq!(shown.label(" › "), "Weekly shop");
    assert_eq!(shown.categories_text(" › "), "Food");
    let named_spent = shown.periods[0].spent;
    base.book.set_budget_name(food, None).unwrap();
    let results = ask(
        &base.book,
        october(),
        date(2026, 12, 1),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let shown = find(&results, food);
    assert_eq!(shown.name, None);
    assert_eq!(shown.label(" › "), "Food");
    assert_eq!(shown.periods[0].spent, named_spent);
}

/// A budget that starts after the day asked for (M6-B236, M6-B237, D-710):
/// with the grouping `budget` it shows its first period, not started, with
/// nothing spent, nothing carried in and all of it left, even when a
/// transaction is already booked in it; a chart over time leaves it out.
#[test]
fn a_budget_that_starts_later_shows_its_first_period_with_all_of_it_left() {
    let mut base = sample();
    let loan = account(&mut base.book, "Loan", AccountKind::Expense, EUR, None);
    let once = base
        .book
        .add_budget(
            &[loan],
            &[],
            BudgetEvery::Range,
            date(2026, 12, 1),
            Some(date(2027, 11, 30)),
            money(110_599),
            false,
        )
        .unwrap();
    let pets = account(&mut base.book, "Pets", AccountKind::Expense, EUR, None);
    let monthly = base
        .book
        .add_budget(
            &[pets],
            &[],
            BudgetEvery::Month,
            date(2027, 1, 1),
            None,
            money(5_000),
            true,
        )
        .unwrap();
    // Booked ahead, inside the first period: not spent before it starts.
    add(
        &mut base.book,
        date(2026, 12, 5),
        vec![
            Split::new(base.checking, money(-2_000)),
            Split::new(loan, money(2_000)),
        ],
    );
    let today = date(2026, 11, 11);
    let november = period(date(2026, 11, 1), date(2026, 11, 30));
    let shown = ask(
        &base.book,
        november,
        today,
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let row = find(&shown, once);
    assert_eq!(row.periods.len(), 1);
    let first = &row.periods[0];
    assert!(first.starts_later);
    assert_eq!(
        (first.first_day, first.last_day, first.counted_to),
        (date(2026, 12, 1), date(2027, 11, 30), date(2026, 12, 1))
    );
    assert_eq!(first.planned, money(110_599));
    assert_eq!(first.available, money(110_599));
    assert_eq!(first.carried_in, money(0));
    assert_eq!(first.spent, money(0));
    assert_eq!(first.left, money(110_599));
    assert_eq!(first.used, Some(Percent::from_tenths(0)));
    assert_eq!(first.notes(), Vec::new());
    // A repeating one shows only its first period.
    let pets_first = &find(&shown, monthly).periods;
    assert_eq!(pets_first.len(), 1);
    assert!(pets_first[0].starts_later);
    assert_eq!(
        (pets_first[0].first_day, pets_first[0].last_day),
        (date(2027, 1, 1), date(2027, 1, 31))
    );
    assert_eq!(pets_first[0].left, money(5_000));
    // The budgets that run show as before, not started.
    assert!(
        shown
            .iter()
            .filter(|result| result.budget != once && result.budget != monthly)
            .all(|result| result.periods.iter().all(|period| !period.starts_later))
    );
    // Only the ones asked for.
    let source = Source::read_with_budgets(&base.book, november, today).expect("read");
    let only = budget_utilization(
        &source,
        november,
        today,
        BudgetGrouping::Budget,
        &Filter::default(),
        Some(&[once]),
    )
    .expect("budgets");
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].budget, once);
    // A chart over time has no period of it yet (D-379).
    let over_time = ask(
        &base.book,
        november,
        today,
        BudgetGrouping::Period,
        &Filter::default(),
    );
    assert!(
        over_time
            .iter()
            .all(|result| result.budget != once && result.budget != monthly)
    );
    // Once it has started it counts as any budget.
    let december = period(date(2026, 12, 1), date(2026, 12, 31));
    let started = ask(
        &base.book,
        december,
        date(2026, 12, 31),
        BudgetGrouping::Budget,
        &Filter::default(),
    );
    let running = &find(&started, once).periods[0];
    assert!(!running.starts_later);
    assert_eq!(running.spent, money(2_000));
    assert_eq!(running.left, money(108_599));
    assert!(find(&started, monthly).periods[0].starts_later);
}
