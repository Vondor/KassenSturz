// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AI's "sort with all Rules" (US2.9, #572, D-682): the store's own run
//! of "Apply all" (D-630, D-631) as one change set, reviewed before it
//! changes anything, applied only as reviewed, and taken back by one Undo.

use kassensturz_store::RuleChange;

use super::*;

/// The sort of a proposal that holds only it.
fn the_sort(proposal: &Proposal) -> &SortWithRules {
    match proposal.changes.changes() {
        [ProposedChange::SortWithRules(sort)] => sort,
        other => panic!("{other:?}"),
    }
}

fn sort_call(item: Value) -> Value {
    json!({ "sort_with_rules": [item] })
}

/// The category the part without the account is in.
fn category_of(book: &Book, id: TransactionId, account: AccountId) -> Option<AccountId> {
    book.transaction(id)
        .expect("transaction")
        .splits()
        .iter()
        .find(|split| split.account != Some(account))
        .and_then(|split| split.account)
}

#[test]
fn the_rules_sort_what_has_no_category_and_one_undo_takes_it_back() {
    let (sample, _) = with_rules();
    let Sample {
        mut book,
        checking,
        groceries,
        bought,
        unsorted,
        split,
        ..
    } = sample;
    let before = state(&book);
    let proposal = propose(&book, &sort_call(json!({}))).expect("prepared");
    assert_eq!(state(&book), before, "a proposal changes nothing");
    let sort = the_sort(&proposal);
    assert_eq!(sort.rule, None);
    assert!(!sort.sort_again);
    // Only the Pet Shop entry has no category; the grocer's was sorted by
    // hand and the market's is in Parts, so neither is touched (D-031).
    assert_eq!(
        sort.changes,
        [RuleChange {
            transaction: unsorted,
            rule: book.rules().expect("rules")[1].rule.id,
            account: checking,
            booked_to: Some(groceries),
            payee: None,
        }]
    );
    assert_eq!(sort.staying(), 0);
    // The model hears the counts, from the run.
    assert_eq!(proposal.output.value["transactions_sorted"], json!(1));
    assert_eq!(
        proposal.output.value["transactions_left_as_they_are"],
        json!(0)
    );
    let hand_sorted = book.transaction(bought).expect("grocer");
    let in_parts = book.transaction(split).expect("market");

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(category_of(&book, unsorted, checking), Some(groceries));
    assert_eq!(book.transaction(bought).expect("grocer"), hand_sorted);
    assert_eq!(book.transaction(split).expect("market"), in_parts);
    let history = book.history().expect("history");
    assert_eq!(history.len(), before.3 + 1, "one entry");
    assert_eq!(history[0].action, Action::ApplyAiChanges);

    book.undo().expect("undone");
    assert_eq!(state(&book).1, before.1, "one Undo takes the run back");
    assert_eq!(category_of(&book, unsorted, checking), None);
}

#[test]
fn sorting_again_follows_a_changed_rule_but_never_what_the_user_sorted() {
    let (sample, rules) = with_rules();
    let Sample {
        mut book,
        checking,
        food,
        groceries,
        bought,
        unsorted,
        ..
    } = sample;
    propose(&book, &sort_call(json!({})))
        .expect("prepared")
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    // The user now sends pets to Food, and the grocer's Rule too.
    for rule in [&rules[0], &rules[1]] {
        book.update_rule(&Rule {
            actions: Actions {
                book_to: Some(BookTo::Category(food)),
                payee: None,
            },
            ..rule.clone()
        })
        .expect("changed");
    }

    // Without sorting again, what a Rule sorted stays.
    let nothing = propose(&book, &sort_call(json!({})));
    assert_eq!(code(nothing), "ai-tools.not-changeable");

    let again = propose(&book, &sort_call(json!({"sort_again": true}))).expect("prepared");
    let sort = the_sort(&again);
    assert!(sort.sort_again);
    let moved: Vec<TransactionId> = sort.changes.iter().map(|c| c.transaction).collect();
    assert_eq!(moved, [unsorted], "the grocer's entry was sorted by hand");
    again
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(category_of(&book, unsorted, checking), Some(food));
    assert_eq!(category_of(&book, bought, checking), Some(groceries));
}

