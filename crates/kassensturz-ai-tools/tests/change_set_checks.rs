// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Change sets whose confirm-time checks, danger line or own consistency
//! must hold what the proposal promised (B-336, B-337, B-344, found by the
//! code audit of 2026-11-15). Synthetic book only.

#![cfg(feature = "ai")]

use kassensturz_ai_tools::{ProposedChange, propose};
use kassensturz_core::Report as _;
use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Split, Transaction,
    TransactionId,
};
use kassensturz_store::Book;
use serde_json::json;

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").unwrap()
}

/// Everything allowed, nothing locked.
fn open() -> Permissions {
    Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false))
}

fn account(name: &str, kind: AccountKind, account_type: Option<AccountType>) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: eur(),
        account_type,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

struct Sample {
    book: Book,
    cash: Account,
    food: Account,
    /// A cash payment the user typed, with a payee and a note.
    noted_cash: TransactionId,
    /// A cash payment split over two categories, each part with a note.
    split_cash: TransactionId,
    /// A cash payment without a category yet.
    unsorted: TransactionId,
}

fn sample() -> Sample {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(eur()).unwrap();
    let cash = account("Wallet", AccountKind::Asset, Some(AccountType::Cash));
    let food = account("Food", AccountKind::Expense, None);
    let fun = account("Fun", AccountKind::Expense, None);
    let empty = account("Unused", AccountKind::Expense, None);
    for a in [&cash, &food, &fun, &empty] {
        book.add_account(a).unwrap();
    }
    let day = Date::constant(2026, 10, 20);
    let m = |cents| Money::from_minor_units(cents, eur());

    let noted_cash = TransactionId::generate();
    book.add_transaction(
        &Transaction::new(
            noted_cash,
            day,
            Some("Corner Bakery".into()),
            vec![Split::new(cash.id, m(-450)), Split::new(food.id, m(450))],
        )
        .unwrap()
        .with_note(Some("birthday cake for the neighbours".into())),
    )
    .unwrap();

    let split_cash = TransactionId::generate();
    let mut a = Split::new(food.id, m(700));
    a.note = Some("groceries, receipt 0815".into());
    let mut b = Split::new(fun.id, m(300));
    b.note = Some("cinema ticket for Mia".into());
    book.add_transaction(
        &Transaction::new(
            split_cash,
            day,
            Some("Market Hall".into()),
            vec![Split::new(cash.id, m(-1000)), a, b],
        )
        .unwrap(),
    )
    .unwrap();

    let unsorted = TransactionId::generate();
    book.add_transaction(
        &Transaction::new(
            unsorted,
            day,
            Some("Kiosk".into()),
            vec![Split::new(cash.id, m(-200)), Split::uncategorized(m(200))],
        )
        .unwrap(),
    )
    .unwrap();

    Sample {
        book,
        cash,
        food,
        noted_cash,
        split_cash,
        unsorted,
    }
}

