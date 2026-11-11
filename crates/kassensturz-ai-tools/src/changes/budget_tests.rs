// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Budgets the AI prepares (M6c.11, D-358, D-394 to D-396), on a small book
//! whose spending is known by hand, and the malformed calls a model may
//! make (they join the malformed-call suite of M6c.10).

use kassensturz_core::ai_switch::Permission;
use kassensturz_core::{AccountType, BudgetEvery, BudgetId, CurrencyCode, Date};
use serde_json::json;

use super::super::*;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

/// The day the tests run on: in November 2026.
const TODAY: Date = Date::constant(2026, 11, 14);

fn eur(cents: i64) -> Money {
    Money::from_minor_units(cents, EUR)
}

struct Small {
    book: Book,
    checking: AccountId,
    food: AccountId,
    groceries: AccountId,
    salary: AccountId,
    fun: AccountId,
}

fn account(name: &str, kind: AccountKind, parent: Option<AccountId>) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: EUR,
        account_type: kind.is_account().then_some(AccountType::Checking),
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

fn spend(book: &mut Book, from: AccountId, to: AccountId, day: Date, cents: i64) {
    let transaction = Transaction::new(
        TransactionId::generate(),
        day,
        Some("Shop".into()),
        vec![Split::new(from, eur(-cents)), Split::new(to, eur(cents))],
    )
    .expect("balances");
    book.add_transaction(&transaction).expect("added");
}

/// Spending in August, September and October 2026: 100.00, 200.00 and
/// 50.01 on Food and the Groceries inside it (Groceries counts towards
/// Food), and an earlier booking in May so that the book reaches back far
/// enough for three and six months but not for twelve.
fn small() -> Small {
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let checking = account("Checking", AccountKind::Asset, None);
    let food = account("Food", AccountKind::Expense, None);
    let groceries = account("Groceries", AccountKind::Expense, Some(food.id));
    let salary = account("Salary", AccountKind::Income, None);
    let fun = account("Fun", AccountKind::Expense, None);
    for account in [&checking, &food, &groceries, &salary, &fun] {
        book.add_account(account).expect("added");
    }
    let (c, f, g) = (checking.id, food.id, groceries.id);
    spend(&mut book, c, f, Date::constant(2026, 5, 2), 1_000);
    spend(&mut book, c, g, Date::constant(2026, 8, 1), 6_000);
    spend(&mut book, c, f, Date::constant(2026, 8, 31), 4_000);
    spend(&mut book, c, g, Date::constant(2026, 9, 15), 20_000);
    spend(&mut book, c, f, Date::constant(2026, 10, 31), 5_001);
    // This month's spending is not a full period: never counted.
    spend(&mut book, c, f, Date::constant(2026, 11, 2), 99_999);
    Small {
        book,
        checking: checking.id,
        food: f,
        groceries: g,
        salary: salary.id,
        fun: fun.id,
    }
}

fn propose(book: &Book, arguments: &Value) -> Result<Proposal, ToolError> {
    super::super::propose_on(book, arguments, Permissions::all(), TODAY)
}

fn code(result: Result<Proposal, ToolError>) -> String {
    match result.map_err(ToolError::without_item) {
        Ok(_) => "prepared".into(),
        Err(ToolError::BadArgument { argument } | ToolError::NotShaped { argument, .. }) => {
            format!("bad-argument {argument}")
        }
        Err(ToolError::UnknownField { argument, .. }) => format!("unknown-field {argument}"),
        Err(ToolError::NotAllowed { argument }) => format!("not-allowed {argument}"),
        Err(error) => {
            let report = error.report();
            match report.params.first() {
                Some((_, kassensturz_core::ErrorParam::Keyword(word))) => {
                    format!("{} {word}", report.code)
                }
                _ => report.code.into(),
            }
        }
    }
}

fn new_budget(set: &ChangeSet) -> &NewBudget {
    match set.changes() {
        [ProposedChange::NewBudget(new)] => new,
        other => panic!("one new budget, not {other:?}"),
    }
}

#[test]
fn an_amount_based_on_spending_is_the_average_rounded_up_to_the_step() {
    let small = small();
    let food = small.food.to_string();
    let proposal = propose(
        &small.book,
        &json!({"new_budgets": [{"category_id": food, "every": "month",
            "based_on": "average_3_periods", "round_to": 10}]}),
    )
    .expect("prepared");
    let new = new_budget(&proposal.changes);
    // 100.00 + 200.00 + 50.01 = 350.01 in August to October, 116.67 a month
    // (116.67 exactly would be 116.67 + 1/3 cent), rounded up to 120.00.
    let basis = new.basis.expect("computed");
    assert_eq!(basis.periods, 3);
    assert_eq!(basis.first_day, Date::constant(2026, 8, 1));
    assert_eq!(basis.last_day, Date::constant(2026, 10, 31));
    assert_eq!(basis.spent, eur(35_001));
    assert_eq!(basis.average, eur(11_667));
    assert_eq!(basis.round_to, 10);
    assert_eq!(new.amount, eur(12_000));
    assert_eq!(new.every, BudgetEvery::Month);
    assert_eq!(new.first_day, Date::constant(2026, 11, 1));
    assert_eq!(new.first_period_end, Date::constant(2026, 11, 30));
    assert_eq!(new.paths, [["Food"]]);
    assert!(!new.carry_over);
    // The model hears how many changes wait, never the amount (D-052).
    assert!(!proposal.output.value.to_string().contains("120"));

    // A step of 1 rounds 116.67 up to 117.00; the last period alone is
    // October's 50.01, rounded up to 51.00; a multiple stays as it is.
    for (based_on, round_to, cents) in [
        ("average_3_periods", 1, 11_700),
        ("last_period", 1, 5_100),
        ("last_period", 50, 5_000 * 2),
        ("average_6_periods", 100, 10_000),
    ] {
        let proposal = propose(
            &small.book,
            &json!({"new_budgets": [{"category_id": food, "every": "month",
                "based_on": based_on, "round_to": round_to}]}),
        )
        .expect("prepared");
        assert_eq!(
            new_budget(&proposal.changes).amount,
            eur(cents),
            "{based_on} {round_to}"
        );
    }
    // Six months, May to October: 360.01 / 6 = 60.0016…, rounded up to the
    // next 1.00: 61.00.
    let proposal = propose(
        &small.book,
        &json!({"new_budgets": [{"category_id": food, "every": "month",
            "based_on": "average_6_periods"}]}),
    )
    .expect("prepared");
    let new = new_budget(&proposal.changes);
    assert_eq!(new.basis.expect("computed").spent, eur(36_001));
    assert_eq!(new.amount, eur(6_100));
}