#[test]
fn one_rule_sorts_only_what_it_decides() {
    let (sample, rules) = with_rules();
    let book = sample.book;
    let pets = propose(
        &book,
        &sort_call(json!({"rule_id": rules[1].id.to_string()})),
    )
    .expect("prepared");
    let sort = the_sort(&pets);
    assert_eq!(sort.rule, Some(rules[1].id));
    assert_eq!(sort.rule_name.as_deref(), Some("Pets"));
    assert_eq!(sort.changes.len(), 1);
    // The grocer's Rule decides nothing without a category.
    let grocer = propose(
        &book,
        &sort_call(json!({"rule_id": rules[0].id.to_string()})),
    );
    assert_eq!(code(grocer), "ai-tools.not-changeable");
}

#[test]
fn a_book_changed_since_the_review_sorts_nothing() {
    // The entry the run sorts was sorted by hand since.
    let (sample, _) = with_rules();
    let Sample {
        mut book,
        checking,
        salary,
        unsorted,
        ..
    } = sample;
    let proposal = propose(&book, &sort_call(json!({}))).expect("prepared");
    let stored = book.transaction(unsorted).expect("pet shop");
    let splits = stored
        .splits()
        .iter()
        .map(|split| {
            if split.account == Some(checking) {
                split.clone()
            } else {
                Split {
                    account: Some(salary),
                    ..split.clone()
                }
            }
        })
        .collect();
    let by_hand = Transaction::new(
        stored.id(),
        stored.booking_date(),
        stored.payee().map(str::to_owned),
        splits,
    )
    .expect("balances")
    .with_details_of(&stored);
    book.update_transaction(&by_hand).expect("sorted by hand");
    let before = state(&book);
    assert!(matches!(
        proposal.changes.apply(&mut book, Confirmed::by_the_user()),
        Err(ChangeError::OutOfDate)
    ));
    assert_eq!(state(&book), before, "nothing of the set is applied");

    // A new entry the Rules would sort too: the run is not the reviewed one.
    let (sample, _) = with_rules();
    let Sample {
        mut book, checking, ..
    } = sample;
    let proposal = propose(&book, &sort_call(json!({}))).expect("prepared");
    bought(
        &mut book,
        "Pet Palace",
        vec![
            Split::new(checking, eur(-750)),
            Split::uncategorized(eur(750)),
        ],
    );
    let before = state(&book);
    assert!(matches!(
        proposal.changes.apply(&mut book, Confirmed::by_the_user()),
        Err(ChangeError::OutOfDate)
    ));
    assert_eq!(state(&book), before, "nothing is sorted");
}

#[test]
fn calls_the_sort_cannot_take_are_refused() {
    let (sample, rules) = with_rules();
    let Sample { book, unsorted, .. } = sample;
    let pets = rules[1].id.to_string();
    for (call, expected) in [
        (
            json!({"sort_with_rules": [{}, {}]}),
            "bad-argument sort_with_rules",
        ),
        (
            json!({"sort_with_rules": {}}),
            "bad-argument sort_with_rules",
        ),
        (
            sort_call(json!({"sort_again": "yes"})),
            "bad-argument sort_again",
        ),
        (
            sort_call(json!({"rule_id": "Pets"})),
            "bad-argument rule_id",
        ),
        (
            sort_call(json!({"rule_id": "00000000-0000-0000-0000-00000000000f"})),
            "ai-tools.unknown-rule",
        ),
        (
            sort_call(json!({"category_id": pets})),
            "unknown-field sort_with_rules",
        ),
        // Alone: another change could make the run differ from the review.
        (
            json!({
                "sort_with_rules": [{}],
                "categorize": [{"transaction_id": unsorted.to_string(), "category_id": pets}],
            }),
            "ai-tools.not-changeable",
        ),
        (
            json!({
                "sort_with_rules": [{}],
                "new_categories": [{"key": "pets", "name": "Pets"}],
            }),
            "ai-tools.not-changeable",
        ),
    ] {
        assert_eq!(code(propose(&book, &call)), expected, "{call}");
    }
    // The refusals name the reason in fixed words only.
    let report = propose(
        &book,
        &json!({"sort_with_rules": [{}], "new_categories": [{"key": "k", "name": "K"}]}),
    )
    .expect_err("refused")
    .model_report();
    assert_eq!(
        report.params,
        [
            ("argument", ErrorParam::Keyword("sort_with_rules")),
            ("reason", ErrorParam::Keyword("not-alone")),
        ]
    );
}

