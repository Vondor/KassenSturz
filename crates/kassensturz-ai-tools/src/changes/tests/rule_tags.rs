// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rules that add tags in the AI's tools (D-827): a new Rule may only tag,
//! an edit adds and takes off a Rule's tags, `list_rules` shows them,
//! `search_transactions` says which tags a Rule added, and
//! `sort_with_rules` counts what it tags, also when only checked.

use kassensturz_core::{Tag, TagId};
use kassensturz_store::TagAddedBy;

use super::*;

fn tag(book: &mut Book, name: &str) -> TagId {
    let id = TagId::generate();
    book.add_tag(&Tag {
        id,
        name: name.into(),
    })
    .expect("tag");
    id
}

fn pets_condition() -> Value {
    json!({"field": "payee", "test": "contains", "text": "pet"})
}

fn only_rule(proposal: &Proposal) -> &NewRule {
    match proposal.changes.changes() {
        [ProposedChange::NewRule(rule)] => rule,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_new_rule_may_only_add_tags_and_then_never_sorts() {
    let Sample {
        mut book,
        unsorted,
        checking,
        ..
    } = sample();
    let animals = tag(&mut book, "Animals");
    let call = json!({"new_rules": [{
        "name": "Animals",
        "conditions": [pets_condition()],
        "add_tag_ids": [animals.to_string()],
    }]});
    let proposal = propose(&book, &call).expect("a Rule that only tags");
    let rule = only_rule(&proposal);
    assert_eq!(rule.category, None);
    assert_eq!(rule.tags, ["Animals"]);
    assert_eq!(rule.rule.actions.tags, [animals]);
    assert_eq!(rule.rule.actions.book_to, None);
    assert!(
        proposal.changes.summary().contains("only adding tags"),
        "{}",
        proposal.changes.summary()
    );
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    // A Rule with neither a category nor a tag is still refused.
    let empty = json!({"new_rules": [{"name": "Empty", "conditions": [pets_condition()]}]});
    assert_ne!(code(propose(&book, &empty)), "prepared");
    // An unknown tag and the same tag twice are refused too.
    let unknown = json!({"new_rules": [{
        "name": "Unknown",
        "conditions": [pets_condition()],
        "add_tag_ids": [TagId::generate().to_string()],
    }]});
    assert_ne!(code(propose(&book, &unknown)), "prepared");
    let twice = json!({"new_rules": [{
        "name": "Twice",
        "conditions": [pets_condition()],
        "add_tag_ids": [animals.to_string(), animals.to_string()],
    }]});
    assert_ne!(code(propose(&book, &twice)), "prepared");

    // The Rule tags and leaves the category alone.
    let run = book.run_rules().expect("run");
    assert!(run.changed.is_empty(), "{:?}", run.changed);
    assert_eq!(category_of_unsorted(&book, unsorted, checking), None);
    let sources = book.tag_sources(unsorted).expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].0, animals);
    assert!(matches!(sources[0].1, TagAddedBy::Rule(Some(_))));
}

fn category_of_unsorted(book: &Book, id: TransactionId, account: AccountId) -> Option<AccountId> {
    book.transaction(id)
        .expect("transaction")
        .splits()
        .iter()
        .find(|split| split.account != Some(account))
        .and_then(|split| split.account)
}