#[test]
fn a_prepared_budget_is_made_only_on_confirm_and_one_undo_takes_it_back() {
    let mut small = small();
    let fun = small.fun.to_string();
    let history = small.book.history().expect("history").len();
    let proposal = propose(
        &small.book,
        &json!({"new_budgets": [{"category_id": fun, "every": "quarter",
            "amount": "75.50", "carry_over": true}]}),
    )
    .expect("prepared");
    assert!(small.book.budgets().expect("budgets").is_empty());
    let new = new_budget(&proposal.changes);
    assert_eq!(new.amount, eur(7_550));
    assert_eq!(new.basis, None);
    assert_eq!(new.first_day, Date::constant(2026, 10, 1));
    assert_eq!(
        proposal.changes.summary(),
        "new budget for \"Fun\" every quarter from 2026-10-01"
    );
    proposal
        .changes
        .apply(&mut small.book, Confirmed::by_the_user())
        .expect("applied");
    let budgets = small.book.budgets().expect("budgets");
    assert_eq!(budgets.len(), 1);
    assert_eq!(budgets[0].categories, [small.fun]);
    assert_eq!(budgets[0].every, BudgetEvery::Quarter);
    assert_eq!(budgets[0].first_day, Date::constant(2026, 10, 1));
    assert_eq!(budgets[0].last_day, None);
    assert_eq!(budgets[0].amounts.len(), 1);
    assert_eq!(budgets[0].amounts[0].amount, eur(7_550));
    assert!(budgets[0].amounts[0].carry_over);
    let changes = small.book.history().expect("history");
    assert_eq!(changes.len(), history + 1, "one entry for the set");
    assert_eq!(changes[0].action, kassensturz_store::Action::ApplyAiChanges);
    small.book.undo().expect("undone");
    assert!(small.book.budgets().expect("budgets").is_empty());
}

#[test]
fn an_edit_changes_a_budget_from_the_current_period_on_and_keeps_the_past() {
    let mut small = small();
    let id = small
        .book
        .add_budget(
            &[small.food],
            &[],
            BudgetEvery::Month,
            Date::constant(2026, 6, 1),
            None,
            eur(30_000),
            false,
        )
        .expect("a budget made by hand");
    let proposal = propose(
        &small.book,
        &json!({"edit_budgets": [{"budget_id": id.to_string(),
            "based_on": "average_3_periods", "round_to": 5}]}),
    )
    .expect("prepared");
    let [ProposedChange::EditBudget(edit)] = proposal.changes.changes() else {
        panic!("one edit");
    };
    assert_eq!(edit.from_day, Date::constant(2026, 11, 1));
    assert_eq!(edit.before.amount, eur(30_000));
    // 116.67 rounded up to a multiple of 5.00.
    assert_eq!(edit.amount, eur(12_000));
    assert!(!edit.carry_over);
    proposal
        .changes
        .apply(&mut small.book, Confirmed::by_the_user())
        .expect("applied");
    let budget = small.book.budget(id).expect("there");
    let amounts: Vec<_> = budget
        .amounts
        .iter()
        .map(|row| (row.from_day, row.amount))
        .collect();
    assert_eq!(
        amounts,
        [
            (Date::constant(2026, 6, 1), eur(30_000)),
            (Date::constant(2026, 11, 1), eur(12_000)),
        ]
    );

    // Only the carry over: the amount stays.
    let proposal = propose(
        &small.book,
        &json!({"edit_budgets": [{"budget_id": id.to_string(), "carry_over": true}]}),
    )
    .expect("prepared");
    let [ProposedChange::EditBudget(edit)] = proposal.changes.changes() else {
        panic!("one edit");
    };
    assert_eq!(edit.amount, eur(12_000));
    assert!(edit.carry_over && !edit.before.carry_over);

    // The user changes the budget by hand before confirming: out of date.
    small
        .book
        .edit_budget_amount(id, Date::constant(2026, 11, 1), eur(9_900), false)
        .expect("edited by hand");
    let error = proposal
        .changes
        .apply(&mut small.book, Confirmed::by_the_user())
        .expect_err("out of date");
    assert_eq!(error.report().code, "ai-changes.out-of-date");
}

#[test]
fn the_stores_own_checks_reach_the_model() {
    let mut small = small();
    let call = |category: AccountId, every: &str| {
        json!({"new_budgets": [{"category_id": category.to_string(), "every": every,
            "amount": "10.00"}]})
    };
    assert_eq!(
        code(propose(&small.book, &call(small.salary, "month"))),
        "store.budget-not-spending"
    );
    assert_eq!(
        code(propose(&small.book, &call(small.checking, "month"))),
        "ai-tools.unknown-category"
    );
    small
        .book
        .add_budget(
            &[small.groceries],
            &[],
            BudgetEvery::Year,
            Date::constant(2026, 1, 1),
            Some(Date::constant(2026, 12, 31)),
            eur(1_000),
            false,
        )
        .expect("by hand");
    assert_eq!(
        code(propose(&small.book, &call(small.groceries, "month"))),
        "ai-tools.has-budget edit_budgets"
    );
    // A budget inside one that has a budget is a separate figure (D-344).
    assert_eq!(
        code(propose(&small.book, &call(small.food, "month"))),
        "prepared"
    );
    assert_eq!(
        code(propose(
            &small.book,
            &json!({"new_budgets": [{"category_id": small.fun.to_string(), "every": "month",
                "amount": "0.00"}]})
        )),
        "bad-argument amount"
    );
    assert_eq!(
        code(propose(
            &small.book,
            &json!({"edit_budgets": [{"budget_id": BudgetId::generate().to_string(),
                "amount": "1.00"}]})
        )),
        "store.budget-unknown"
    );
}

