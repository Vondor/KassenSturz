// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plans the AI prepares and changes (US7, B-357, D-814) on the five-year
//! Space Colony book as the KMyMoney importer, the pipeline and the store
//! fill it (AGENTS.md §9.4: tests where steps meet). The amount comes from
//! the user or from one of the book's own entries, never from the model;
//! nothing is written before the user confirms, and the forecast then
//! counts the plan.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{
    Confirmed, ProposedChange, ToolError, call, check_only_answer, propose, propose_on,
};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{AccountId, Date, Money, Report, ScheduleUnit, Transaction};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::{Action, Book, NewSchedule};
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

/// The book, its first checking or cash account, and the latest entry
/// that took money out of that account before today.
struct Fixture {
    book: Book,
    account: AccountId,
    entry: Transaction,
}

fn fixture() -> Fixture {
    let book = five_years();
    let account = *book
        .default_forecast_accounts()
        .expect("accounts")
        .first()
        .expect("a checking or cash account");
    let entry = book
        .transactions_of(account)
        .expect("entries")
        .into_iter()
        .filter(|entry| entry.booking_date() <= TODAY)
        .filter(|entry| {
            // One part in the account, as "Plan this again…" takes it.
            let own: Vec<_> = entry
                .splits()
                .iter()
                .filter(|split| split.account == Some(account))
                .collect();
            own.len() == 1 && own[0].amount.is_negative() && own[0].foreign.is_none()
        })
        .max_by_key(|entry| entry.booking_date())
        .expect("money went out of the account");
    Fixture {
        book,
        account,
        entry,
    }
}

fn plans_only() -> Permissions {
    Permissions::NONE
        .with(Permission::NewPlans, true)
        .with(Permission::EditPlans, true)
}

fn rent(account: AccountId) -> Value {
    json!({"new_plans": [{
        "name": "Rent",
        "account_id": account.to_string(),
        "direction": "out",
        "amount": "850.00",
        "unit": "month",
        "first_due": "2026-11-01",
        "paid_to": "Habitat Ring Housing",
    }]})
}

#[test]
fn plans_are_off_until_their_own_cells_are_on() {
    let f = fixture();
    for permission in [Permission::NewPlans, Permission::EditPlans] {
        assert!(!Permissions::defaults().allows(permission));
    }
    let off = Permissions::all().with(Permission::NewPlans, false);
    let error = propose_on(&f.book, &rent(f.account), off, TODAY).expect_err("not allowed");
    assert_eq!(error.model_report().code, "ai-tools.not-allowed");
    // The schema the model is offered then has no such list.
    let spec = kassensturz_ai_tools::propose_spec(off).expect("offered");
    assert!(!spec.parameters.contains("\"new_plans\""));
    assert!(spec.parameters.contains("\"edit_plans\""));
    // Either cell lets the AI read the transactions it takes amounts from.
    assert!(plans_only().allows(Permission::TransactionsRead));
}

#[test]
fn a_named_amount_becomes_a_plan_only_when_confirmed_and_the_forecast_counts_it() {
    let mut f = fixture();
    let forecast = |book: &Book| {
        call(book, "forecast", &json!({}), TODAY)
            .expect("forecast")
            .value
    };
    let before = forecast(&f.book);
    assert_eq!(before["has_plans"], json!(false));
    let proposal = propose_on(&f.book, &rent(f.account), plans_only(), TODAY).expect("prepared");
    let [ProposedChange::NewPlan(new)] = proposal.changes.changes() else {
        panic!("{:?}", proposal.changes.changes());
    };
    let currency = f.book.account(f.account).expect("account").currency;
    assert_eq!(new.plan.amount, Money::from_minor_units(-85_000, currency));
    assert_eq!(new.plan.unit, ScheduleUnit::Month);
    assert_eq!(new.plan.every, 1);
    assert_eq!(new.plan.anchor, Date::constant(2026, 11, 1));
    assert_eq!(new.plan.party_name.as_deref(), Some("Habitat Ring Housing"));
    assert_eq!(new.plan.party_iban, None);
    assert!(new.amount_from.is_none());
    // The review card shows the next three due days, the dialog's way.
    let next = new.next_due(TODAY, 3);
    assert_eq!(next.len(), 3);
    assert!(next[0] >= Date::constant(2026, 11, 1));
    // Nothing is written before the user confirms.
    assert!(f.book.schedules().expect("plans").is_empty());

    let history = f.book.history().expect("history").len();
    proposal
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    let plans = f.book.schedules().expect("plans");
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].name, "Rent");
    assert!(!plans[0].paused);
    let entries = f.book.history().expect("history");
    assert_eq!(entries.len(), history + 1, "one entry for the whole set");
    assert_eq!(entries[0].action, Action::ApplyAiChanges);
    assert_eq!(forecast(&f.book)["has_plans"], json!(true));

    f.book.undo().expect("undone").expect("a change");
    assert!(f.book.schedules().expect("plans").is_empty());
}

