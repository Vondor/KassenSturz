// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AI's `join_transfers` (US3.3, #571, D-693): two sides of a move
//! between the user's accounts named by the model, checked as the store
//! joins them (D-691), reviewed before anything changes, applied only as
//! reviewed, and taken back with the rest of the set by one Undo.

use kassensturz_store::SideSorting;

use super::*;

/// The sample book with two sides of one 100 € move: `out` leaves the
/// checking account and the user put it into Food by hand; `into` arrives
/// in the savings account and has no category yet.
struct Sides {
    sample: Sample,
    savings: AccountId,
    out: TransactionId,
    into: TransactionId,
}

fn sides() -> Sides {
    let mut sample = sample();
    let savings = sample
        .book
        .accounts(false)
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == "Savings")
        .expect("savings")
        .id;
    let out = bought(
        &mut sample.book,
        "To my savings",
        vec![
            Split::new(sample.checking, eur(-10000)),
            Split::new(sample.food, eur(10000)),
        ],
    );
    let into = bought(
        &mut sample.book,
        "From my checking account",
        vec![
            Split::new(savings, eur(10000)),
            Split::uncategorized(eur(-10000)),
        ],
    );
    Sides {
        sample,
        savings,
        out,
        into,
    }
}

fn join_call(pairs: &[(TransactionId, TransactionId)]) -> Value {
    json!({
        "join_transfers": pairs
            .iter()
            .map(|(a, b)| json!({"transaction_id": a.to_string(), "other_transaction_id": b.to_string()}))
            .collect::<Vec<_>>()
    })
}

/// The join of a proposal that holds only it.
fn the_join(proposal: &Proposal) -> &JoinTransfer {
    match proposal.changes.changes() {
        [ProposedChange::JoinTransfer(join)] => join,
        other => panic!("{other:?}"),
    }
}

/// What the parts booked to `account` add up to.
fn booked(book: &Book, account: AccountId) -> i64 {
    book.transactions()
        .expect("transactions")
        .iter()
        .flat_map(|transaction| transaction.splits().iter())
        .filter(|split| split.account == Some(account))
        .map(|split| split.account_amount().minor_units())
        .sum()
}

#[test]
fn a_pair_in_either_order_becomes_one_transfer_and_one_undo_takes_it_back() {
    let Sides {
        sample,
        savings,
        out,
        into,
    } = sides();
    let (mut book, checking, food) = (sample.book, sample.checking, sample.food);
    let before = state(&book);
    let (checking_before, savings_before) = (booked(&book, checking), booked(&book, savings));

    // The incoming side first: the money's direction decides which stays.
    let proposal = propose(&book, &join_call(&[(into, out)])).expect("prepared");
    assert_eq!(state(&book), before, "a proposal changes nothing");
    let join = the_join(&proposal);
    assert_eq!(join.transfer.outgoing.transaction, out);
    assert_eq!(join.transfer.outgoing.account, checking);
    assert_eq!(join.transfer.outgoing.category, Some(food));
    assert_eq!(join.transfer.incoming.transaction, into);
    assert_eq!(join.transfer.incoming.account, savings);
    assert_eq!(join.transfer.business_days, 0);
    // A side the user sorted may be named; the review says who sorted it.
    assert_eq!(join.outgoing_sorting, SideSorting::ByHand);
    assert_eq!(join.incoming_sorting, SideSorting::Open);
    assert_eq!(proposal.output.value["prepared"], json!(1));
    assert_eq!(
        proposal.changes.summary(),
        format!("transactions {out} and {into} become one Transfer")
    );

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert!(matches!(
        book.transaction(into),
        Err(StoreError::UnknownTransaction(_))
    ));
    let joined = book.transaction(out).expect("kept");
    let accounts: Vec<Option<AccountId>> =
        joined.splits().iter().map(|split| split.account).collect();
    assert_eq!(accounts, [Some(checking), Some(savings)]);
    // Both registers stay as their banks booked them.
    assert_eq!(booked(&book, checking), checking_before);
    assert_eq!(booked(&book, savings), savings_before);
    assert_eq!(booked(&book, food), before_food(&before, food));
    let history = book.history().expect("history");
    assert_eq!(history.len(), before.3 + 1, "one entry");
    assert_eq!(history[0].action, Action::ApplyAiChanges);

    book.undo().expect("undone");
    assert_eq!(state(&book).1, before.1, "both sides and the category back");
}