/// A budget item with a key it doesn't take, such as a colour, is refused
/// with the list, the item and the keys it does take; the model's key is
/// never repeated back (M6-B201, X-8).
#[test]
fn an_unknown_budget_key_lists_the_keys_a_budget_takes() {
    let small = small();
    let food = small.food.to_string();
    let call = json!({"new_budgets": [
        {"category_id": food, "every": "month", "amount": "10"},
        {"category_id": food, "every": "year", "amount": "10", "colour": "Groceries green"}
    ]});
    let error = propose(&small.book, &call).expect_err("refused");
    let allowed = error.allowed_fields().expect("the keys it takes");
    assert!(allowed.contains(&"category_ids") && allowed.contains(&"every"));
    assert!(allowed.contains(&"name") && !allowed.contains(&"colour"));
    let report = error.model_report();
    assert_eq!(report.code, "ai-tools.unknown-field");
    assert_eq!(
        report.param("argument"),
        Some(&kassensturz_core::ErrorParam::Keyword("new_budgets"))
    );
    assert_eq!(
        report.param("item"),
        Some(&kassensturz_core::ErrorParam::Number(2))
    );
    let shown = format!("{report:?} {allowed:?}");
    assert!(!shown.contains("Groceries"), "{shown}");
    assert!(!shown.contains("colour"), "{shown}");
}

#[test]
fn malformed_budget_calls_are_refused_with_a_fixed_keyword() {
    let small = small();
    let food = small.food.to_string();
    let fun = small.fun.to_string();
    let salary = small.salary.to_string();
    let checking = small.checking.to_string();
    let budget = |fields: Value| json!({"new_budgets": [fields]});
    let cases: Vec<(Value, &str)> = vec![
        (json!({"new_budgets": "food"}), "bad-argument new_budgets"),
        (json!({"new_budgets": ["food"]}), "bad-argument new_budgets"),
        (
            budget(json!({"every": "month", "amount": "1"})),
            "bad-argument category_id",
        ),
        (
            budget(json!({"category_id": "food", "every": "month", "amount": "1"})),
            "bad-argument category_id",
        ),
        (
            budget(json!({"category_id": food, "amount": "1"})),
            "bad-argument every",
        ),
        (
            budget(json!({"category_id": food, "every": "week", "amount": "1"})),
            "bad-argument every",
        ),
        // A stretch of days needs both days the user named (D-688).
        (
            budget(json!({"category_id": food, "every": "range", "amount": "1"})),
            "bad-argument first_day",
        ),
        (
            budget(json!({"category_id": food, "every": 1, "amount": "1"})),
            "bad-argument every",
        ),
        (
            budget(json!({"category_id": food, "every": "month"})),
            "bad-argument amount",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "amount": 12.5})),
            "bad-argument amount",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "amount": "12,50"})),
            "bad-argument amount",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "amount": "-12.50"})),
            "bad-argument amount",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "amount": "12.505"})),
            "bad-argument amount",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "amount": "1e3"})),
            "bad-argument amount",
        ),
        (
            budget(
                json!({"category_id": food, "every": "month", "amount": "1", "based_on": "last_period"}),
            ),
            "bad-argument based_on",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "based_on": "average_4_periods"})),
            "bad-argument based_on",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "based_on": "median"})),
            "bad-argument based_on",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "based_on": true})),
            "bad-argument based_on",
        ),
        // The book reaches back to May only: no twelve full months.
        (
            budget(
                json!({"category_id": food, "every": "month", "based_on": "average_12_periods"}),
            ),
            "ai-tools.nothing-to-base-on book-too-short",
        ),
        // No full year before 2026 in the book.
        (
            budget(json!({"category_id": food, "every": "year", "based_on": "last_period"})),
            "ai-tools.nothing-to-base-on book-too-short",
        ),
        // Fun spent nothing: nothing to base an amount on.
        (
            budget(json!({"category_id": fun, "every": "month", "based_on": "last_period"})),
            "ai-tools.nothing-to-base-on nothing-spent",
        ),
        (
            budget(
                json!({"category_id": food, "every": "month", "based_on": "last_period", "round_to": 3}),
            ),
            "bad-argument round_to",
        ),
        (
            budget(
                json!({"category_id": food, "every": "month", "based_on": "last_period", "round_to": "10"}),
            ),
            "bad-argument round_to",
        ),
        (
            budget(
                json!({"category_id": food, "every": "month", "based_on": "last_period", "round_to": -10}),
            ),
            "bad-argument round_to",
        ),
        (
            budget(json!({"category_id": food, "every": "month", "amount": "10", "round_to": 10})),
            "bad-argument round_to",
        ),
        (
            budget(
                json!({"category_id": food, "every": "month", "amount": "10", "carry_over": "yes"}),
            ),
            "bad-argument carry_over",
        ),
        // A repeating budget starts with the current period; only a
        // stretch of days names its first day (D-688).
        (
            budget(
                json!({"category_id": food, "every": "month", "amount": "10", "first_day": "2026-01-01"}),
            ),
            "bad-argument first_day",
        ),
        (
            budget(
                json!({"category_id": food, "every": "month", "amount": "10", "computed_amount": "10"}),
            ),
            "unknown-field new_budgets",
        ),
        (
            json!({"new_budgets": [
                {"category_id": food, "every": "month", "amount": "10"},
                {"category_id": food, "every": "year", "amount": "10"}
            ]}),
            "bad-argument category_id",
        ),
        // Several categories (D-641): one way to name them, as a list of
        // IDs, each once, in one currency; the accounts are the user's own.
        (
            budget(
                json!({"category_id": food, "category_ids": [fun], "every": "month", "amount": "10"}),
            ),
            "bad-argument category_ids",
        ),
        (
            budget(json!({"category_ids": [], "every": "month", "amount": "10"})),
            "bad-argument category_id",
        ),
        (
            budget(json!({"category_ids": food, "every": "month", "amount": "10"})),
            "bad-argument category_ids",
        ),
        (
            budget(json!({"category_ids": ["food"], "every": "month", "amount": "10"})),
            "bad-argument category_ids",
        ),
        (
            budget(json!({"category_ids": [food, food], "every": "month", "amount": "10"})),
            "store.budget-choice category-twice",
        ),
        (
            budget(json!({"category_ids": [food, salary], "every": "month", "amount": "10"})),
            "store.budget-not-spending",
        ),
        (
            budget(
                json!({"category_id": food, "account_ids": [fun], "every": "month", "amount": "10"}),
            ),
            "store.budget-choice not-own-account",
        ),
        (
            budget(
                json!({"category_id": food, "account_ids": [checking, checking], "every": "month", "amount": "10"}),
            ),
            "store.budget-choice account-twice",
        ),
        (
            budget(
                json!({"category_id": food, "account_ids": [BudgetId::generate().to_string()], "every": "month", "amount": "10"}),
            ),
            "ai-tools.unknown-account",
        ),
        (
            budget(
                json!({"category_id": food, "account_ids": "checking", "every": "month", "amount": "10"}),
            ),
            "bad-argument account_ids",
        ),
        (
            json!({"new_budgets": [
                {"category_ids": [food, fun], "every": "month", "amount": "10"},
                {"category_id": fun, "every": "year", "amount": "10"}
            ]}),
            "bad-argument category_id",
        ),
        (
            json!({"edit_budgets": [{"amount": "10"}]}),
            "bad-argument budget_id",
        ),
        (
            json!({"edit_budgets": [{"budget_id": "the food one", "amount": "10"}]}),
            "bad-argument budget_id",
        ),
        (
            json!({"edit_budgets": [{"budget_id": BudgetId::generate().to_string(), "delete": true}]}),
            "unknown-field edit_budgets",
        ),
    ];
    for (call, expected) in &cases {
        assert_eq!(&code(propose(&small.book, call)), expected, "{call}");
    }
    assert!(cases.len() >= 30, "enough cases for the suite");

    // At most 50 budgets in one set.
    let many: Vec<Value> = (0..51)
        .map(|_| json!({"category_id": food, "every": "month", "amount": "1"}))
        .collect();
    assert_eq!(
        code(propose(&small.book, &json!({ "new_budgets": many }))),
        "ai-tools.too-many-changes"
    );
    // A caller that doesn't know the day can't prepare budgets (D-394).
    assert_eq!(
        code(super::super::propose(
            &small.book,
            &budget(json!({"category_id": food, "every": "month", "amount": "1"})),
            Permissions::all()
        )),
        "bad-argument new_budgets"
    );
}

