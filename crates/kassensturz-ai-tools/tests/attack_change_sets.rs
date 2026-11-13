// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenarios B1 and B3 of the outside security review: a change set
//! a manipulated AI prepares to erase a lot at once, and look-alike
//! characters in names and Rule conditions. All data is made up (AGENTS.md
//! §3.1 rule 2).
//!
//! What a review card should say about such a set (a one-line danger
//! warning, a warning on mixed alphabets) has no data in the change set yet;
//! the tests here check what exists today: nothing changes before the
//! confirm, the limits per list, and that Undo restores everything.

use kassensturz_ai_tools::{Confirmed, propose};
use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::{Action, Book};
use serde_json::{Value, json};

/// Everything allowed, nothing locked: the worst case of the grid.
fn open() -> Permissions {
    Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false))
}

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").expect("valid code")
}

fn account(name: &str, kind: AccountKind) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: eur(),
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    }
}

/// A book with `count` transactions, each with a note the user wrote, and
/// a category "Rent".
fn book_with_notes(count: usize) -> (Book, Vec<TransactionId>) {
    let mut book = Book::create_in_memory().expect("a book");
    book.use_currency(eur()).expect("currency used");
    let checking = account("Checking", AccountKind::Asset);
    let rent = account("Rent", AccountKind::Expense);
    book.add_account(&checking).expect("added");
    book.add_account(&rent).expect("added");
    let mut ids = Vec::new();
    for n in 0..count {
        let day = i8::try_from(n % 28 + 1).expect("a day");
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::new(2026, 10, day).expect("valid date"),
            Some("Corner Shop".into()),
            vec![
                Split::new(checking.id, Money::from_minor_units(-100, eur())),
                Split::new(rent.id, Money::from_minor_units(100, eur())),
            ],
        )
        .expect("balanced")
        .with_note(Some(format!("My own note {n}")));
        book.add_transaction(&transaction).expect("added");
        ids.push(transaction.id());
    }
    (book, ids)
}

fn notes(book: &Book) -> Vec<Option<String>> {
    book.transactions()
        .expect("read")
        .iter()
        .map(|t| t.note().map(str::to_owned))
        .collect()
}

fn clear_notes(ids: &[TransactionId]) -> Value {
    json!({"edit_transactions": ids
        .iter()
        .map(|id| json!({"transaction_id": id.to_string(), "note": ""}))
        .collect::<Vec<_>>()})
}

#[test]
fn b1_a_set_that_empties_100_notes_waits_for_the_confirm_and_undo_restores_all() {
    let (mut book, ids) = book_with_notes(100);
    let before = notes(&book);
    let proposal = propose(&book, &clear_notes(&ids), open()).expect("prepared");
    assert_eq!(notes(&book), before, "nothing changes before the confirm");
    assert_eq!(proposal.changes.changes().len(), 100);

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert!(
        notes(&book).iter().all(Option::is_none),
        "the notes are gone"
    );
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::ApplyAiChanges,
        "the whole set is one change"
    );

    book.undo().expect("undone").expect("a change");
    assert_eq!(notes(&book), before, "one Undo brings every note back");
}

#[test]
#[ignore = "attack B1: the 100-item limit of edit_transactions is only in the schema; reported, waiting for a bug number"]
fn b1_more_than_100_edits_in_one_set_are_refused() {
    let (book, ids) = book_with_notes(101);
    let before = notes(&book);
    assert!(propose(&book, &clear_notes(&ids), open()).is_err());
    assert_eq!(notes(&book), before);
}

#[test]
fn b1_more_than_200_changes_in_one_set_are_refused() {
    let (book, ids) = book_with_notes(201);
    let before = notes(&book);
    assert!(propose(&book, &clear_notes(&ids), open()).is_err());
    assert_eq!(notes(&book), before);
}

#[test]
fn b1_a_locked_note_cannot_be_emptied() {
    let (book, ids) = book_with_notes(3);
    let locked = open().with_lock(Field::Note, true);
    assert!(propose(&book, &clear_notes(&ids), locked).is_err());
}

#[test]
#[ignore = "attack B3: look-alike names are not checked yet; reported, waiting for a bug number"]
fn b3_a_new_category_that_looks_like_an_existing_one_is_refused() {
    let (book, _) = book_with_notes(1);
    // "Rent" with a Cyrillic "е" (U+0435).
    let twin = "R\u{0435}nt";
    let result = propose(
        &book,
        &json!({"new_categories": [{"key": "twin", "name": twin}]}),
        open(),
    );
    assert!(
        result.is_err(),
        "a category {twin:?} next to \"Rent\" was prepared without a word"
    );
}

#[test]
#[ignore = "attack B3: mixed alphabets in Rule conditions are not checked yet; reported, waiting for a bug number"]
fn b3_a_rule_condition_with_mixed_alphabets_is_refused() {
    let (book, _) = book_with_notes(1);
    let rent = book
        .categories()
        .expect("categories")
        .into_iter()
        .find(|entry| entry.category.name == "Rent")
        .expect("Rent")
        .category
        .id;
    // "Shop" with a Cyrillic "о" (U+043E): it silently never matches.
    let result = propose(
        &book,
        &json!({"new_rules": [{
            "name": "Shop",
            "conditions": [{"field": "payee", "test": "contains", "text": "Sh\u{043E}p"}],
            "category_id": rent.to_string(),
        }]}),
        open(),
    );
    assert!(
        result.is_err(),
        "a Rule condition mixing Latin and Cyrillic was prepared without a word"
    );
}
