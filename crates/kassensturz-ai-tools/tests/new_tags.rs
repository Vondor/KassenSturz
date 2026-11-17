// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! New tags the AI prepares together with the transactions that get them
//! (D-830): `new_tags` and `add_new_tags` in `propose_changes`, behind the
//! cell *Tags on transactions › Add*, made only when the user confirms,
//! taken back with one Undo, and never a second tag for a name the book
//! has. On a small synthetic book; no real data.

#![cfg(feature = "ai")]

use kassensturz_ai_tools::{Confirmed, ProposedChange, Slot, call, propose, propose_spec};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Report as _, Split,
    Tag, TagId, Transaction, TransactionId,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2027, 1, 15);

struct Fixture {
    book: Book,
    /// A quarterly bank fee, the entry the user wants tagged.
    fee: TransactionId,
    /// A second fee of another quarter.
    other_fee: TransactionId,
}

fn account(name: &str, kind: AccountKind, currency: CurrencyCode) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency,
        account_type: kind.is_account().then_some(AccountType::Checking),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

fn fixture() -> Fixture {
    let eur = CurrencyCode::new("EUR").expect("EUR");
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(eur).expect("EUR");
    let checking = account("Example Checking", AccountKind::Asset, eur);
    let fees = account("Account fees", AccountKind::Expense, eur);
    for account in [&checking, &fees] {
        book.add_account(account).expect("added");
    }
    let mut fee = |date: Date, minor: i64| {
        let id = TransactionId::generate();
        book.add_transaction(
            &Transaction::new(
                id,
                date,
                Some("Example Bank".into()),
                vec![
                    Split::new(checking.id, Money::from_minor_units(-minor, eur)),
                    Split::new(fees.id, Money::from_minor_units(minor, eur)),
                ],
            )
            .expect("balanced"),
        )
        .expect("added");
        id
    };
    let fee_id = fee(Date::constant(2026, 12, 31), 1_234);
    let other_fee = fee(Date::constant(2026, 9, 30), 1_234);
    Fixture {
        book,
        fee: fee_id,
        other_fee,
    }
}

fn run(book: &Book, name: &str, arguments: Value) -> Value {
    call(book, name, &arguments, TODAY)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .value
}

/// The call of the maintainer's use case, on the synthetic book: one new
/// tag, put on one entry.
fn tag_the_fee(fee: TransactionId) -> Value {
    json!({
        "new_tags": [{"key": "tax", "name": "Tax relevant"}],
        "tag_transactions": [{"transaction_id": fee.to_string(), "add_new_tags": ["tax"]}],
    })
}

/// Everything on except making tags.
fn without_new_tags() -> Permissions {
    Permissions::all().with(Permission::NewTags, false)
}

fn code(result: Result<impl std::fmt::Debug, kassensturz_ai_tools::ToolError>) -> String {
    let report = result.expect_err("refused").model_report();
    let mut text = report.code.to_owned();
    for (name, value) in &report.params {
        if let kassensturz_core::ErrorParam::Keyword(word) = value {
            text.push_str(&format!(" {name}={word}"));
        }
        if let kassensturz_core::ErrorParam::Number(number) = value {
            text.push_str(&format!(" {name}={number}"));
        }
    }
    text
}