#[test]
fn each_budget_list_needs_its_own_cell_and_add_or_change_switch_read_on() {
    let small = small();
    let food = small.food.to_string();
    let add = json!({"new_budgets": [{"category_id": food, "every": "month", "amount": "1"}]});
    let none = Permissions::NONE.with(Permission::NewCategories, true);
    assert_eq!(
        code(super::super::propose_on(&small.book, &add, none, TODAY)),
        "not-allowed new_budgets"
    );
    let adds = none.with(Permission::NewBudgets, true);
    assert_eq!(
        code(super::super::propose_on(&small.book, &add, adds, TODAY)),
        "prepared"
    );
    let edit = json!({"edit_budgets": [{"budget_id": BudgetId::generate().to_string(),
        "amount": "1"}]});
    assert_eq!(
        code(super::super::propose_on(&small.book, &edit, adds, TODAY)),
        "not-allowed edit_budgets"
    );
    assert!(adds.allows(Permission::BudgetsRead), "Add switches Read on");
    assert!(
        none.with(Permission::EditBudgets, true)
            .allows(Permission::BudgetsRead),
        "Change switches Read on"
    );
    assert!(!none.allows(Permission::BudgetsRead));
    // A model is offered only what the user allows.
    let spec = propose_spec(adds).expect("offered");
    assert!(spec.parameters.contains("new_budgets"));
    assert!(!spec.parameters.contains("edit_budgets"));
    assert!(spec.description.contains("based_on"));
    assert!(
        !propose_spec(none)
            .expect("offered")
            .parameters
            .contains("budgets")
    );
    // Turned off after it was prepared: refused at the confirm.
    let proposal = super::super::propose_on(&small.book, &add, adds, TODAY).expect("prepared");
    assert!(proposal.changes.check_allowed(none).is_err());
    assert!(proposal.changes.check_allowed(adds).is_ok());
}

#[test]
fn a_budget_left_out_and_put_back_keeps_the_day_it_was_prepared_on() {
    let small = small();
    let food = small.food.to_string();
    let fun = small.fun.to_string();
    let proposal = propose(
        &small.book,
        &json!({"new_budgets": [
            {"category_id": food, "every": "month", "based_on": "last_period"},
            {"category_id": fun, "every": "year", "amount": "200"}
        ]}),
    )
    .expect("prepared");
    let slot = Slot::parse("new_budgets/0").expect("a slot");
    assert!(slot.editable().is_empty(), "budgets can only be left out");
    let without = proposal
        .changes
        .leave_out(&small.book, slot, Permissions::all())
        .expect("left out");
    assert_eq!(without.changes().len(), 1);
    let back = without
        .back_to_ai(&small.book, slot, Permissions::all())
        .expect("back");
    assert_eq!(back.changes(), proposal.changes.changes());
}

