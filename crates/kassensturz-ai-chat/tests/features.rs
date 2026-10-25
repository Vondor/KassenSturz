// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in features (M5.9, D-221 to D-223): each button's question is
//! answered by the fake model through the real chat, functions, number
//! check and change sets, with the Space Colony book plus entries that have
//! no category, added with the store's own operations.

#![cfg(feature = "ai")]

mod common;

use common::{TODAY, settings, space_colony};
use kassensturz_ai::KeyUse;
use kassensturz_ai_chat::{
    BuildInfo, ChatSession, DEMO_CATEGORY, DEMO_LABEL, DemoModel, Feature, LogEntry, NoHooks,
    demo_provider,
};
use kassensturz_ai_tools::{Confirmed, ProposedChange};
use kassensturz_core::{Date, Money, Split, Transaction, TransactionId};
use kassensturz_store::Book;

fn session() -> ChatSession {
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
}

/// The book with `count` entries that have no category, in its first open
/// account.
fn book_with_open_entries(count: i64) -> Book {
    let mut book = space_colony();
    let account = book
        .accounts(false)
        .unwrap()
        .into_iter()
        .next()
        .expect("an account");
    for number in 1..=count {
        let amount = Money::from_minor_units(-number * 1_000, account.currency);
        let opposite = Money::from_minor_units(number * 1_000, account.currency);
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 10 + number as i8),
            Some(format!("Open payee {number}")),
            vec![
                Split::new(account.id, amount),
                Split::uncategorized(opposite),
            ],
        )
        .unwrap();
        book.add_transaction(&transaction).unwrap();
    }
    book
}

fn open_entries(book: &Book) -> usize {
    book.transactions()
        .unwrap()
        .iter()
        .filter(|transaction| {
            transaction
                .splits()
                .iter()
                .any(|split| split.account.is_none())
        })
        .count()
}

#[test]
fn the_category_button_prepares_a_set_for_the_open_entries_and_undo_takes_it_back() {
    let mut book = book_with_open_entries(2);
    let before = book.transactions().unwrap();
    assert_eq!(open_entries(&book), 2);
    let question = Feature::SuggestCategories.question(TODAY, None).unwrap();

    let provider = demo_provider(settings(true, None, KeyUse::None), DemoModel::Functions);
    let answer = session()
        .ask(&book, &provider, None, &question, TODAY, &mut NoHooks)
        .unwrap();

    assert!(answer.text.starts_with(DEMO_LABEL), "{}", answer.text);
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    let [changes] = <[_; 1]>::try_from(answer.changes).expect("one set");
    let categorized = changes
        .changes()
        .iter()
        .filter(|change| matches!(change, ProposedChange::Categorize(_)))
        .count();
    assert_eq!(categorized, 2, "only the entries without a category");
    assert!(
        matches!(&changes.changes()[0], ProposedChange::NewCategory(c) if c.path == [DEMO_CATEGORY])
    );
    // Nothing changed before the user's confirm.
    assert_eq!(book.transactions().unwrap(), before);

    changes.apply(&mut book, Confirmed::by_the_user()).unwrap();
    assert_eq!(open_entries(&book), 0);
    book.undo().unwrap();
    assert_eq!(book.transactions().unwrap(), before);
}

#[test]
fn the_category_button_for_one_account_names_it_and_a_book_with_none_open_prepares_nothing() {
    let book = space_colony();
    assert_eq!(open_entries(&book), 0);
    let account = book.accounts(false).unwrap().remove(0);
    let question = Feature::SuggestCategories
        .question(TODAY, Some(&account.id.to_string()))
        .unwrap();
    let provider = demo_provider(settings(true, None, KeyUse::None), DemoModel::Functions);
    let answer = session()
        .ask(&book, &provider, None, &question, TODAY, &mut NoHooks)
        .unwrap();
    assert!(answer.changes.is_empty());
}

#[test]
fn the_month_button_is_answered_with_figures_that_came_from_a_function() {
    let book = space_colony();
    let question = Feature::ExplainMonth.question(TODAY, None).unwrap();
    let provider = demo_provider(settings(true, None, KeyUse::None), DemoModel::Functions);
    let mut chat = session();
    let answer = chat
        .ask(&book, &provider, None, &question, TODAY, &mut NoHooks)
        .unwrap();
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    assert!(answer.changes.is_empty());
    let called: Vec<&str> = chat
        .log()
        .iter()
        .filter_map(|entry| match entry {
            LogEntry::Called { function, .. } => Some(function.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(called, ["category_totals"]);
}

#[test]
fn the_rules_button_asks_for_the_rules_the_book_already_has() {
    let book = space_colony();
    let question = Feature::PrepareRules.question(TODAY, None).unwrap();
    let provider = demo_provider(settings(true, None, KeyUse::None), DemoModel::Functions);
    let mut chat = session();
    let answer = chat
        .ask(&book, &provider, None, &question, TODAY, &mut NoHooks)
        .unwrap();
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    assert!(
        chat.log().iter().any(
            |entry| matches!(entry, LogEntry::Called { function, .. } if function == "list_rules")
        ),
        "{:?}",
        chat.log()
    );
}
