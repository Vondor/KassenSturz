// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the warnings tile's rules (US4.9, D-721 to D-726): each reason
//! triggered and not, at its exact limit, the lowest point of a month with
//! several bookings on one day, a removed budget or account, different
//! currencies, and the rules read from and written to a document. The
//! budget figures are checked against `budget_utilization`'s own output, so
//! the tile and the bars agree.

use kassensturz_core::{AccountId, AccountKind, BudgetEvery, BudgetId, Date, Money};
use kassensturz_store::Book;
use serde_json::json;

use crate::tests::{EUR, USD, account, date, money, transaction};
use crate::*;

struct Shared {
    book: Book,
    shared: AccountId,
    dollars: AccountId,
    /// Rent and power, 1,900.00 a month, from 1 October.
    costs: BudgetId,
    /// Food, 400.00 a month, from 1 October.
    groceries: BudgetId,
}

fn add(book: &mut Book, day: Date, parts: &[(AccountId, Money)]) {
    let parts: Vec<_> = parts
        .iter()
        .map(|&(id, amount)| (Some(id), amount))
        .collect();
    book.add_transaction(&transaction(day, &parts))
        .expect("added");
}

/// A shared account that starts November with 2,000.00: rent of 1,500.00
/// goes out on the 3rd, then two people pay in 1,000.00 each on the 10th,
/// the second payment on the same day as 300.00 of food. Its lowest
/// end-of-day balance in November is 500.00 on the 3rd; today, the 20th,
/// it holds 2,200.00.
fn shared() -> Shared {
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).unwrap();
    book.use_currency(USD).unwrap();
    let shared = account(&mut book, "Shared account", AccountKind::Asset, EUR, None);
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD, None);
    let opening = account(&mut book, "Opening", AccountKind::Equity, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let rent = account(&mut book, "Rent", AccountKind::Expense, EUR, None);
    let food = account(&mut book, "Food", AccountKind::Expense, EUR, None);
    add(
        &mut book,
        date(2026, 10, 1),
        &[(shared, money(200_000)), (opening, money(-200_000))],
    );
    // October: 1,500.00 rent and 380.00 of food, then the payments in.
    add(
        &mut book,
        date(2026, 10, 3),
        &[(shared, money(-150_000)), (rent, money(150_000))],
    );
    add(
        &mut book,
        date(2026, 10, 15),
        &[(shared, money(-38_000)), (food, money(38_000))],
    );
    add(
        &mut book,
        date(2026, 10, 20),
        &[(shared, money(188_000)), (salary, money(-188_000))],
    );
    // November.
    add(
        &mut book,
        date(2026, 11, 3),
        &[(shared, money(-150_000)), (rent, money(150_000))],
    );
    add(
        &mut book,
        date(2026, 11, 10),
        &[(shared, money(100_000)), (salary, money(-100_000))],
    );
    add(
        &mut book,
        date(2026, 11, 10),
        &[(shared, money(100_000)), (salary, money(-100_000))],
    );
    add(
        &mut book,
        date(2026, 11, 10),
        &[(shared, money(-30_000)), (food, money(30_000))],
    );
    let costs = book
        .add_budget(
            &[rent],
            &[],
            BudgetEvery::Month,
            date(2026, 10, 1),
            None,
            money(190_000),
            false,
        )
        .unwrap();
    let groceries = book
        .add_budget(
            &[food],
            &[],
            BudgetEvery::Month,
            date(2026, 10, 1),
            None,
            money(40_000),
            false,
        )
        .unwrap();
    Shared {
        book,
        shared,
        dollars,
        costs,
        groceries,
    }
}

fn today() -> Date {
    date(2026, 11, 20)
}

fn budget_rule(reason: AlarmReason, budget: BudgetId, share: u16) -> AlarmRule {
    AlarmRule {
        budget: budget.to_string(),
        share,
        ..AlarmRule::new(reason)
    }
}

fn below(account: AccountId, amount: &str, lowest: bool) -> AlarmRule {
    AlarmRule {
        account: account.to_string(),
        amount: amount.to_owned(),
        lowest,
        ..AlarmRule::new(AlarmReason::AccountBelow)
    }
}

fn reserve(account: AccountId, budget: BudgetId, times: u8, lowest: bool) -> AlarmRule {
    AlarmRule {
        account: account.to_string(),
        budget: budget.to_string(),
        times,
        lowest,
        ..AlarmRule::new(AlarmReason::AccountReserve)
    }
}

fn states(book: &Book, rules: &[AlarmRule], today: Date) -> Vec<AlarmState> {
    evaluate_alarms(book, rules, today)
        .expect("evaluated")
        .into_iter()
        .map(|result| result.state)
        .collect()
}