#[test]
fn an_amount_from_an_entry_is_the_one_the_entry_was_booked_with() {
    let f = fixture();
    let call = json!({"new_plans": [{
        "name": "Weekly shop",
        "account_id": f.account.to_string(),
        "direction": "out",
        "amount_from_transaction_id": f.entry.id().to_string(),
        "every": 2,
        "unit": "week",
        "first_due": "2026-11-02",
        "ends": "2027-06-30",
        "tolerance": "10.00",
    }]});
    let proposal = propose_on(&f.book, &call, plans_only(), TODAY).expect("prepared");
    let [ProposedChange::NewPlan(new)] = proposal.changes.changes() else {
        panic!("{:?}", proposal.changes.changes());
    };
    let booked = f
        .entry
        .splits()
        .iter()
        .find(|split| split.account == Some(f.account))
        .expect("the part")
        .amount;
    assert_eq!(new.plan.amount, booked);
    assert_eq!(new.amount_from, Some(f.entry.id()));
    assert_eq!(new.plan.ends_on, Some(Date::constant(2027, 6, 30)));
    assert_eq!(new.plan.every, 2);
    assert_eq!(new.plan.unit, ScheduleUnit::Week);
    // The entry takes money out, so it can't be the amount of money in.
    let mut call = call;
    call["new_plans"][0]["direction"] = json!("in");
    let error = propose_on(&f.book, &call, plans_only(), TODAY).expect_err("refused");
    let ToolError::InItem { cause, .. } = error else {
        panic!("{error:?}");
    };
    assert!(
        matches!(
            *cause,
            ToolError::BadArgument {
                argument: "amount_from_transaction_id"
            }
        ),
        "{cause:?}"
    );
}

#[test]
fn a_change_is_reviewed_applied_as_one_undo_and_is_out_of_date_when_the_plan_changed() {
    let mut f = fixture();
    let currency = f.book.account(f.account).expect("account").currency;
    let id = f
        .book
        .add_schedule(&NewSchedule {
            name: "Gym".into(),
            account: f.account,
            amount: Money::from_minor_units(-2_500, currency),
            tolerance: Money::zero(currency),
            category: None,
            party_iban: Some("DE89370400440532013000".into()),
            party_name: None,
            unit: ScheduleUnit::Month,
            every: 1,
            anchor: Date::constant(2026, 1, 5),
            track_from: Date::constant(2026, 1, 5),
            ends_on: None,
        })
        .expect("saved");
    let before = f.book.schedule(id).expect("stored");
    let call = json!({"edit_plans": [{
        "plan_id": id.to_string(),
        "amount": "29.90",
        "every": 3,
        "ends": "2027-12-31",
        "paused": true,
    }]});
    let first = propose_on(&f.book, &call, plans_only(), TODAY).expect("prepared");
    let second = propose_on(&f.book, &call, plans_only(), TODAY).expect("prepared");
    let [ProposedChange::EditPlan(edit)] = first.changes.changes() else {
        panic!("{:?}", first.changes.changes());
    };
    assert_eq!(edit.before, before);
    assert_eq!(edit.after.amount, Money::from_minor_units(-2_990, currency));
    assert_eq!(edit.after.every, 3);
    assert!(edit.after.paused);
    // What the plan remembers stays, and the model never saw it.
    assert_eq!(edit.after.party_iban, before.party_iban);
    assert_eq!(f.book.schedule(id).expect("stored"), before);

    first
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    let after = f.book.schedule(id).expect("stored");
    assert_eq!(after.amount, Money::from_minor_units(-2_990, currency));
    assert_eq!(after.ends_on, Some(Date::constant(2027, 12, 31)));
    assert!(after.paused);
    let error = second
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect_err("out of date");
    assert_eq!(error.report().code, "ai-changes.out-of-date");
    f.book.undo().expect("undone").expect("a change");
    assert_eq!(f.book.schedule(id).expect("stored"), before);

    // An empty end takes the end away.
    let call = json!({"edit_plans": [{"plan_id": id.to_string(), "ends": "2027-01-31"}]});
    propose_on(&f.book, &call, plans_only(), TODAY)
        .expect("prepared")
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    let call = json!({"edit_plans": [{"plan_id": id.to_string(), "ends": ""}]});
    propose_on(&f.book, &call, plans_only(), TODAY)
        .expect("prepared")
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(f.book.schedule(id).expect("stored").ends_on, None);
}

