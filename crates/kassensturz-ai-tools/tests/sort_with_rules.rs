// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AI's "sort with all Rules" where the steps meet (AGENTS.md §9.4,
//! US2.9, D-682): the synthetic Space Colony book as the KMyMoney importer
//! and the pipeline fill it, a statement imported into it as a bank would
//! deliver it, a Rule the AI prepared and the user confirmed, and then the
//! sort with every Rule, held against the store's own "Apply all".

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{Confirmed, ProposedChange, propose};
use kassensturz_core::ai_switch::Permissions;
use kassensturz_core::{AccountKind, Transaction};
use kassensturz_import::{Camt053, Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::{Choices, fill_book_from, import_statements};
use kassensturz_store::{Action, ApplyRules, Book};
use serde_json::json;

fn space_colony() -> Book {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden/kmy-sqlite/space-colony-v15.sqlite");
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

/// The Space Colony book with its example statement imported into the
/// everyday account, as the import does it: its own run of the Rules sorts
/// what they fit, and two entries stay without a category.
fn with_statement() -> Book {
    let mut book = space_colony();
    let checking = book
        .accounts(false)
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == "Everyday Checking")
        .expect("Everyday Checking");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/books/examples/statements/space-colony.camt053.xml");
    let input = fs::read(&path).expect("the statement is in the repository");
    let Imported::Statements(statements) =
        import(&Camt053, &input, &Limits::default()).expect("the statement reads")
    else {
        panic!("a statement");
    };
    import_statements(
        &mut book,
        checking.id,
        &statements,
        &Choices::default(),
        None,
    )
    .expect("imported");
    book
}

fn unsorted(book: &Book) -> Vec<Transaction> {
    book.transactions()
        .expect("transactions")
        .into_iter()
        .filter(|t| t.splits().len() == 2 && t.splits().iter().any(|s| s.account.is_none()))
        .collect()
}

#[test]
fn a_rule_the_ai_made_sorts_what_the_import_left_and_nothing_else() {
    let mut book = with_statement();
    let left = unsorted(&book);
    let payees: Vec<_> = left.iter().filter_map(Transaction::payee).collect();
    assert_eq!(payees, ["Colony Pay Office", "Fusion Grid Utility"]);
    // Nothing to sort yet: the import's run sorted what the Rules fit.
    assert!(
        book.preview_apply_rules(ApplyRules::default())
            .expect("preview")
            .changed
            .is_empty()
    );

    // The AI prepares a Rule for the utility, and the user confirms it.
    let category = book
        .all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.kind == AccountKind::Expense && !account.closed)
        .expect("a spending category");
    let rule = json!({"new_rules": [{
        "name": "Power",
        "conditions": [{"field": "payee", "test": "contains", "text": "fusion grid"}],
        "category_id": category.id.to_string(),
    }]});
    propose(&book, &rule, Permissions::all())
        .expect("prepared")
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("the Rule is made");

    let everything_else: Vec<Transaction> = book
        .transactions()
        .expect("transactions")
        .into_iter()
        .filter(|t| t.payee() != Some("Fusion Grid Utility"))
        .collect();
    let history = book.history().expect("history").len();
    for sort_again in [false, true] {
        let call = json!({"sort_with_rules": [{"sort_again": sort_again}]});
        let proposal = propose(&book, &call, Permissions::all()).expect("prepared");
        let [ProposedChange::SortWithRules(sort)] = proposal.changes.changes() else {
            panic!("{:?}", proposal.changes.changes());
        };
        // The same run as "Apply all" on the Rules page (D-630, D-631).
        let apply_all = book
            .preview_apply_rules(ApplyRules {
                rule: None,
                resort: sort_again,
            })
            .expect("preview");
        assert_eq!(sort.changes, apply_all.changed);
        assert_eq!(sort.changes.len(), 1, "only the utility");
        assert_eq!(sort.changes[0].transaction, left[1].id());
        assert_eq!(sort.changes[0].booked_to, Some(category.id));
    }

    let proposal = propose(
        &book,
        &json!({"sort_with_rules": [{"sort_again": true}]}),
        Permissions::all(),
    )
    .expect("prepared");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("sorted");
    let sorted = book.transaction(left[1].id()).expect("utility");
    assert!(
        sorted
            .splits()
            .iter()
            .any(|split| split.account == Some(category.id)),
        "the utility is in the Rule's category"
    );
    // Nothing else changed: not what KMyMoney's payees or the import's run
    // sorted, not what the user sorted by hand, not the salary entry no
    // Rule fits.
    let after: Vec<Transaction> = book
        .transactions()
        .expect("transactions")
        .into_iter()
        .filter(|t| t.payee() != Some("Fusion Grid Utility"))
        .collect();
    assert_eq!(after, everything_else);
    let entries = book.history().expect("history");
    assert_eq!(entries.len(), history + 1);
    assert_eq!(entries[0].action, Action::ApplyAiChanges);

    book.undo().expect("undone");
    assert_eq!(
        book.transaction(left[1].id()).expect("utility"),
        left[1],
        "one Undo takes the sort back"
    );
}