#[test]
fn a_budget_over_a_share_is_strict_and_agrees_with_the_bars() {
    let base = shared();
    // November's food: 300.00 of 400.00, exactly 75 %.
    let rules = [
        budget_rule(AlarmReason::BudgetOver, base.groceries, 74),
        budget_rule(AlarmReason::BudgetOver, base.groceries, 75),
        budget_rule(AlarmReason::BudgetOver, base.groceries, 100),
    ];
    let results = evaluate_alarms(&base.book, &rules, today()).unwrap();
    let found: Vec<_> = results.iter().map(|result| result.state).collect();
    assert_eq!(
        found,
        vec![AlarmState::Triggered, AlarmState::Fine, AlarmState::Fine]
    );
    // The figures are the bars' own.
    let period = Period::new(today(), today()).unwrap();
    let source = Source::read_with_budgets(&base.book, period, today()).unwrap();
    let bars = budget_utilization(
        &source,
        period,
        today(),
        BudgetGrouping::Budget,
        &Filter::default(),
        Some(&[base.groceries]),
    )
    .unwrap();
    let look = results[0].budget.expect("budget figures");
    assert_eq!(look.period, bars[0].periods[0]);
    assert_eq!(look.every, BudgetEvery::Month);
    assert_eq!(look.period.spent, money(30_000));
    assert_eq!(look.period.available, money(40_000));
}

#[test]
fn a_budget_under_a_share_reads_the_last_finished_period() {
    let base = shared();
    // October's food: 380.00 of 400.00, 95 %; November's 75 % doesn't count.
    let rules = [
        budget_rule(AlarmReason::BudgetUnder, base.groceries, 96),
        budget_rule(AlarmReason::BudgetUnder, base.groceries, 95),
        budget_rule(AlarmReason::BudgetUnder, base.groceries, 80),
    ];
    let results = evaluate_alarms(&base.book, &rules, today()).unwrap();
    assert_eq!(
        results
            .iter()
            .map(|result| result.state)
            .collect::<Vec<_>>(),
        vec![AlarmState::Triggered, AlarmState::Fine, AlarmState::Fine]
    );
    let look = results[0].budget.unwrap();
    assert_eq!(look.period.first_day, date(2026, 10, 1));
    assert_eq!(look.period.spent, money(38_000));
    // In its first month nothing has finished yet: not triggered.
    assert_eq!(
        states(&base.book, &rules[..1], date(2026, 10, 20)),
        vec![AlarmState::Fine]
    );
}

#[test]
fn an_account_under_an_amount_compares_today_or_the_lowest_point() {
    let base = shared();
    let rules = [
        // Today it holds 2,200.00.
        below(base.shared, "2200.01", false),
        below(base.shared, "2200.00", false),
        // Its lowest end-of-day balance is 500.00 on the 3rd: the salary
        // and the food on the 10th count only as that day's end.
        below(base.shared, "500.01", true),
        below(base.shared, "500", true),
    ];
    let results = evaluate_alarms(&base.book, &rules, today()).unwrap();
    assert_eq!(
        results
            .iter()
            .map(|result| result.state)
            .collect::<Vec<_>>(),
        vec![
            AlarmState::Triggered,
            AlarmState::Fine,
            AlarmState::Triggered,
            AlarmState::Fine
        ]
    );
    assert_eq!(results[0].balance, Some(money(220_000)));
    assert_eq!(results[0].balance_day, Some(today()));
    assert_eq!(results[2].balance, Some(money(50_000)));
    assert_eq!(results[2].balance_day, Some(date(2026, 11, 3)));
    assert_eq!(results[2].limit, Some(money(50_001)));
}

#[test]
fn the_lowest_point_counts_the_end_of_the_month_before() {
    let base = shared();
    // On 1 November nothing is booked yet: the lowest point is the end of
    // October, 2,000.00 (2,000 − 1,500 − 380 + 1,880).
    let results = evaluate_alarms(
        &base.book,
        &[below(base.shared, "2000.01", true)],
        date(2026, 11, 1),
    )
    .unwrap();
    assert_eq!(results[0].state, AlarmState::Triggered);
    assert_eq!(results[0].balance, Some(money(200_000)));
    assert_eq!(results[0].balance_day, Some(date(2026, 10, 31)));
}