#[test]
fn a_check_only_call_counts_the_plans_and_sends_nothing() {
    let f = fixture();
    let proposal = propose(&f.book, &rent(f.account), plans_only()).expect("prepared");
    let answer = check_only_answer(&f.book, &proposal, plans_only()).expect("answer");
    assert_eq!(answer["check_only"], json!(true));
    assert_eq!(answer["lists"]["new_plans"], json!(1));
    assert!(f.book.schedules().expect("plans").is_empty());
}

#[test]
fn what_the_book_or_the_amount_rule_refuses_is_never_prepared() {
    let f = fixture();
    let account = f.account.to_string();
    let base = |extra: Value| {
        let mut item = json!({
            "name": "Rent",
            "account_id": account,
            "direction": "out",
            "amount": "850.00",
            "unit": "month",
            "first_due": "2026-11-01",
        });
        for (key, value) in extra.as_object().expect("object") {
            if value.is_null() {
                item.as_object_mut().expect("object").remove(key);
            } else {
                item[key] = value.clone();
            }
        }
        json!({"new_plans": [item]})
    };
    let cause = |call: &Value| -> String {
        let error = propose_on(&f.book, call, plans_only(), TODAY).expect_err("refused");
        match error {
            ToolError::InItem { cause, .. } => {
                cause.model_report().code.to_owned()
                    + &cause
                        .model_report()
                        .params
                        .iter()
                        .map(|(name, value)| format!(" {name}={value:?}"))
                        .collect::<String>()
            }
            other => other.model_report().code.to_owned(),
        }
    };
    for (extra, expected) in [
        // No amount at all, or both kinds: the model never fills one in.
        (
            json!({"amount": null}),
            "ai-tools.bad-argument argument=Keyword(\"amount\")",
        ),
        (
            json!({"amount_from_transaction_id": f.entry.id().to_string()}),
            "ai-tools.bad-argument argument=Keyword(\"amount\")",
        ),
        // A size, never a sign; never zero.
        (
            json!({"amount": "-850.00"}),
            "ai-tools.bad-argument argument=Keyword(\"amount\")",
        ),
        (
            json!({"amount": "0"}),
            "ai-tools.bad-argument argument=Keyword(\"amount\")",
        ),
        (
            json!({"amount": "85,00"}),
            "ai-tools.bad-argument argument=Keyword(\"amount\")",
        ),
        (
            json!({"direction": "sideways"}),
            "ai-tools.bad-argument argument=Keyword(\"direction\")",
        ),
        (
            json!({"unit": "day"}),
            "ai-tools.bad-argument argument=Keyword(\"unit\")",
        ),
        (
            json!({"every": 0}),
            "ai-tools.bad-argument argument=Keyword(\"every\")",
        ),
        (
            json!({"every": 53}),
            "ai-tools.bad-argument argument=Keyword(\"every\")",
        ),
        (
            json!({"first_due": "1.11.2026"}),
            "ai-tools.bad-argument argument=Keyword(\"first_due\")",
        ),
        (
            json!({"received_from": "Someone"}),
            "ai-tools.bad-argument argument=Keyword(\"received_from\")",
        ),
        (
            json!({"name": "  "}),
            "ai-tools.bad-argument argument=Keyword(\"name\")",
        ),
        (
            json!({"account_id": AccountId::generate().to_string()}),
            "ai-tools.unknown-account",
        ),
        (
            json!({"category_id": AccountId::generate().to_string()}),
            "ai-tools.unknown-category",
        ),
        // The book's own checks: as for a plan typed in the dialog.
        (
            json!({"name": "R".repeat(61)}),
            "store.schedule-invalid reason=Keyword(\"name\")",
        ),
        (
            json!({"ends": "2026-10-01"}),
            "store.schedule-invalid reason=Keyword(\"end-before-start\")",
        ),
        (
            json!({"tolerance": "850.00"}),
            "store.schedule-invalid reason=Keyword(\"tolerance-range\")",
        ),
    ] {
        assert_eq!(cause(&base(extra.clone())), expected, "{extra}");
    }
    // A plan the book doesn't have.
    let call = json!({"edit_plans": [{"plan_id": AccountId::generate().to_string(), "name": "X"}]});
    assert_eq!(cause(&call), "ai-tools.unknown-plan");
    assert!(f.book.schedules().expect("plans").is_empty());
}