#[test]
fn a_new_tag_is_made_with_its_transaction_only_when_the_user_confirms() {
    let mut f = fixture();
    assert_eq!(run(&f.book, "list_tags", json!({}))["tags"], json!([]));
    let proposal = propose(&f.book, &tag_the_fee(f.fee), Permissions::all()).expect("prepared");
    let [
        ProposedChange::NewTag(new),
        ProposedChange::TagTransaction(tagged),
    ] = proposal.changes.changes()
    else {
        panic!("{:?}", proposal.changes.changes());
    };
    assert_eq!(new.name, "Tax relevant");
    assert_eq!(new.uses, 1);
    assert_eq!(tagged.transaction, f.fee);
    assert_eq!(tagged.added_new, ["Tax relevant"]);
    assert!(tagged.added.is_empty());
    let slots: Vec<String> = proposal
        .changes
        .slots()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(slots, ["new_tags/0", "tag_transactions/0"]);
    // Nothing is written before the user confirms.
    assert!(f.book.tags().expect("tags").is_empty());
    assert!(f.book.transaction(f.fee).expect("stored").tags().is_empty());

    let history = f.book.history().expect("history").len();
    proposal
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    let tags = run(&f.book, "list_tags", json!({}))["tags"].clone();
    let [listed] = tags.as_array().expect("tags").as_slice() else {
        panic!("{tags}");
    };
    assert_eq!(listed["name"], "Tax relevant");
    assert_eq!(listed["transactions"], 1);
    let rows = run(
        &f.book,
        "search_transactions",
        json!({"tag_id": listed["tag_id"]}),
    )["rows"]
        .clone();
    let [row] = rows.as_array().expect("rows").as_slice() else {
        panic!("{rows}");
    };
    assert_eq!(row["transaction_id"], f.fee.to_string());
    assert_eq!(row["tags"], json!(["Tax relevant"]));
    assert!(
        f.book
            .transaction(f.other_fee)
            .expect("stored")
            .tags()
            .is_empty()
    );
    assert_eq!(
        f.book.history().expect("history").len(),
        history + 1,
        "one entry for the tag and its use"
    );

    // A second new tag of that name is refused, ignoring capitals and
    // spaces, and the answer names the tag to use instead.
    let again = json!({
        "new_tags": [{"key": "t", "name": "  tax RELEVANT "}],
        "tag_transactions": [{"transaction_id": f.other_fee.to_string(), "add_new_tags": ["t"]}],
    });
    let error = propose(&f.book, &again, Permissions::all()).expect_err("refused");
    assert_eq!(error.model_report().code, "ai-tools.tag-name-taken");
    assert_eq!(
        error.existing_tag().map(|tag| tag.to_string()),
        listed["tag_id"].as_str().map(str::to_owned)
    );
    // Also without a key, as a model might send it.
    let bare = json!({"new_tags": [{"name": "Tax relevant"}]});
    let error = propose(&f.book, &bare, Permissions::all()).expect_err("refused");
    assert!(error.existing_tag().is_some(), "{:?}", error.model_report());
}

#[test]
fn undo_takes_the_new_tag_back_with_its_use() {
    let mut f = fixture();
    let before = f.book.transaction(f.fee).expect("stored");
    propose(&f.book, &tag_the_fee(f.fee), Permissions::all())
        .expect("prepared")
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(f.book.tags().expect("tags").len(), 1);
    f.book.undo().expect("undone").expect("a change");
    assert!(f.book.tags().expect("tags").is_empty());
    assert_eq!(f.book.transaction(f.fee).expect("stored"), before);
    assert_eq!(run(&f.book, "list_tags", json!({}))["tags"], json!([]));
    // And Redo brings both back.
    f.book.redo().expect("redone").expect("a change");
    let tags = f.book.tags().expect("tags");
    assert_eq!(tags.len(), 1);
    assert_eq!(
        f.book.transaction(f.fee).expect("stored").tags(),
        [tags[0].id]
    );
}

#[test]
fn a_declined_proposal_makes_no_tag() {
    let f = fixture();
    let proposal = propose(&f.book, &tag_the_fee(f.fee), Permissions::all()).expect("prepared");
    // The user throws the set away: it is dropped, never applied.
    drop(proposal);
    assert!(f.book.tags().expect("tags").is_empty());
    assert_eq!(run(&f.book, "list_tags", json!({}))["tags"], json!([]));
}

#[test]
fn a_new_tag_no_transaction_gets_is_refused() {
    let f = fixture();
    let alone = json!({"new_tags": [{"key": "tax", "name": "Tax relevant"}]});
    assert_eq!(
        code(propose(&f.book, &alone, Permissions::all())),
        "ai-tools.not-changeable argument=key reason=not-used list=new_tags item=1"
    );
    // One used and one not: the unused one is named.
    let one_unused = json!({
        "new_tags": [{"key": "tax", "name": "Tax relevant"}, {"key": "spare", "name": "Spare"}],
        "tag_transactions": [{"transaction_id": f.fee.to_string(), "add_new_tags": ["tax"]}],
    });
    assert_eq!(
        code(propose(&f.book, &one_unused, Permissions::all())),
        "ai-tools.not-changeable argument=key reason=not-used list=new_tags item=2"
    );
}

