// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Confirmation races (D-787, US7 Phase 3 round 1, section C): a set the
//! user confirms after the book changed underneath it, the same set
//! confirmed twice, and two sets prepared from the same state and confirmed
//! one after the other. The rule is one: a set is applied only to the book
//! it was checked against, and a set that no longer fits changes nothing.

use super::*;

/// A call that gives the unsorted purchase the category `Pets`, made new.
fn pets_for(book: &Book, transaction: TransactionId) -> Proposal {
    propose(
        book,
        &json!({
            "new_categories": [{"key": "pets", "name": "Pets"}],
            "categorize": [{"transaction_id": transaction.to_string(), "new_category": "pets"}],
        }),
    )
    .expect("prepared")
}

#[test]
fn a_set_confirmed_twice_is_applied_once() {
    let Sample {
        mut book, unsorted, ..
    } = sample();
    let set = pets_for(&book, unsorted).changes;
    set.clone()
        .apply(&mut book, Confirmed::by_the_user())
        .expect("the first confirm applies it");
    let after_first = state(&book);
    let error = set
        .apply(&mut book, Confirmed::by_the_user())
        .expect_err("a second confirm of the same set is refused");
    assert!(
        matches!(error, ChangeError::OutOfDate | ChangeError::NotApplied(_)),
        "{error:?}"
    );
    assert_eq!(
        state(&book),
        after_first,
        "the second confirm adds no second Pets category and no history entry"
    );
}

#[test]
fn two_sets_for_one_entry_the_second_is_out_of_date() {
    let Sample {
        mut book,
        groceries,
        unsorted,
        ..
    } = sample();
    let to_pets = pets_for(&book, unsorted).changes;
    let to_groceries = propose(
        &book,
        &json!({"categorize": [{
            "transaction_id": unsorted.to_string(),
            "category_id": groceries.to_string(),
        }]}),
    )
    .expect("prepared")
    .changes;
    // Confirmed in the wrong order: the later question first.
    to_groceries
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let before = state(&book);
    let error = to_pets
        .apply(&mut book, Confirmed::by_the_user())
        .expect_err("the entry changed since this set was prepared");
    assert!(matches!(error, ChangeError::OutOfDate), "{error:?}");
    assert_eq!(state(&book), before, "nothing of the second set is applied");
}

#[test]
fn two_sets_for_different_entries_apply_in_either_order() {
    for first_is_unsorted in [true, false] {
        let Sample {
            mut book,
            groceries,
            unsorted,
            bought,
            ..
        } = sample();
        let for_unsorted = propose(
            &book,
            &json!({"categorize": [{
                "transaction_id": unsorted.to_string(),
                "category_id": groceries.to_string(),
            }]}),
        )
        .expect("prepared")
        .changes;
        let for_other = to_food(&book, bought);
        let (first, second) = if first_is_unsorted {
            (for_unsorted, for_other)
        } else {
            (for_other, for_unsorted)
        };
        first
            .apply(&mut book, Confirmed::by_the_user())
            .expect("applied");
        second
            .apply(&mut book, Confirmed::by_the_user())
            .expect("a set about other entries is still right");
    }
}

/// A set that moves `transaction` to the category Food.
fn to_food(book: &Book, transaction: TransactionId) -> ChangeSet {
    propose(
        book,
        &json!({"categorize": [{
            "transaction_id": transaction.to_string(),
            "category_id": book_food(book),
        }]}),
    )
    .expect("prepared")
    .changes
}

#[test]
fn a_set_for_an_entry_removed_meanwhile_changes_nothing() {
    let Sample {
        mut book,
        transfer,
        unsorted,
        ..
    } = sample();
    let set = pets_for(&book, unsorted).changes;
    // Something else in the book goes, then the very entry the set is about.
    book.remove_transaction(transfer).expect("removed");
    book.remove_transaction(unsorted).expect("removed");
    let before = state(&book);
    let error = set
        .apply(&mut book, Confirmed::by_the_user())
        .expect_err("the entry is gone");
    assert!(matches!(error, ChangeError::OutOfDate), "{error:?}");
    assert_eq!(state(&book), before, "not even the new category is added");
}

#[test]
fn a_set_confirmed_again_after_undo_leaves_the_book_consistent() {
    let Sample {
        mut book, unsorted, ..
    } = sample();
    let set = pets_for(&book, unsorted).changes;
    set.clone()
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    book.undo().expect("undone");
    // The user pressed confirm on a stale screen after undoing: whether the
    // book takes the set again or refuses it, it stays consistent: one
    // category called Pets at most, and no entry lost.
    let _ = set.apply(&mut book, Confirmed::by_the_user());
    let pets = book
        .all_accounts()
        .expect("accounts")
        .iter()
        .filter(|account| account.name == "Pets")
        .count();
    assert!(pets <= 1, "{pets} categories called Pets");
    assert_eq!(
        book.transactions().expect("transactions").len(),
        4,
        "no entry was lost"
    );
}