#[test]
fn a_reserve_is_times_what_the_budget_plans_at_the_lowest_point() {
    let base = shared();
    let rules = [
        // 1 × 1,900.00 against the lowest point, 500.00.
        reserve(base.shared, base.costs, 1, true),
        // Against today's 2,200.00: one month holds, two don't.
        reserve(base.shared, base.costs, 1, false),
        reserve(base.shared, base.costs, 2, false),
    ];
    let results = evaluate_alarms(&base.book, &rules, today()).unwrap();
    assert_eq!(
        results
            .iter()
            .map(|result| result.state)
            .collect::<Vec<_>>(),
        vec![
            AlarmState::Triggered,
            AlarmState::Fine,
            AlarmState::Triggered
        ]
    );
    assert_eq!(results[0].limit, Some(money(190_000)));
    assert_eq!(results[2].limit, Some(money(380_000)));
    assert_eq!(results[0].budget.unwrap().period.planned, money(190_000));
}

#[test]
fn different_currencies_and_a_bad_amount_are_never_fine() {
    let base = shared();
    let rules = [
        reserve(base.dollars, base.costs, 1, true),
        // Euros have two decimal places.
        below(base.shared, "1.005", false),
        below(base.shared, "99999999999999999", false),
    ];
    assert_eq!(
        states(&base.book, &rules, today()),
        vec![
            AlarmState::CantCheck(CantCheck::Currencies),
            AlarmState::CantCheck(CantCheck::Amount),
            AlarmState::CantCheck(CantCheck::Amount),
        ]
    );
    // Trailing zeros beyond the currency's digits are the same amount.
    let results =
        evaluate_alarms(&base.book, &[below(base.shared, "500.000", true)], today()).unwrap();
    assert_eq!(results[0].limit, Some(money(50_000)));
}

#[test]
fn a_removed_budget_or_account_is_shown_as_gone() {
    let base = shared();
    let rules = [
        budget_rule(AlarmReason::BudgetOver, BudgetId::generate(), 100),
        below(AccountId::generate(), "1", false),
        reserve(base.shared, BudgetId::generate(), 1, true),
    ];
    assert_eq!(
        states(&base.book, &rules, today()),
        vec![AlarmState::Gone, AlarmState::Gone, AlarmState::Gone]
    );
}

#[test]
fn a_budget_with_nothing_to_spend_is_over_once_anything_is_spent() {
    let mut base = shared();
    // 100.00 a month with carry over: October's 380.00 of treats leaves
    // November with −180.00 to spend, and November spends 300.00 more.
    let treats = account(&mut base.book, "Treats", AccountKind::Expense, EUR, None);
    for day in [date(2026, 10, 15), date(2026, 11, 10)] {
        let amount = if day.month() == 10 { 38_000 } else { 30_000 };
        add(
            &mut base.book,
            day,
            &[(base.shared, money(-amount)), (treats, money(amount))],
        );
    }
    let empty = base
        .book
        .add_budget(
            &[treats],
            &[],
            BudgetEvery::Month,
            date(2026, 10, 1),
            None,
            money(10_000),
            true,
        )
        .unwrap();
    let rules = [
        budget_rule(AlarmReason::BudgetOver, empty, 100),
        budget_rule(AlarmReason::BudgetUnder, empty, 80),
    ];
    assert_eq!(
        states(&base.book, &rules, today()),
        vec![AlarmState::Triggered, AlarmState::Fine]
    );
    // In December, November (−180.00 to spend) is the finished period:
    // nothing to spend is never "under". December starts over by what was
    // carried in, as the bars say.
    let december = evaluate_alarms(&base.book, &rules, date(2026, 12, 1)).unwrap();
    assert_eq!(december[1].state, AlarmState::Fine);
    assert_eq!(december[1].budget.unwrap().period.available, money(-18_000));
    assert_eq!(december[0].state, AlarmState::Triggered);
    assert!(december[0].budget.unwrap().period.over().is_some());
}

#[test]
fn large_amounts_never_overflow_the_comparison() {
    // `spent × 100` and `available × share` of the largest amounts fit in
    // 128 bits.
    let rule = budget_rule(AlarmReason::BudgetOver, BudgetId::generate(), MAX_SHARE);
    assert!(rule.check().is_ok());
    let most = amount_money("92233720368547758.07", EUR, 2).expect("i64::MAX");
    assert_eq!(most.minor_units(), i64::MAX);
    assert_eq!(amount_money("92233720368547758.08", EUR, 2), None);
    assert_eq!(amount_text(most, 2), "92233720368547758.07");
    assert_eq!(amount_text(money(-5), 2), "-0.05");
    assert_eq!(amount_text(Money::from_minor_units(12, EUR), 0), "12");
}