/// D-239: "turning a permission off also stops what was prepared earlier".
/// An edit of a payee is refused at confirm once the user locks the payee
/// field, but a new cash transaction with a payee, note and category is
/// not: `check_allowed` only looks at `adds_allowed` (account, date,
/// amount), while the proposal refused locked payee/note/category.
#[test]
fn a_new_transaction_is_checked_again_against_the_payee_note_and_category_locks() {
    let s = sample();
    let set = propose(
        &s.book,
        &json!({"new_transactions": [{
            "account_id": s.cash.id.to_string(),
            "date": "2026-10-21",
            "amount": "-3.20",
            "payee": "Ice Cream Van",
            "note": "a note the user did not want the AI to write",
            "category_id": s.food.id.to_string(),
        }]}),
        open(),
    )
    .expect("prepared while nothing is locked")
    .changes;

    // The user now locks the payee, the note and the category against the AI.
    let later = open()
        .with_lock(Field::Payee, true)
        .with_lock(Field::Note, true)
        .with_lock(Field::Category, true);

    // The same call made now is refused...
    assert!(
        propose(
            &s.book,
            &json!({"new_transactions": [{
                "account_id": s.cash.id.to_string(),
                "date": "2026-10-21",
                "amount": "-3.20",
                "payee": "Ice Cream Van",
            }]}),
            later,
        )
        .is_err(),
        "a locked payee is refused when proposed"
    );
    // ...and an edit of a payee prepared earlier is stopped at confirm...
    let edit = propose(
        &s.book,
        &json!({"edit_transactions": [{
            "transaction_id": s.unsorted.to_string(),
            "payee": "Kiosk at the station",
        }]}),
        open(),
    )
    .unwrap()
    .changes;
    assert!(edit.check_allowed(later).is_err());

    // ...and so is the new transaction prepared earlier, which would write
    // the locked payee, note and category (B-336).
    assert!(
        set.check_allowed(later).is_err(),
        "a new transaction that writes locked fields passes the confirm-time check"
    );
    // Without them it still may be added.
    assert!(set.check_allowed(open()).is_ok());
}

/// B-337 (extends B-248): a set that deletes a cash transaction with the
/// user's own payee and note, or that joins a split whose parts carry the
/// user's notes back into one unnamed part, erases those texts, so it gets
/// the danger line as an edit that empties a note does.
#[test]
fn erasing_a_users_notes_by_delete_or_join_gets_the_danger_line() {
    let s = sample();
    let delete = propose(
        &s.book,
        &json!({"delete_transactions": [{"transaction_id": s.noted_cash.to_string()}]}),
        open(),
    )
    .unwrap()
    .changes;
    assert!(matches!(
        delete.changes()[0],
        ProposedChange::DeleteTransaction(_)
    ));

    let join = propose(
        &s.book,
        &json!({"split_transactions": [{
            "transaction_id": s.split_cash.to_string(),
            "parts": [{"category_id": s.food.id.to_string(), "amount": "-10.00"}],
        }]}),
        open(),
    )
    .unwrap()
    .changes;

    // For comparison: emptying the same note with an edit carries the line.
    let edit = propose(
        &s.book,
        &json!({"edit_transactions": [{
            "transaction_id": s.noted_cash.to_string(),
            "note": "",
        }]}),
        open(),
    )
    .unwrap()
    .changes;
    assert!(edit.danger().is_some_and(|d| d.texts_lost == 1));

    let lost_by_join = join.danger().map_or(0, |d| d.texts_lost);
    let lost_by_delete = delete.danger().map_or(0, |d| d.texts_lost);
    assert!(
        lost_by_join == 2 && lost_by_delete == 2,
        "texts lost by join: {lost_by_join} (2 part notes), by delete: {lost_by_delete} \
         (payee and note): no danger line"
    );
}

/// B-344: a set that sorts a transaction into a category and deletes that
/// category could never be applied, so it is refused when proposed, as a
/// category in use, instead of being offered for review.
#[test]
fn a_set_that_books_into_a_category_it_deletes_is_refused_at_once() {
    let s = sample();
    let unused = s
        .book
        .categories()
        .unwrap()
        .into_iter()
        .find(|entry| entry.category.name == "Unused")
        .unwrap()
        .category;
    let proposed = propose(
        &s.book,
        &json!({
            "categorize": [{"transaction_id": s.unsorted.to_string(), "category_id": unused.id.to_string()}],
            "delete_categories": [{"category_id": unused.id.to_string()}],
        }),
        open(),
    );
    assert_eq!(
        proposed
            .map(|_| ())
            .map_err(|error| error.kind().report().code),
        Err("ai-tools.category-in-use")
    );
    // Deleting it alone is fine: nothing uses it.
    assert!(
        propose(
            &s.book,
            &json!({"delete_categories": [{"category_id": unused.id.to_string()}]}),
            open(),
        )
        .is_ok()
    );
}
