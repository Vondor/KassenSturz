// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Budgets the AI prepares and reads (M6c.11, D-358, D-394 to D-396) on the
//! five-year Space Colony book as the KMyMoney importer, the pipeline and
//! the store fill it, with the figures of `budget_utilization` (M6c.4) for
//! a budget the user made by hand (AGENTS.md §9.4: tests where steps meet).

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{Confirmed, ProposedChange, ToolError, call, call_allowed, propose_on};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{AccountKind, BudgetEvery, Date, Report};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_report::{BudgetGrouping, Filter, Period, Source, budget_utilization};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 10, 28);

fn five_years() -> Book {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/books/space-colony-5y-v15.sqlite");
    let bytes = fs::read(&path).expect("the Space Colony book is in the repository");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    book
}

/// The top-level spending category whose name comes first: a category with
/// everything inside it, as a budget counts it.
fn a_spending_category(book: &Book) -> kassensturz_core::AccountId {
    book.categories()
        .expect("categories")
        .into_iter()
        .filter(|entry| entry.category.kind == AccountKind::Expense && entry.path.len() == 1)
        .min_by(|a, b| a.path.cmp(&b.path))
        .expect("a spending category")
        .category
        .id
}

#[test]
fn an_amount_based_on_spending_uses_the_figures_a_budget_made_by_hand_shows() {
    let mut book = five_years();
    let category = a_spending_category(&book);
    let proposal = propose_on(
        &book,
        &json!({"new_budgets": [{"category_id": category.to_string(), "every": "month",
            "based_on": "average_3_periods", "round_to": 5}]}),
        Permissions::all(),
        TODAY,
    )
    .expect("prepared");
    let [ProposedChange::NewBudget(new)] = proposal.changes.changes() else {
        panic!("one new budget");
    };
    let new = new.clone();
    let basis = new.basis.expect("computed");
    assert_eq!(basis.first_day, Date::constant(2026, 7, 1));
    assert_eq!(basis.last_day, Date::constant(2026, 9, 30));

    // The same three months, as the dataset counts them for a budget the
    // user made by hand for them (it ends before the prepared one starts).
    book.add_budget(
        category,
        BudgetEvery::Month,
        basis.first_day,
        Some(basis.last_day),
        kassensturz_core::Money::from_minor_units(100, new.amount.currency()),
        false,
    )
    .expect("by hand");
    let period = Period::new(basis.first_day, basis.last_day).expect("a period");
    let source = Source::read_with_budgets(&book, period, basis.last_day).expect("read");
    let rows = budget_utilization(
        &source,
        period,
        basis.last_day,
        BudgetGrouping::Period,
        &Filter::default(),
    )
    .expect("figures");
    let spent: i64 = rows[0]
        .periods
        .iter()
        .map(|shown| shown.spent.minor_units())
        .sum();
    assert_eq!(rows[0].periods.len(), 3);
    assert_eq!(basis.spent.minor_units(), spent);
    assert!(spent > 0, "the Space Colony spends on {:?}", new.path);
    // Rounded up to a multiple of 5.00, and never below the average.
    let amount = new.amount.minor_units();
    assert_eq!(amount % 500, 0);
    assert!(amount * 3 >= spent && (amount - 500) * 3 < spent);

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let budgets = book.budgets().expect("budgets");
    assert_eq!(budgets.len(), 2);
    assert_eq!(budgets[1].first_day, Date::constant(2026, 10, 1));
    assert_eq!(budgets[1].last_day, None);
    assert_eq!(budgets[1].amounts[0].amount, new.amount);
}

fn read(book: &Book, permissions: Permissions) -> Result<Value, ToolError> {
    call_allowed(book, "list_budgets", &json!({}), TODAY, permissions).map(|output| output.value)
}

#[test]
fn list_budgets_reads_what_the_grid_allows_and_its_figures_come_from_the_dataset() {
    let mut book = five_years();
    let category = a_spending_category(&book);
    let eur = book.account(category).expect("there").currency;
    book.add_budget(
        category,
        BudgetEvery::Month,
        Date::constant(2026, 1, 1),
        None,
        kassensturz_core::Money::from_minor_units(40_000, eur),
        true,
    )
    .expect("by hand");
    // Ended budgets are not listed: nothing about them can change.
    let other = book
        .categories()
        .expect("categories")
        .into_iter()
        .filter(|entry| {
            entry.category.kind == AccountKind::Expense && entry.category.id != category
        })
        .find(|entry| entry.path.len() == 1)
        .expect("another")
        .category
        .id;
    book.add_budget(
        other,
        BudgetEvery::Year,
        Date::constant(2025, 1, 1),
        Some(Date::constant(2025, 12, 31)),
        kassensturz_core::Money::from_minor_units(1_000, eur),
        false,
    )
    .expect("by hand");

    let none = Permissions::NONE;
    let error = read(&book, none).expect_err("not allowed");
    assert_eq!(error.report().code, "ai-tools.not-allowed");
    // Budgets › Read alone isn't enough: a budget names its category.
    let budgets_only = none.with(Permission::BudgetsRead, true);
    assert!(read(&book, budgets_only).is_err());
    let names = budgets_only.with(Permission::CategoriesRead, true);
    let listed = read(&book, names).expect("read");
    let rows = listed["budgets"].as_array().expect("a list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["category_id"], category.to_string());
    assert_eq!(rows[0]["every"], "month");
    assert_eq!(rows[0]["amount"], "400.00");
    assert_eq!(rows[0]["carry_over"], true);
    assert!(
        rows[0].get("current_period").is_none(),
        "no spending without Transactions"
    );

    let all = read(&book, names.with(Permission::TransactionsRead, true)).expect("read");
    let current = &all["budgets"][0]["current_period"];
    assert_eq!(current["from"], "2026-10-01");
    assert_eq!(current["to"], "2026-10-31");
    // The figures are the dataset's for the same day.
    let period = Period::new(TODAY, TODAY).expect("a day");
    let source = Source::read_with_budgets(&book, period, TODAY).expect("read");
    let rows = budget_utilization(
        &source,
        period,
        TODAY,
        BudgetGrouping::Budget,
        &Filter::default(),
    )
    .expect("figures");
    let shown = &rows[0].periods[0];
    let text = |units: i64| format!("{}.{:02}", units / 100, (units % 100).abs());
    assert_eq!(current["spent"], text(shown.spent.minor_units()));
    assert_eq!(current["left"], text(shown.left.minor_units()));
    assert_eq!(current["available"], text(shown.available.minor_units()));
    // The full call without a grid gives the same.
    assert_eq!(
        call(&book, "list_budgets", &json!({}), TODAY)
            .expect("read")
            .value,
        all
    );
    // An argument it doesn't take is refused.
    assert!(call(&book, "list_budgets", &json!({"all": true}), TODAY).is_err());
}