#[test]
fn sorting_needs_its_permission_and_the_fields_it_changes_unlocked() {
    let (sample, _) = with_rules();
    let mut book = sample.book;
    let call = sort_call(json!({}));
    let no_sorting = Permissions::all().with(Permission::SortTransactions, false);
    assert_eq!(
        code(super::super::propose(&book, &call, no_sorting)),
        "ai-tools.not-allowed"
    );
    let category_locked = Permissions::all().with_lock(Field::Category, true);
    assert_eq!(
        code(super::super::propose(&book, &call, category_locked)),
        "ai-tools.not-allowed"
    );
    assert!(
        propose_spec(category_locked)
            .is_none_or(|spec| !spec.parameters.contains("sort_with_rules")),
        "not offered when it can't be used"
    );

    // A Rule that also sets the name shown: the payee lock (on by default)
    // stops it, and so does switching it on after the review.
    let mut rule = book.rules().expect("rules")[1].rule.clone();
    rule.actions.payee = Some("Pet shop".into());
    book.update_rule(&rule).expect("changed");
    assert_eq!(
        code(super::super::propose(&book, &call, Permissions::all())),
        "ai-tools.field-locked"
    );
    let payee_open = Permissions::all().with_lock(Field::Payee, false);
    let proposal = super::super::propose(&book, &call, payee_open).expect("prepared");
    assert!(the_sort(&proposal).sets_payee());
    assert!(proposal.changes.check_allowed(payee_open).is_ok());
    assert!(matches!(
        proposal.changes.check_allowed(Permissions::all()),
        Err(ChangeError::NotAllowed)
    ));
}

#[test]
fn the_user_switches_sorting_again_on_in_the_review() {
    let (sample, rules) = with_rules();
    let Sample { mut book, food, .. } = sample;
    propose(&book, &sort_call(json!({})))
        .expect("prepared")
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    book.update_rule(&Rule {
        actions: Actions {
            book_to: Some(BookTo::Category(food)),
            payee: None,
        },
        ..rules[1].clone()
    })
    .expect("changed");
    // A new entry for the Rules, so the first call has something to sort.
    let checking = book
        .all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == "Checking account")
        .expect("checking")
        .id;
    bought(
        &mut book,
        "Pet Palace",
        vec![
            Split::new(checking, eur(-750)),
            Split::uncategorized(eur(750)),
        ],
    );
    let proposal = propose(&book, &sort_call(json!({}))).expect("prepared");
    assert_eq!(the_sort(&proposal).changes.len(), 1);
    let slot = proposal.changes.slots()[0];
    assert_eq!(slot.to_string(), "sort_with_rules/0");
    assert_eq!(slot.editable(), ["sort_again"]);
    let edited = proposal
        .changes
        .edit(
            &book,
            slot,
            &json!({"sort_again": true}),
            Permissions::all(),
        )
        .expect("kept");
    assert!(edited.edited(slot));
    let ProposedChange::SortWithRules(sort) = &edited.changes()[0] else {
        panic!("{:?}", edited.changes());
    };
    assert!(sort.sort_again);
    assert_eq!(sort.changes.len(), 2, "the earlier pet shop entry too");
    // Only this switch can be edited.
    assert!(matches!(
        proposal.changes.edit(
            &book,
            slot,
            &json!({"rule_id": rules[0].id.to_string()}),
            Permissions::all()
        ),
        Err(EditError::NotEditable)
    ));
    edited
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
}

#[test]
fn the_summary_says_what_sorts() {
    let (sample, _) = with_rules();
    let proposal = propose(&sample.book, &sort_call(json!({}))).expect("prepared");
    assert_eq!(
        proposal.changes.summary(),
        "sort 1 transactions with all Rules"
    );
}

#[test]
fn a_rule_named_next_to_its_id_is_checked() {
    let (sample, rules) = with_rules();
    let book = sample.book;
    let pets = rules[1].id.to_string();
    let named = sort_call(json!({"rule_id": pets, "rule_name": "Pets"}));
    assert_eq!(code(propose(&book, &named)), "prepared");
    let wrong = sort_call(json!({"rule_id": pets, "rule_name": "Grocer"}));
    assert_eq!(code(propose(&book, &wrong)), "ai-tools.name-mismatch");
}