#[test]
fn a_budget_over_several_categories_from_one_account_is_prepared_and_made() {
    let mut small = small();
    let savings = account("Savings", AccountKind::Asset, None);
    small.book.add_account(&savings).expect("added");
    // Fun from savings: not counted for a budget of checking.
    spend(
        &mut small.book,
        savings.id,
        small.fun,
        Date::constant(2026, 10, 3),
        7_000,
    );
    spend(
        &mut small.book,
        small.checking,
        small.fun,
        Date::constant(2026, 10, 4),
        1_000,
    );
    let (food, fun, checking) = (
        small.food.to_string(),
        small.fun.to_string(),
        small.checking.to_string(),
    );
    let proposal = propose(
        &small.book,
        &json!({"new_budgets": [{"category_ids": [fun, food], "account_ids": [checking],
            "every": "month", "based_on": "last_period"}]}),
    )
    .expect("prepared");
    let new = new_budget(&proposal.changes).clone();
    assert_eq!(new.categories, [small.fun, small.food]);
    assert_eq!(new.paths, [vec!["Fun".to_owned()], vec!["Food".to_owned()]]);
    assert_eq!(new.accounts, [small.checking]);
    assert_eq!(new.account_names, ["Checking"]);
    // October from checking: 50.01 on Food and 10.00 on Fun; the 70.00
    // from savings are not counted.
    assert_eq!(new.basis.expect("computed").spent, eur(6_001));
    // Rounded up to a whole euro, the step when the call names none.
    assert_eq!(new.amount, eur(6_100));
    assert!(proposal.changes.summary().contains("from \"Checking\""));
    proposal
        .changes
        .apply(&mut small.book, Confirmed::by_the_user())
        .expect("applied");
    let budgets = small.book.budgets().expect("read");
    assert_eq!(budgets[0].categories, [small.fun, small.food]);
    assert_eq!(budgets[0].accounts, [small.checking]);
    // The same category from another account is a budget of its own; from
    // every account it would count checking twice.
    let savings_id = savings.id.to_string();
    assert_eq!(
        code(propose(
            &small.book,
            &json!({"new_budgets": [{"category_id": fun, "account_ids": [savings_id],
                "every": "month", "amount": "20"}]}),
        )),
        "prepared"
    );
    assert_eq!(
        code(propose(
            &small.book,
            &json!({"new_budgets": [{"category_id": fun, "every": "month", "amount": "20"}]}),
        )),
        "ai-tools.has-budget edit_budgets"
    );
}

#[test]
fn a_monthly_budget_can_start_on_a_salary_day_and_counts_those_months() {
    let small = small();
    let food = small.food.to_string();
    let proposal = propose(
        &small.book,
        &json!({"new_budgets": [{"category_id": food, "every": "month", "salary_day": 2,
            "based_on": "average_3_periods"}]}),
    )
    .expect("prepared");
    let new = new_budget(&proposal.changes);
    // On 14 November the month from day 2 runs from 2 November to
    // 1 December; the three before it hold 40.00 (31 August), 200.00
    // (15 September) and 50.01 (31 October): 290.01 / 3, rounded up to
    // 97.00. The 999.99 of 2 November is in the current one (D-651).
    assert_eq!(new.every, BudgetEvery::SalaryMonth { day: 2 });
    assert_eq!(new.first_day, Date::constant(2026, 11, 2));
    assert_eq!(new.first_period_end, Date::constant(2026, 12, 1));
    let basis = new.basis.expect("computed");
    assert_eq!(basis.first_day, Date::constant(2026, 8, 2));
    assert_eq!(basis.last_day, Date::constant(2026, 11, 1));
    assert_eq!(basis.spent, eur(29_001));
    assert_eq!(new.amount, eur(9_700));

    // Day 1 is the calendar month; a day outside a month, or a day for
    // a quarter, is refused by name.
    let call = |every: &str, day: Value| {
        json!({"new_budgets": [{"category_id": food, "every": every, "salary_day": day,
            "amount": "10.00"}]})
    };
    let proposal = propose(&small.book, &call("month", json!(1))).expect("prepared");
    assert_eq!(new_budget(&proposal.changes).every, BudgetEvery::Month);
    for (every, day) in [
        ("month", json!(0)),
        ("month", json!(32)),
        ("month", json!("25")),
        ("quarter", json!(25)),
    ] {
        assert_eq!(
            code(propose(&small.book, &call(every, day.clone()))),
            "bad-argument salary_day",
            "{every} {day}"
        );
    }
}

/// A budget the AI prepares can get a name the user asked for, and an
/// edit can change only the name, which leaves every amount as it is
/// (M6-B203, D-664). The book's own checks of a name reach the model.
#[test]
fn a_budget_gets_the_name_the_user_asked_for_and_keeps_its_amounts() {
    let mut small = small();
    let fun = small.fun.to_string();
    let proposal = propose(
        &small.book,
        &json!({"new_budgets": [{"name": " Weekends ", "category_id": fun,
            "every": "month", "amount": "40"}]}),
    )
    .expect("prepared");
    let new = new_budget(&proposal.changes);
    assert_eq!(new.name.as_deref(), Some("Weekends"));
    assert_eq!(
        proposal.changes.summary(),
        "new budget \"Weekends\" for \"Fun\" every month from 2026-11-01"
    );
    proposal
        .changes
        .apply(&mut small.book, Confirmed::by_the_user())
        .expect("applied");
    let made = small.book.budgets().expect("budgets");
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].name.as_deref(), Some("Weekends"));
    let id = made[0].id;

    // A budget made by hand, with an amount planned for a later period.
    let food = small
        .book
        .add_budget(
            &[small.food],
            &[],
            BudgetEvery::Month,
            Date::constant(2026, 6, 1),
            None,
            eur(30_000),
            false,
        )
        .expect("a budget made by hand");
    small
        .book
        .edit_budget_amount(food, Date::constant(2026, 12, 1), eur(35_000), false)
        .expect("planned for December");
    let before = small.book.budget(food).expect("there");
    let proposal = propose(
        &small.book,
        &json!({"edit_budgets": [{"budget_id": food.to_string(), "name": "Food at home"}]}),
    )
    .expect("prepared");
    let [ProposedChange::EditBudget(edit)] = proposal.changes.changes() else {
        panic!("one edit");
    };
    assert_eq!(edit.name_before, None);
    assert_eq!(edit.rename, Some(Some("Food at home".to_owned())));
    assert!(!edit.changes_amount);
    assert_eq!(
        proposal.changes.summary(),
        "budget for \"Food\" changes its name to \"Food at home\""
    );
    proposal
        .changes
        .apply(&mut small.book, Confirmed::by_the_user())
        .expect("applied");
    let after = small.book.budget(food).expect("there");
    assert_eq!(after.name.as_deref(), Some("Food at home"));
    // December's amount is still planned: a new name changes no amount.
    assert_eq!(after.amounts, before.amounts);
    small.book.undo().expect("undone");
    assert_eq!(small.book.budget(food).expect("there").name, None);

    // An empty name takes the name away; the same name again is no change.
    let proposal = propose(
        &small.book,
        &json!({"edit_budgets": [{"budget_id": id.to_string(), "name": ""}]}),
    )
    .expect("prepared");
    let [ProposedChange::EditBudget(edit)] = proposal.changes.changes() else {
        panic!("one edit");
    };
    assert_eq!(edit.rename, Some(None));
    assert_eq!(
        proposal.changes.summary(),
        "budget \"Weekends\" for \"Fun\" changes to have no name"
    );
    assert_eq!(
        code(propose(
            &small.book,
            &json!({"edit_budgets": [{"budget_id": id.to_string(), "name": "Weekends"}]}),
        )),
        "bad-argument name"
    );
    // A name with a line break or of more than 100 characters is refused
    // with the book's own code, and a name that is not text as a bad name.
    for (name, expected) in [
        (json!("Two\nlines"), "store.budget-name line-break"),
        (json!("x".repeat(101)), "store.budget-name too-long"),
        (json!(7), "bad-argument name"),
    ] {
        assert_eq!(
            code(propose(
                &small.book,
                &json!({"new_budgets": [{"name": name, "category_id": small.groceries.to_string(),
                    "every": "month", "amount": "10"}]}),
            )),
            expected,
            "{name}"
        );
    }
}

