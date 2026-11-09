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
        Err(ToolError::BadArgument { argument }) => format!("bad-argument {argument}"),
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
        "store.budget-overlap"
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

/// A budget item with a key it doesn't take, such as a name, is refused
/// with the list, the item and the keys it does take; the model's key is
/// never repeated back (M6-B201, X-8).
#[test]
fn an_unknown_budget_key_lists_the_keys_a_budget_takes() {
    let small = small();
    let food = small.food.to_string();
    let call = json!({"new_budgets": [
        {"category_id": food, "every": "month", "amount": "10"},
        {"category_id": food, "every": "year", "amount": "10", "name": "Groceries cap"}
    ]});
    let error = propose(&small.book, &call).expect_err("refused");
    let allowed = error.allowed_fields().expect("the keys it takes");
    assert!(allowed.contains(&"category_ids") && allowed.contains(&"every"));
    assert!(!allowed.contains(&"name"));
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
    assert!(!shown.contains("\"name\""), "{shown}");
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
        (
            budget(json!({"category_id": food, "every": "range", "amount": "1"})),
            "bad-argument every",
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
            "bad-argument based_on",
        ),
        // No full year before 2026 in the book.
        (
            budget(json!({"category_id": food, "every": "year", "based_on": "last_period"})),
            "bad-argument based_on",
        ),
        // Fun spent nothing: nothing to base an amount on.
        (
            budget(json!({"category_id": fun, "every": "month", "based_on": "last_period"})),
            "bad-argument based_on",
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
        (
            budget(
                json!({"category_id": food, "every": "month", "amount": "10", "first_day": "2026-01-01"}),
            ),
            "unknown-field new_budgets",
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
        "store.budget-overlap"
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