#[test]
fn a_new_tag_keeps_the_limits_of_a_tag_made_by_hand() {
    let f = fixture();
    let with = |tags: Value, keys: Value| {
        json!({
            "new_tags": tags,
            "tag_transactions": [{"transaction_id": f.fee.to_string(), "add_new_tags": keys}],
        })
    };
    let cases = [
        // Longer than a tag may be.
        (
            with(json!([{"key": "a", "name": "x".repeat(61)}]), json!(["a"])),
            "store.tag-name reason=too-long max_chars=60 list=new_tags item=1",
        ),
        // More than one line.
        (
            with(json!([{"key": "a", "name": "Tax\nrelevant"}]), json!(["a"])),
            "ai-tools.bad-argument argument=name list=new_tags item=1",
        ),
        // Empty.
        (
            with(json!([{"key": "a", "name": "  "}]), json!(["a"])),
            "ai-tools.bad-argument argument=name list=new_tags item=1",
        ),
        // Two new tags of one name.
        (
            with(
                json!([{"key": "a", "name": "Tax"}, {"key": "b", "name": "tax"}]),
                json!(["a", "b"]),
            ),
            "ai-tools.name-taken list=new_tags item=2",
        ),
        // One key for two tags.
        (
            with(
                json!([{"key": "a", "name": "Tax"}, {"key": "a", "name": "Fees"}]),
                json!(["a"]),
            ),
            "ai-tools.bad-argument argument=key list=new_tags item=2",
        ),
        // A key the call has no new tag for.
        (
            with(json!([{"key": "a", "name": "Tax"}]), json!(["a", "b"])),
            "ai-tools.bad-argument argument=add_new_tags list=tag_transactions item=1",
        ),
        // The same new tag twice on one entry.
        (
            with(json!([{"key": "a", "name": "Tax"}]), json!(["a", "a"])),
            "ai-tools.bad-argument argument=add_new_tags list=tag_transactions item=1",
        ),
        // More new tags than one set may make.
        (
            with(
                json!(
                    (0..6)
                        .map(|n| json!({"key": format!("k{n}"), "name": format!("Tag {n}")}))
                        .collect::<Vec<_>>()
                ),
                json!((0..6).map(|n| format!("k{n}")).collect::<Vec<_>>()),
            ),
            "ai-tools.too-many-changes",
        ),
    ];
    for (call, expected) in cases {
        assert_eq!(
            code(propose(&f.book, &call, Permissions::all())),
            expected,
            "{call}"
        );
    }
    // Five is fine.
    let five = with(
        json!(
            (0..5)
                .map(|n| json!({"key": format!("k{n}"), "name": format!("Tag {n}")}))
                .collect::<Vec<_>>()
        ),
        json!((0..5).map(|n| format!("k{n}")).collect::<Vec<_>>()),
    );
    propose(&f.book, &five, Permissions::all()).expect("five new tags");
}

#[test]
fn making_tags_is_off_until_its_own_cell_is_on() {
    let f = fixture();
    let call = tag_the_fee(f.fee);
    assert_eq!(
        code(propose(&f.book, &call, without_new_tags())),
        "ai-tools.not-allowed argument=new_tags"
    );
    // A key alone is refused as not allowed too, so the model hears where
    // the user allows it.
    let keys_only = json!({"tag_transactions": [{
        "transaction_id": f.fee.to_string(), "add_new_tags": ["tax"],
    }]});
    assert_eq!(
        code(propose(&f.book, &keys_only, without_new_tags())),
        "ai-tools.not-allowed argument=add_new_tags"
    );
    // Making tags without tagging does nothing: a new tag needs a
    // transaction to go on.
    let tags_only = Permissions::all().with(Permission::TagTransactions, false);
    assert_eq!(
        code(propose(&f.book, &call, tags_only)),
        "ai-tools.not-allowed argument=new_tags"
    );
    // The grid starts with it off (AGENTS.md §1: defaults never lower
    // protection).
    assert!(!Permissions::defaults().allows(Permission::NewTags));

    // The model is told about it only when it may do it.
    let offered = |allowed| {
        let spec = propose_spec(allowed).expect("offered");
        (
            spec.parameters.contains("\"new_tags\""),
            spec.parameters.contains("\"add_new_tags\""),
            spec.parameters.contains("never one list_tags shows"),
        )
    };
    assert_eq!(offered(Permissions::all()), (true, true, true));
    assert_eq!(offered(without_new_tags()), (false, false, false));
    assert_eq!(offered(tags_only), (false, false, false));
    // Only a name the user gave, as for a budget's name: the list says it.
    let spec = propose_spec(Permissions::all()).expect("offered");
    let parameters: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    let said = parameters["properties"]["new_tags"]["description"]
        .as_str()
        .expect("described");
    assert!(
        said.contains("only with a name the user gave or clearly asked for"),
        "{said}"
    );
    assert!(said.contains("never one list_tags shows"), "{said}");
}

