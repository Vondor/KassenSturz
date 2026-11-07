// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! "Apply now" and "Apply all" (US1.7, D-630, D-631) on what the real
//! importers make: the Space Colony example book read from its KMyMoney
//! file, with its Rules from the payees, and its CAMT.053 statement
//! imported through the real pipeline. Rules added afterwards sort what the
//! import left without a category as the import would have, never what the
//! book's owner sorted by hand (D-031), and one undo takes the whole run
//! back. All data is made
//! up (AGENTS.md §3.1 rule 2).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use kassensturz_core::{
    AccountId, Actions, BookTo, Condition, MatchMode, Rule, RuleId, RuleNode, Transaction,
    TransactionId,
};
use kassensturz_import::{Camt053, Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::{Choices, fill_book_from, import_statements};
use kassensturz_store::{Action, ApplyRules, Book};

fn books() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kassensturz-import/tests/books")
}

fn account_of(book: &Book, name: &str) -> AccountId {
    book.all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == name)
        .unwrap_or_else(|| panic!("no account {name}"))
        .id
}

fn all(book: &Book) -> HashMap<TransactionId, Transaction> {
    book.transactions()
        .expect("transactions")
        .into_iter()
        .map(|transaction| (transaction.id(), transaction))
        .collect()
}

/// Whether the Rules may sort `transaction` without a re-sort: two parts,
/// one of them without a category yet.
fn open(transaction: &Transaction) -> bool {
    transaction.splits().len() == 2
        && transaction
            .splits()
            .iter()
            .filter(|split| split.account.is_none())
            .count()
            == 1
}

/// The Space Colony book with its CAMT.053 statement imported into its
/// checking account, as a tester has it after card 5 of the tester kit.
/// Without `rules`, the book's Rules are taken out before the import, as if
/// the user made them only afterwards; they are returned to add back.
fn space_colony_after_import(rules: bool) -> (Book, AccountId, Vec<TransactionId>, Vec<RuleNode>) {
    let bytes = fs::read(books().join("space-colony-5y-v15.sqlite")).expect("the KMyMoney book");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    let checking = account_of(&book, "Everyday Checking");
    let tree = book.rules().expect("rules");
    if !rules {
        for node in &tree {
            book.remove_rule(node.rule.id).expect("removed");
        }
    }
    let bytes = fs::read(books().join("examples/statements/space-colony.camt053.xml"))
        .expect("the statement");
    let Imported::Statements(statements) =
        import(&Camt053, &bytes, &Limits::default()).expect("the statement reads")
    else {
        panic!("a statement");
    };
    let report = import_statements(&mut book, checking, &statements, &Choices::default(), None)
        .expect("imported");
    (book, checking, report.imported(), tree)
}

/// Adds `nodes` with their sub-Rules, in their order.
fn add_back(book: &mut Book, nodes: &[RuleNode], parent: Option<RuleId>) {
    for node in nodes {
        book.add_rule(&node.rule, parent).expect("added");
        add_back(book, &node.sub_rules, Some(node.rule.id));
    }
}

/// The names of the categories `transaction`'s parts are in, sorted.
fn categories(book: &Book, transaction: TransactionId) -> Vec<String> {
    let mut names: Vec<String> = book
        .transaction(transaction)
        .expect("read")
        .splits()
        .iter()
        .filter_map(|split| split.account)
        .map(|id| book.account(id).expect("account"))
        .filter(|account| account.kind.is_category())
        .map(|account| account.name)
        .collect();
    names.sort();
    names
}