/// What Food held before the join, less the 100 € that went with it.
fn before_food(before: &(Vec<Account>, Vec<Transaction>, usize, usize), food: AccountId) -> i64 {
    before
        .1
        .iter()
        .flat_map(|transaction| transaction.splits().iter())
        .filter(|split| split.account == Some(food))
        .map(|split| split.account_amount().minor_units())
        .sum::<i64>()
        - 10000
}

#[test]
fn a_pair_the_store_would_not_join_is_refused_with_its_reason() {
    let Sides {
        sample, out, into, ..
    } = sides();
    let book = &sample.book;
    let refused = |pairs: &[(TransactionId, TransactionId)]| code(propose(book, &join_call(pairs)));
    assert_eq!(refused(&[(out, out)]), "transfer.same-transaction");
    // Both in the checking account.
    assert_eq!(refused(&[(out, sample.unsorted)]), "transfer.same-account");
    assert_eq!(
        refused(&[(into, sample.unsorted)]),
        "transfer.amounts-differ"
    );
    // In Parts: not one side of a move.
    assert_eq!(refused(&[(into, sample.split)]), "transfer.not-a-side");
    assert_eq!(
        refused(&[(into, TransactionId::generate())]),
        "ai-tools.unknown-transaction"
    );
    assert_eq!(
        code(propose(
            book,
            &json!({"join_transfers": [{"transaction_id": out.to_string()}]})
        )),
        "bad-argument other_transaction_id"
    );
    // The model hears which pair it was.
    let error = propose(book, &join_call(&[(out, out)])).expect_err("refused");
    let report = error.model_report();
    assert_eq!(report.code, "transfer.same-transaction");
    assert!(
        report
            .params
            .iter()
            .any(|(name, value)| *name == "list" && *value == ErrorParam::Keyword("join_transfers"))
    );
}

#[test]
fn a_transaction_is_in_one_change_of_a_set_only() {
    let Sides {
        sample, out, into, ..
    } = sides();
    let book = &sample.book;
    let twice = join_call(&[(out, into), (into, out)]);
    assert_eq!(code(propose(book, &twice)), "ai-tools.not-changeable");
    let mut sorted_too = join_call(&[(out, into)]);
    sorted_too["categorize"] = json!([{
        "transaction_id": into.to_string(),
        "category_id": sample.groceries.to_string(),
    }]);
    let error = propose(book, &sorted_too).expect_err("refused");
    assert!(
        matches!(
            error.without_item(),
            ToolError::NotChangeable {
                argument: "transaction_id",
                reason: "also-changed",
            }
        ),
        "refused for another reason"
    );
}

#[test]
fn joining_needs_its_own_cell_and_is_checked_again_on_confirm() {
    let Sides {
        mut sample,
        savings,
        out,
        into,
    } = sides();
    let call = join_call(&[(out, into)]);
    // Sorting transactions never allowed merging them.
    let sorting = Permissions::NONE
        .with(Permission::SortTransactions, true)
        .with_lock(Field::Category, false);
    assert_eq!(
        code(super::super::propose(&sample.book, &call, sorting)),
        "ai-tools.not-allowed"
    );
    let spec = propose_spec(sorting).expect("offered");
    assert!(!spec.parameters.contains("join_transfers"));

    let joining = Permissions::NONE.with(Permission::JoinTransfers, true);
    assert!(joining.allows(Permission::TransactionsRead));
    let spec = propose_spec(joining).expect("offered");
    let parameters: Value = serde_json::from_str(&spec.parameters).expect("json");
    let lists: Vec<&String> = parameters["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .collect();
    assert_eq!(lists, ["join_transfers"]);
    assert!(spec.description.contains("joining Transfers"));
    let proposal = super::super::propose(&sample.book, &call, joining).expect("prepared");
    assert!(matches!(
        proposal.changes.check_allowed(Permissions::NONE),
        Err(ChangeError::NotAllowed)
    ));
    proposal
        .changes
        .check_allowed(joining)
        .expect("still allowed");

    // A side the user sorted since the review: nothing is applied.
    let sorted = Transaction::new(
        into,
        Date::constant(2026, 10, 5),
        Some("From my checking account".into()),
        vec![
            Split::new(savings, eur(10000)),
            Split::new(sample.salary, eur(-10000)),
        ],
    )
    .expect("balances");
    sample.book.update_transaction(&sorted).expect("sorted");
    let before = state(&sample.book);
    assert!(matches!(
        proposal
            .changes
            .apply(&mut sample.book, Confirmed::by_the_user()),
        Err(ChangeError::OutOfDate)
    ));
    assert_eq!(state(&sample.book), before);
}