#[test]
fn a_set_with_a_new_tag_stops_when_the_cell_is_switched_off_before_confirming() {
    let f = fixture();
    let proposal = propose(&f.book, &tag_the_fee(f.fee), Permissions::all()).expect("prepared");
    assert!(proposal.changes.check_allowed(Permissions::all()).is_ok());
    assert!(proposal.changes.check_allowed(without_new_tags()).is_err());
}

#[test]
fn a_tag_made_by_hand_in_between_stops_the_whole_set() {
    let mut f = fixture();
    let proposal = propose(&f.book, &tag_the_fee(f.fee), Permissions::all()).expect("prepared");
    let by_hand = TagId::generate();
    f.book
        .add_tag(&Tag {
            id: by_hand,
            name: "tax relevant".into(),
        })
        .expect("made by hand");
    let error = proposal
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect_err("refused");
    assert_eq!(error.report().code, "ai-changes.not-applied");
    assert_eq!(f.book.tags().expect("tags").len(), 1);
    assert!(f.book.transaction(f.fee).expect("stored").tags().is_empty());
}

#[test]
fn leaving_out_the_last_entry_of_a_new_tag_leaves_the_tag_out_too() {
    let f = fixture();
    let call = json!({
        "new_tags": [{"key": "tax", "name": "Tax relevant"}],
        "tag_transactions": [
            {"transaction_id": f.fee.to_string(), "add_new_tags": ["tax"]},
            {"transaction_id": f.other_fee.to_string(), "add_new_tags": ["tax"]},
        ],
    });
    let allowed = Permissions::all();
    let set = propose(&f.book, &call, allowed).expect("prepared").changes;
    let slot = |text: &str| Slot::parse(text).expect("a slot");
    // The new tag can't be left out while an entry gets it.
    let error = set
        .leave_out(&f.book, slot("new_tags/0"), allowed)
        .expect_err("in use");
    assert_eq!(error.report().code, "ai-changes.edit-in-use");
    // One entry out: the tag stays, on the other.
    let one = set
        .leave_out(&f.book, slot("tag_transactions/0"), allowed)
        .expect("left out");
    let [
        ProposedChange::NewTag(new),
        ProposedChange::TagTransaction(_),
    ] = one.changes()
    else {
        panic!("{:?}", one.changes());
    };
    assert_eq!(new.uses, 1);
    // The last entry out: the tag goes with it.
    let none = one
        .leave_out(&f.book, slot("tag_transactions/1"), allowed)
        .expect("left out");
    assert!(none.changes().is_empty(), "{:?}", none.changes());
    // Taking an entry back brings its new tag back.
    let back = none
        .back_to_ai(&f.book, slot("tag_transactions/1"), allowed)
        .expect("back");
    let [
        ProposedChange::NewTag(new),
        ProposedChange::TagTransaction(tagged),
    ] = back.changes()
    else {
        panic!("{:?}", back.changes());
    };
    assert_eq!(new.uses, 1);
    assert_eq!(tagged.transaction, f.other_fee);
}

#[test]
fn a_check_only_call_counts_the_new_tags() {
    let f = fixture();
    let proposal = propose(&f.book, &tag_the_fee(f.fee), Permissions::all()).expect("prepared");
    let answer = kassensturz_ai_tools::check_only_answer(&f.book, &proposal, Permissions::all())
        .expect("answer");
    assert_eq!(answer["lists"]["new_tags"], json!(1));
    assert_eq!(answer["lists"]["tag_transactions"], json!(1));
    assert!(f.book.tags().expect("tags").is_empty());
}