/// The book of #576: 50.00 a month on Food, booked on 30 June, 31 July,
/// 31 August and 30 September; the October payment slipped to 2 November,
/// and the November one is booked on 30 November. `from` leaves out the
/// payments before it, for a book that starts later.
fn slipped(from: Date) -> Small {
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let checking = account("Checking", AccountKind::Asset, None);
    let food = account("Food", AccountKind::Expense, None);
    let groceries = account("Groceries", AccountKind::Expense, Some(food.id));
    let salary = account("Salary", AccountKind::Income, None);
    let fun = account("Fun", AccountKind::Expense, None);
    for account in [&checking, &food, &groceries, &salary, &fun] {
        book.add_account(account).expect("added");
    }
    for day in [
        Date::constant(2026, 6, 30),
        Date::constant(2026, 7, 31),
        Date::constant(2026, 8, 31),
        Date::constant(2026, 9, 30),
        Date::constant(2026, 11, 2),
        Date::constant(2026, 11, 30),
    ] {
        if day >= from {
            spend(&mut book, checking.id, food.id, day, 5_000);
        }
    }
    Small {
        book,
        checking: checking.id,
        food: food.id,
        groceries: groceries.id,
        salary: salary.id,
        fun: fun.id,
    }
}

fn based_on(small: &Small, word: &str, today: Date) -> Result<Proposal, ToolError> {
    let food = small.food.to_string();
    super::super::propose_on(
        &small.book,
        &json!({"new_budgets": [{"category_id": food, "every": "month", "based_on": word}]}),
        Permissions::all(),
        today,
    )
}

#[test]
fn a_payment_that_slipped_past_the_month_end_moves_the_count_back_one_period() {
    let small = slipped(Date::constant(2000, 1, 1));
    // On 14 November October is empty: its payment is in November, which
    // is never counted. The count moves back to the months before it.
    let proposal = based_on(&small, "last_period", TODAY).expect("prepared");
    let new = new_budget(&proposal.changes);
    let basis = new.basis.expect("computed");
    assert_eq!(new.amount, eur(5_000));
    assert_eq!(basis.first_day, Date::constant(2026, 9, 1));
    assert_eq!(basis.last_day, Date::constant(2026, 9, 30));
    assert_eq!(
        basis.left_out,
        Some((Date::constant(2026, 10, 1), Date::constant(2026, 10, 31)))
    );

    let proposal = based_on(&small, "average_3_periods", TODAY).expect("prepared");
    let new = new_budget(&proposal.changes);
    let basis = new.basis.expect("computed");
    assert_eq!(new.amount, eur(5_000));
    assert_eq!(basis.spent, eur(15_000));
    assert_eq!(basis.first_day, Date::constant(2026, 7, 1));
    assert_eq!(basis.last_day, Date::constant(2026, 9, 30));
    assert!(basis.left_out.is_some());
}

#[test]
fn a_slip_inside_the_periods_counted_evens_out_and_nothing_moves() {
    let small = slipped(Date::constant(2000, 1, 1));
    // On 14 December October is empty and November holds two payments:
    // September to November spent 150.00, 50.00 a month.
    let today = Date::constant(2026, 12, 14);
    let proposal = based_on(&small, "average_3_periods", today).expect("prepared");
    let new = new_budget(&proposal.changes);
    let basis = new.basis.expect("computed");
    assert_eq!(new.amount, eur(5_000));
    assert_eq!(basis.first_day, Date::constant(2026, 9, 1));
    assert_eq!(basis.last_day, Date::constant(2026, 11, 30));
    assert_eq!(basis.left_out, None);
    // The last period alone is November with both payments: what was
    // spent in it, as before (D-669 moves only past an empty period).
    let proposal = based_on(&small, "last_period", today).expect("prepared");
    assert_eq!(new_budget(&proposal.changes).amount, eur(10_000));
}