#[test]
fn an_edit_adds_and_takes_off_a_rules_tags() {
    let (mut sample, rules) = with_rules();
    let book = &mut sample.book;
    let animals = tag(book, "Animals");
    let monthly = tag(book, "Monthly");
    let pets = rules[1].id;
    let add = json!({"edit_rules": [{
        "rule_id": pets.to_string(),
        "add_tag_ids": [animals.to_string(), monthly.to_string()],
    }]});
    let proposal = propose(book, &add).expect("edited");
    let ProposedChange::EditRule(edit) = &proposal.changes.changes()[0] else {
        panic!("an edit");
    };
    assert!(edit.before_tags.is_empty());
    assert_eq!(edit.tags, ["Animals", "Monthly"]);
    proposal
        .changes
        .apply(book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        book.rule(pets).expect("Rule").actions.tags,
        [animals, monthly]
    );
    let remove = json!({"edit_rules": [{
        "rule_id": pets.to_string(),
        "remove_tag_ids": [animals.to_string()],
    }]});
    let proposal = propose(book, &remove).expect("edited");
    let ProposedChange::EditRule(edit) = &proposal.changes.changes()[0] else {
        panic!("an edit");
    };
    assert_eq!(edit.before_tags, ["Animals", "Monthly"]);
    assert_eq!(edit.tags, ["Monthly"]);
    proposal
        .changes
        .apply(book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(book.rule(pets).expect("Rule").actions.tags, [monthly]);
    // A tag the Rule doesn't have can't be taken off.
    let absent = json!({"edit_rules": [{
        "rule_id": pets.to_string(),
        "remove_tag_ids": [animals.to_string()],
    }]});
    assert_ne!(code(propose(book, &absent)), "prepared");
}

#[test]
fn the_reads_show_a_rules_tags_and_which_tags_a_rule_added() {
    let (mut sample, rules) = with_rules();
    let unsorted = sample.unsorted;
    let book = &mut sample.book;
    let animals = tag(book, "Animals");
    let tagger = Rule {
        id: RuleId::generate(),
        name: "Animals".into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::Payee,
            operator: TextOperator::Contains,
            value: "pet".into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: None,
            payee: None,
            tags: vec![animals],
        },
    };
    book.add_rule(&tagger, None).expect("Rule");
    let listed = crate::call(book, "list_rules", &json!({}), Date::constant(2026, 11, 1))
        .expect("listed")
        .value;
    let rows = listed["rules"].as_array().expect("rules");
    let row = rows
        .iter()
        .find(|row| row["id"] == json!(tagger.id.to_string()))
        .unwrap_or_else(|| panic!("{listed}"));
    assert_eq!(row["tag_ids"], json!([animals.to_string()]), "{row}");
    assert_eq!(row["only_adds_tags"], json!(true), "{row}");
    let sorting = rows
        .iter()
        .find(|row| row["id"] == json!(rules[0].id.to_string()))
        .expect("Grocer");
    assert!(sorting.get("only_adds_tags").is_none(), "{sorting}");
    assert!(sorting.get("tag_ids").is_none(), "{sorting}");

    book.run_rules().expect("run");
    let found = crate::call(
        book,
        "search_transactions",
        &json!({"limit": 50}),
        Date::constant(2026, 11, 1),
    )
    .expect("found")
    .value;
    let rows = found["rows"].as_array().expect("rows");
    let tagged: Vec<&Value> = rows
        .iter()
        .filter(|row| row.get("tag_ids").is_some())
        .collect();
    assert_eq!(tagged.len(), 1, "{found}");
    assert_eq!(tagged[0]["rule_tag_ids"], json!([animals.to_string()]));
    assert_eq!(
        tagged[0]["transaction_id"],
        json!(unsorted.to_string()),
        "{found}"
    );
}

#[test]
fn the_sort_counts_what_it_tags_and_a_check_says_so() {
    let (mut sample, _) = with_rules();
    let animals = tag(&mut sample.book, "Animals");
    let tagger = Rule {
        id: RuleId::generate(),
        name: "Animals".into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::Payee,
            operator: TextOperator::Contains,
            value: "pet".into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: None,
            payee: None,
            tags: vec![animals],
        },
    };
    sample.book.add_rule(&tagger, None).expect("Rule");
    let call = json!({"sort_with_rules": [{}]});
    let proposal = propose(&sample.book, &call).expect("prepared");
    let ProposedChange::SortWithRules(sort) = &proposal.changes.changes()[0] else {
        panic!("a sort");
    };
    assert_eq!(sort.changes.len(), 1, "the Pet Shop entry is sorted");
    assert_eq!(sort.tagged.len(), 1, "and tagged");
    assert_eq!(sort.tagged[0].transaction, sample.unsorted);
    assert!(
        proposal.changes.summary() == "sort 1 transactions and tag 1 with all Rules",
        "{}",
        proposal.changes.summary()
    );
    let counted =
        crate::check_only_answer(&sample.book, &proposal, Permissions::all()).expect("checked");
    assert_eq!(counted["would_sort"], json!(1), "{counted}");
    assert_eq!(counted["would_tag"], json!(1), "{counted}");
    proposal
        .changes
        .apply(&mut sample.book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        sample
            .book
            .transaction(sample.unsorted)
            .expect("Pet Shop")
            .all_tags(),
        [animals]
    );
    // A second sort has nothing left to do.
    assert_ne!(code(propose(&sample.book, &call)), "prepared");
}