#[test]
fn the_rules_are_read_and_written_with_the_fields_of_their_reason_only() {
    let base = shared();
    let rules = vec![
        budget_rule(AlarmReason::BudgetOver, base.groceries, 80),
        budget_rule(AlarmReason::BudgetUnder, base.groceries, 80),
        below(base.shared, "-12.50", true),
        reserve(base.shared, base.costs, 2, true),
    ];
    let mut component = Component::new(ComponentType::Alarms, "").unwrap();
    component
        .set("alarms", Setting::Alarms(rules.clone()))
        .unwrap();
    let value = component.to_value();
    assert_eq!(
        value["props"]["alarms"][0],
        json!({"reason": "budget_over", "budget": base.groceries.to_string(), "share": 80})
    );
    assert_eq!(
        value["props"]["alarms"][2],
        json!({"reason": "account_below", "account": base.shared.to_string(),
            "amount": "-12.50", "lowest": true})
    );
    assert_eq!(value["props"]["alarms"][3]["times"], json!(2));
    let back = Component::from_value(&value).unwrap();
    assert_eq!(back, component);
    assert_eq!(back.get("alarms"), Some(&Setting::Alarms(rules)));
    // `lowest` may be left out: it takes the reason's default.
    let short = json!({"type": "notice.alarms", "props": {"alarms": [
        {"reason": "account_reserve", "account": "a", "budget": "b", "times": 1}]}});
    let read = Component::from_value(&short).unwrap();
    assert!(matches!(read.get("alarms"), Some(Setting::Alarms(list)) if list[0].lowest));
}

#[test]
fn a_bad_rule_is_refused_with_the_place_it_sits() {
    let id = AccountId::generate().to_string();
    let bad = [
        // A field of another reason, an unknown field, a missing field.
        json!({"reason": "budget_over", "budget": "b", "share": 80, "amount": "1"}),
        json!({"reason": "budget_over", "budget": "b", "share": 80, "note": "x"}),
        json!({"reason": "budget_over", "share": 80}),
        json!({"reason": "warn_me", "budget": "b", "share": 80}),
        json!({"reason": "budget_over", "budget": "b", "share": 0}),
        json!({"reason": "budget_over", "budget": "b", "share": 1001}),
        json!({"reason": "budget_over", "budget": "b", "share": "80"}),
        json!({"reason": "account_below", "account": id, "amount": "1,00"}),
        json!({"reason": "account_below", "account": id, "amount": "1."}),
        json!({"reason": "account_below", "account": id, "amount": ".5"}),
        json!({"reason": "account_below", "account": id, "amount": "+5"}),
        json!({"reason": "account_below", "account": id, "amount": "123456789012345678901"}),
        json!({"reason": "account_below", "account": "", "amount": "1"}),
        json!({"reason": "account_below", "account": "x".repeat(65), "amount": "1"}),
        json!({"reason": "account_reserve", "account": id, "budget": "b", "times": 0}),
        json!({"reason": "account_reserve", "account": id, "budget": "b", "times": 25}),
        json!({"reason": "account_reserve", "account": id, "budget": "b\n", "times": 1}),
        json!("budget_over"),
    ];
    for rule in bad {
        let value = json!({"type": "notice.alarms", "props": {"alarms": [rule.clone()]}});
        match Component::from_value(&value) {
            Err(ReportError::DocumentInvalid { path, .. }) => {
                assert_eq!(path, "props.alarms", "{rule}");
            }
            other => panic!("{rule} was not refused: {other:?}"),
        }
    }
    let one = json!({"reason": "budget_over", "budget": "b", "share": 80});
    let many = json!({"type": "notice.alarms",
        "props": {"alarms": vec![one; MAX_ALARMS + 1]}});
    assert!(matches!(
        Component::from_value(&many),
        Err(ReportError::DocumentInvalid {
            reason: "too-many",
            ..
        })
    ));
}

#[test]
fn the_tile_shows_no_data_and_the_ai_never_sees_it() {
    let kind = ComponentType::Alarms;
    assert_eq!(ComponentType::from_id("notice.alarms"), Some(kind));
    assert!(ComponentType::LIBRARY.contains(&kind));
    assert!(!ComponentType::ALL.contains(&kind));
    assert!(kind.shows_no_data());
    assert_eq!(kind.default_size(), grid::Size { cols: 6, rows: 3 });
    // It shrinks to its status icon below 3 x 2 (D-735).
    assert_eq!(kind.smallest_size(), grid::Size { cols: 1, rows: 1 });
    assert_eq!(kind.full_size(), Some(grid::Size { cols: 3, rows: 2 }));
    let component = Component::new(kind, "").unwrap();
    assert_eq!(
        component.to_value(),
        json!({"type": "notice.alarms", "props": {"alarms": [], "title": ""}})
    );
}