#[test]
fn spending_that_stopped_is_not_taken_for_a_slip() {
    let small = slipped(Date::constant(2000, 1, 1));
    // On 14 March 2027 January and February are both empty: the count
    // doesn't move, and nothing was spent in the last month.
    let today = Date::constant(2027, 3, 14);
    assert_eq!(
        code(based_on(&small, "last_period", today)),
        "ai-tools.nothing-to-base-on nothing-spent"
    );
    // The refusal names the month it looked at, so the model can ask the
    // user for an amount instead (M6-B210).
    let report = based_on(&small, "last_period", today)
        .expect_err("refused")
        .model_report();
    let days: Vec<_> = report
        .params
        .iter()
        .filter(|(name, _)| *name == "from" || *name == "to")
        .map(|(_, value)| value.clone())
        .collect();
    assert_eq!(
        days,
        [
            kassensturz_core::ErrorParam::Date(Date::constant(2027, 2, 1)),
            kassensturz_core::ErrorParam::Date(Date::constant(2027, 2, 28)),
        ]
    );
    // December 2026 and January 2027 empty on 14 February: no move
    // either, the three months hold only November's 100.00.
    let today = Date::constant(2027, 2, 14);
    let proposal = based_on(&small, "average_3_periods", today).expect("prepared");
    let basis = new_budget(&proposal.changes).basis.expect("computed");
    assert_eq!(basis.left_out, None);
    assert_eq!(basis.spent, eur(10_000));
}

#[test]
fn the_count_moves_back_only_as_far_as_the_book_reaches() {
    // The book starts with the payment of 31 August: on 14 November the
    // count can't move back to July, so August to October are counted
    // as they are: 100.00 in three months, rounded up to 34.00 (a step
    // of one whole euro).
    let small = slipped(Date::constant(2026, 8, 1));
    let proposal = based_on(&small, "average_3_periods", TODAY).expect("prepared");
    let new = new_budget(&proposal.changes);
    let basis = new.basis.expect("computed");
    assert_eq!(basis.left_out, None);
    assert_eq!(basis.first_day, Date::constant(2026, 8, 1));
    assert_eq!(new.amount, eur(3_400));
    // The last period alone moves to September, which the book reaches.
    let proposal = based_on(&small, "last_period", TODAY).expect("prepared");
    assert_eq!(new_budget(&proposal.changes).amount, eur(5_000));
}

// A call only checked (US2.15, #595, D-674).
#[test]
fn a_check_names_the_computed_amount_and_how_it_was_computed() {
    let small = small();
    let (book, food) = (&small.book, small.food);
    let call = json!({
        "check_only": true,
        "new_categories": [{"key": "b", "name": "Bakery"}],
        "new_budgets": [{"category_id": food.to_string(), "every": "month",
            "based_on": "average_3_periods", "round_to": 10}],
    });
    let (check_only, call) = take_check_only(&call).expect("taken");
    assert!(check_only);
    assert!(
        call.get(CHECK_ONLY).is_none(),
        "the lists alone are checked"
    );
    let proposal = propose(book, &call).expect("checked");
    let answer = check_only_answer(book, &proposal, Permissions::all()).expect("answered");
    assert_eq!(answer["check_only"], json!(true));
    assert_eq!(answer["would_prepare"], json!(2));
    assert!(answer.get("prepared").is_none(), "nothing is prepared");
    assert_eq!(
        answer["lists"],
        json!({"new_categories": 1, "new_budgets": 1})
    );
    // 350.01 over August to October, 116.67 a month, rounded up to 120.
    assert_eq!(
        answer["budget_amounts"],
        json!([{
            "list": "new_budgets",
            "item": 1,
            "amount": "120.00",
            "currency": "EUR",
            "from": "2026-11-01",
            "based_on": {
                "periods": 3,
                "from": "2026-08-01",
                "to": "2026-10-31",
                "spent": "350.01",
                "average": "116.67",
                "rounded_up_to": 10,
            },
        }])
    );
    // No name from the book: only lists, counts, amounts and days.
    assert!(!answer.to_string().contains("Food"), "{answer}");
    assert!(!answer.to_string().contains("Bakery"), "{answer}");
}

#[test]
fn check_only_is_true_false_or_absent() {
    let call = json!({"new_categories": []});
    assert_eq!(
        take_check_only(&call).expect("absent"),
        (false, call.clone())
    );
    let (on, rest) =
        take_check_only(&json!({"check_only": false, "new_categories": []})).expect("false");
    assert!(!on);
    assert_eq!(rest, call);
    for bad in [json!("true"), json!(1), json!(null)] {
        assert!(matches!(
            take_check_only(&json!({"check_only": bad})),
            Err(ToolError::BadArgument {
                argument: CHECK_ONLY
            })
        ));
    }
}

// The check names the left-out period as the review card does (M6-B209).
#[test]
fn a_check_names_the_period_a_slipped_payment_left_out() {
    let small = slipped(Date::constant(2000, 1, 1));
    let proposal = based_on(&small, "last_period", TODAY).expect("prepared");
    let answer = check_only_answer(&small.book, &proposal, Permissions::all()).expect("answered");
    assert_eq!(
        answer["budget_amounts"][0]["based_on"],
        json!({
            "periods": 1,
            "from": "2026-09-01",
            "to": "2026-09-30",
            "spent": "50.00",
            "average": "50.00",
            "rounded_up_to": 1,
            "left_out": {"from": "2026-10-01", "to": "2026-10-31", "reason": "nothing-spent"},
        })
    );
    // Without a slip there is no left_out.
    let proposal = based_on(&small, "last_period", Date::constant(2026, 10, 14)).expect("prepared");
    let answer = check_only_answer(&small.book, &proposal, Permissions::all()).expect("answered");
    assert!(
        answer["budget_amounts"][0]["based_on"]
            .get("left_out")
            .is_none()
    );
}