#[test]
fn rules_made_after_an_import_sort_it_as_they_would_have_during_it() {
    // What the import sorts while the book's Rules exist.
    let (sorted_at_import, _, new_with_rules, _) = space_colony_after_import(true);
    // The same import before the Rules exist leaves every line open.
    let (mut book, _, new, tree) = space_colony_after_import(false);
    assert_eq!(new.len(), new_with_rules.len());
    let before = all(&book);
    assert!(new.iter().all(|id| open(&before[id])));
    assert!(!tree.is_empty());
    add_back(&mut book, &tree, None);
    let changes = book.history().expect("history").len();

    let preview = book
        .preview_apply_rules(ApplyRules::default())
        .expect("preview");
    let sorted: HashSet<TransactionId> = preview.changed.iter().map(|c| c.transaction).collect();
    // Only what had no category yet.
    assert!(sorted.iter().all(|id| open(&before[id])), "{sorted:?}");
    assert!(!sorted.is_empty());
    assert_eq!(all(&book), before, "a preview changes nothing");
    assert_eq!(book.history().expect("history").len(), changes);

    // "Apply all" sorts what the preview said, as one user action, and the
    // new lines end up as the import would have sorted them.
    let run = book.apply_rules(ApplyRules::default()).expect("applied");
    let applied: HashSet<TransactionId> = run.changed.iter().map(|c| c.transaction).collect();
    assert_eq!(applied, sorted);
    assert_eq!(book.history().expect("history").len(), changes + 1);
    assert_eq!(
        book.change_to_undo().expect("read").map(|c| c.action),
        Some(Action::RunRules)
    );
    for (id, at_import) in new.iter().zip(&new_with_rules) {
        assert_eq!(
            categories(&book, *id),
            categories(&sorted_at_import, *at_import),
            "{id:?}"
        );
    }
    // Everything else, the owner's own sorting included, stays as it was.
    let after = all(&book);
    for (id, transaction) in &before {
        if !sorted.contains(id) {
            assert_eq!(&after[id], transaction, "{id:?} stays as it was");
        }
    }

    // One undo takes the whole run back.
    book.undo().expect("undone");
    assert_eq!(all(&book), before);
}

#[test]
fn sorting_again_takes_only_what_rules_sorted_and_the_user_left_alone() {
    let (mut book, checking, new, _) = space_colony_after_import(true);
    let parts: HashMap<_, TransactionId> = all(&book)
        .values()
        .flat_map(|transaction| {
            transaction
                .splits()
                .iter()
                .map(|split| (split.id, transaction.id()))
        })
        .collect();
    let marked: HashSet<TransactionId> = book
        .rule_marks()
        .expect("marks")
        .into_iter()
        .map(|mark| parts[&mark.split])
        .collect();
    assert!(
        new.iter().any(|id| marked.contains(id)),
        "the book's own Rules sorted some of the new lines"
    );
    let before = all(&book);

    // A Rule moved to the top would decide everything in the account now.
    let food = account_of(&book, "Food");
    let everything = Rule {
        id: RuleId::generate(),
        name: "Everything in checking".into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Account(checking)],
        actions: Actions {
            book_to: Some(BookTo::Category(food)),
            payee: None,
        },
    };
    book.add_rule(&everything, None).expect("added");
    book.move_rule(everything.id, None, 0).expect("first");

    let plain = book
        .preview_apply_rules(ApplyRules::default())
        .expect("preview");
    let resort = ApplyRules {
        rule: None,
        resort: true,
    };
    let again = book.apply_rules(resort).expect("applied");
    let plain: HashSet<TransactionId> = plain.changed.iter().map(|c| c.transaction).collect();
    let again: HashSet<TransactionId> = again.changed.iter().map(|c| c.transaction).collect();
    // The switch adds what Rules sorted before; all of it was either open
    // or sorted by a Rule, never by hand.
    assert!(plain.is_subset(&again));
    assert!(again.iter().any(|id| marked.contains(id)));
    assert!(
        again
            .iter()
            .all(|id| open(&before[id]) || marked.contains(id)),
        "{again:?}"
    );
    let after = all(&book);
    for (id, transaction) in &before {
        if !again.contains(id) {
            assert_eq!(&after[id], transaction, "{id:?} stays as it was");
        }
    }
    book.undo().expect("undone");
    assert_eq!(all(&book), before);
}