/// A budget for one stretch of days covers the days the user named, from
/// the first to the last (D-688, which amends D-394): it is prepared, made
/// on confirm as the book's own range, read back by `list_budgets` with
/// those days, and one Undo takes it back.
#[test]
fn a_budget_for_a_stretch_of_days_covers_the_days_the_user_named() {
    let mut small = small();
    let fun = small.fun.to_string();
    let proposal = propose(
        &small.book,
        &json!({"new_budgets": [{"name": "Presents", "category_id": fun, "every": "range",
            "first_day": "2026-12-01", "last_day": "2026-12-24", "amount": "300.00"}]}),
    )
    .expect("prepared");
    let new = new_budget(&proposal.changes);
    assert_eq!(new.every, BudgetEvery::Range);
    assert_eq!(new.first_day, Date::constant(2026, 12, 1));
    assert_eq!(new.first_period_end, Date::constant(2026, 12, 24));
    assert_eq!(new.amount, eur(30_000));
    assert!(!new.carry_over);
    assert_eq!(
        proposal.changes.summary(),
        "new budget \"Presents\" for \"Fun\" from 2026-12-01 to 2026-12-24"
    );
    proposal
        .changes
        .apply(&mut small.book, Confirmed::by_the_user())
        .expect("applied");
    let budgets = small.book.budgets().expect("budgets");
    assert_eq!(budgets.len(), 1);
    assert_eq!(budgets[0].every, BudgetEvery::Range);
    assert_eq!(budgets[0].first_day, Date::constant(2026, 12, 1));
    assert_eq!(budgets[0].last_day, Some(Date::constant(2026, 12, 24)));
    assert_eq!(budgets[0].amounts.len(), 1);
    assert_eq!(budgets[0].amounts[0].amount, eur(30_000));
    assert!(!budgets[0].amounts[0].carry_over);

    // The AI reads it back with the days it covers.
    let listed = crate::call(&small.book, "list_budgets", &json!({}), TODAY)
        .expect("read")
        .value;
    let row = &listed["budgets"][0];
    assert_eq!(row["every"], "range", "{listed}");
    assert_eq!(row["starts"], "2026-12-01", "{listed}");
    assert_eq!(row["ends"], "2026-12-24", "{listed}");
    assert_eq!(row["amount"], "300.00", "{listed}");

    // The same category can't get another budget on those days, and the
    // refusal points to edit_budgets as for a repeating one.
    let again = json!({"new_budgets": [{"category_id": fun, "every": "range",
        "first_day": "2026-12-20", "last_day": "2027-01-06", "amount": "50.00"}]});
    assert_eq!(
        code(propose(&small.book, &again)),
        "ai-tools.has-budget edit_budgets"
    );
    // After its last day the category is free again.
    let after = json!({"new_budgets": [{"category_id": fun, "every": "range",
        "first_day": "2026-12-25", "last_day": "2027-01-06", "amount": "50.00"}]});
    assert_eq!(code(propose(&small.book, &after)), "prepared");

    small.book.undo().expect("undone");
    assert!(small.book.budgets().expect("budgets").is_empty());
}

/// A stretch of days may start before today when it reaches today or later,
/// and a budget over a stretch that is over already is refused: the AI
/// never prepares a budget only for the past (D-688).
#[test]
fn a_stretch_of_days_must_not_be_over_already() {
    let small = small();
    let fun = small.fun.to_string();
    let call = |first: &str, last: &str| {
        json!({"new_budgets": [{"category_id": fun, "every": "range",
            "first_day": first, "last_day": last, "amount": "10.00"}]})
    };
    // TODAY is 14 November 2026.
    let started = propose(&small.book, &call("2026-11-01", "2026-11-14")).expect("prepared");
    assert_eq!(
        new_budget(&started.changes).first_day,
        Date::constant(2026, 11, 1)
    );
    assert_eq!(
        code(propose(&small.book, &call("2026-11-01", "2026-11-13"))),
        "bad-argument last_day"
    );
    assert_eq!(
        code(propose(&small.book, &call("2026-12-01", "2026-12-01"))),
        "prepared",
        "a single day is a stretch too"
    );
}

/// The malformed calls a model may make for a stretch of days, each with
/// its fixed keyword (D-688).
#[test]
fn malformed_stretch_of_days_calls_are_refused_with_a_fixed_keyword() {
    let small = small();
    let food = small.food.to_string();
    let range = |fields: Value| {
        let mut item = json!({"category_id": food, "every": "range",
            "first_day": "2026-12-01", "last_day": "2026-12-24", "amount": "10.00"});
        for (key, value) in fields.as_object().expect("object") {
            if value.is_null() {
                item.as_object_mut().expect("object").remove(key);
            } else {
                item[key] = value.clone();
            }
        }
        json!({"new_budgets": [item]})
    };
    let cases: Vec<(Value, &str)> = vec![
        (range(json!({"first_day": null})), "bad-argument first_day"),
        (range(json!({"last_day": null})), "bad-argument last_day"),
        (
            range(json!({"first_day": "1 December"})),
            "bad-argument first_day",
        ),
        (
            range(json!({"first_day": "2026-12-1"})),
            "bad-argument first_day",
        ),
        (
            range(json!({"first_day": 20261201})),
            "bad-argument first_day",
        ),
        (
            range(json!({"last_day": "2026-02-30"})),
            "bad-argument last_day",
        ),
        (
            range(json!({"last_day": "2026-12-24T00:00"})),
            "bad-argument last_day",
        ),
        (
            range(json!({"first_day": "2026-12-24", "last_day": "2026-12-01"})),
            "store.budget-period ends-before-starts",
        ),
        (
            range(json!({"first_day": "2026-12-01", "last_day": "2127-12-31"})),
            "store.budget-period too-long",
        ),
        (
            range(json!({"carry_over": true})),
            "store.budget-period carry-over-on-range",
        ),
        (
            range(json!({"amount": null, "based_on": "last_period"})),
            "bad-argument based_on",
        ),
        (range(json!({"amount": null})), "bad-argument amount"),
        (range(json!({"salary_day": 25})), "bad-argument salary_day"),
        // A repeating budget starts with the current period: its days are
        // never named.
        (range(json!({"every": "month"})), "bad-argument first_day"),
        (
            range(json!({"every": "year", "first_day": null})),
            "bad-argument last_day",
        ),
    ];
    for (call, expected) in cases {
        assert_eq!(code(propose(&small.book, &call)), expected, "{call}");
    }
}
