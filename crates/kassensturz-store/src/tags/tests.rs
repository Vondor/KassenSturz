// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of tags on whole transactions and on parts, their names and
//! limits, and renaming, removing and merging them (D-712, D-713), on books
//! in memory. All data is made up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{
    AccountId, MAX_TAG_NAME_CHARS, MAX_TAGS_ON_PART, MAX_TAGS_ON_TRANSACTION, Split, Tag, TagId,
    Transaction, TransactionId,
};

use super::*;
use crate::tests::{Sample, date, eur, sample};

fn add_tag(book: &mut Book, name: &str) -> TagId {
    let tag = Tag {
        id: TagId::generate(),
        name: name.into(),
    };
    book.add_tag(&tag).expect("tag added");
    tag.id
}

/// A purchase of 90.00 from `checking`: 60.00 on `groceries` with
/// `part_tags`, 30.00 on no category yet, and `entry_tags` on the whole.
fn purchase(
    checking: AccountId,
    groceries: AccountId,
    entry_tags: Vec<TagId>,
    part_tags: Vec<TagId>,
) -> Transaction {
    Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        Some("Corner Grocery".into()),
        vec![
            Split::new(checking, eur(-9000)),
            Split {
                tags: part_tags,
                ..Split::new(groceries, eur(6000))
            },
            Split::uncategorized(eur(3000)),
        ],
    )
    .expect("balanced")
    .with_tags(entry_tags)
}

fn last_action(book: &Book) -> String {
    book.conn
        .query_row(
            "SELECT action FROM main.changes ORDER BY sequence DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("newest change")
}

#[test]
fn a_whole_transaction_keeps_its_tags_and_every_part_counts_for_them() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let holiday = add_tag(&mut book, "Holiday 2026");
    let tax = add_tag(&mut book, "Tax");
    // A tag given twice is kept once.
    let bought = purchase(
        checking,
        groceries,
        vec![holiday, holiday],
        vec![tax, holiday],
    );
    assert_eq!(bought.tags(), [holiday]);
    book.add_transaction(&bought).expect("added");

    let read = book.transaction(bought.id()).expect("read");
    assert_eq!(read, bought);
    assert_eq!(book.transactions().expect("all"), vec![bought.clone()]);
    assert_eq!(
        book.transactions_of(checking).expect("register"),
        vec![bought.clone()]
    );
    // The entry's tags first, then the part's own, each once (D-713); a
    // part without own tags counts for the entry's.
    assert_eq!(read.tags_of(&read.splits()[1]), vec![holiday, tax]);
    assert_eq!(read.tags_of(&read.splits()[2]), vec![holiday]);

    // A change writes them again and Undo brings the earlier ones back.
    let changed = Transaction::new(
        bought.id(),
        bought.booking_date(),
        bought.payee().map(str::to_owned),
        bought.splits().to_vec(),
    )
    .expect("balanced")
    .with_tags(vec![tax, holiday]);
    book.update_transaction(&changed).expect("changed");
    assert_eq!(
        book.transaction(bought.id()).expect("read").tags(),
        [tax, holiday]
    );
    book.undo().expect("undone");
    assert_eq!(book.transaction(bought.id()).expect("read"), bought);

    // Building it again from the stored one keeps the tags (D-132).
    let rebuilt = Transaction::new(
        bought.id(),
        bought.booking_date(),
        None,
        bought.splits().to_vec(),
    )
    .expect("balanced")
    .with_details_of(&read);
    assert_eq!(rebuilt.tags(), [holiday]);

    // Removing the transaction takes its tags with it, and Undo brings them
    // back.
    book.remove_transaction(bought.id()).expect("removed");
    let left: i64 = book
        .conn
        .query_row("SELECT count(*) FROM main.transaction_tags", [], |row| {
            row.get(0)
        })
        .expect("counted");
    assert_eq!(left, 0);
    book.undo().expect("undone");
    assert_eq!(book.transaction(bought.id()).expect("read"), bought);
}

#[test]
fn unknown_tags_and_too_many_tags_are_refused() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let stray = purchase(checking, groceries, vec![TagId::generate()], vec![]);
    assert!(matches!(
        book.add_transaction(&stray),
        Err(StoreError::UnknownTag(_))
    ));
    let many: Vec<TagId> = (0..=MAX_TAGS_ON_TRANSACTION)
        .map(|number| add_tag(&mut book, &format!("Tag {number}")))
        .collect();
    let on_entry = purchase(checking, groceries, many.clone(), vec![]);
    assert!(matches!(
        book.add_transaction(&on_entry),
        Err(StoreError::TooManyTags {
            limit: "transaction",
            max: MAX_TAGS_ON_TRANSACTION
        })
    ));
    let on_part = purchase(
        checking,
        groceries,
        vec![],
        many[..=MAX_TAGS_ON_PART].to_vec(),
    );
    assert!(matches!(
        book.add_transaction(&on_part),
        Err(StoreError::TooManyTags {
            limit: "part",
            max: MAX_TAGS_ON_PART
        })
    ));
    // Exactly the most is fine.
    let full = purchase(
        checking,
        groceries,
        many[..MAX_TAGS_ON_TRANSACTION].to_vec(),
        many[..MAX_TAGS_ON_PART].to_vec(),
    );
    book.add_transaction(&full).expect("added");
}

#[test]
fn tag_names_are_trimmed_unique_one_line_and_not_too_long() {
    let Sample { mut book, .. } = sample();
    let longest = "x".repeat(MAX_TAG_NAME_CHARS);
    assert_eq!(
        checked_tag_name(&format!("  {longest} ")).expect("fits"),
        longest
    );
    assert!(matches!(
        checked_tag_name(&format!("{longest}y")),
        Err(StoreError::TagName {
            reason: "too-long",
            max_chars: MAX_TAG_NAME_CHARS
        })
    ));
    // Characters, not bytes: 60 letters with umlauts fit.
    checked_tag_name(&"ä".repeat(MAX_TAG_NAME_CHARS)).expect("fits");
    assert!(matches!(
        checked_tag_name("Tax\n2026"),
        Err(StoreError::TagName {
            reason: "line-break",
            ..
        })
    ));
    assert!(matches!(checked_tag_name("  "), Err(StoreError::EmptyName)));

    let tax = add_tag(&mut book, "  Tax ");
    assert_eq!(book.tags().expect("tags")[0].name, "Tax");
    let other = add_tag(&mut book, "Holiday");
    assert!(matches!(
        book.rename_tag(other, " tax"),
        Err(StoreError::DuplicateTagName { existing }) if existing == "Tax"
    ));
    // A tag can be renamed to its own name in other capitals.
    book.rename_tag(tax, "TAX").expect("renamed");
    assert_eq!(last_action(&book), "rename_tag");
    book.undo().expect("undone");
    assert!(
        book.tags()
            .expect("tags")
            .iter()
            .any(|tag| tag.name == "Tax")
    );
    assert!(matches!(
        book.rename_tag(TagId::generate(), "Gifts"),
        Err(StoreError::UnknownTag(_))
    ));
}

#[test]
fn removing_a_tag_takes_it_off_everything_and_undo_puts_it_back() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let holiday = add_tag(&mut book, "Holiday");
    let tax = add_tag(&mut book, "Tax");
    let bought = purchase(checking, groceries, vec![holiday, tax], vec![holiday]);
    book.add_transaction(&bought).expect("added");
    assert_eq!(
        book.tag_uses().expect("uses")[&holiday],
        TagUse {
            transactions: 1,
            parts: 1
        }
    );

    book.remove_tag(holiday).expect("removed");
    assert_eq!(last_action(&book), "remove_tag");
    let read = book.transaction(bought.id()).expect("read");
    assert_eq!(read.tags(), [tax]);
    assert!(read.splits()[1].tags.is_empty());
    // No amount moves.
    let amounts = |transaction: &Transaction| -> Vec<i64> {
        transaction
            .splits()
            .iter()
            .map(|split| split.amount.minor_units())
            .collect()
    };
    assert_eq!(amounts(&read), amounts(&bought));
    assert!(!book.tag_uses().expect("uses").contains_key(&holiday));
    assert_eq!(book.tags().expect("tags").len(), 1);

    book.undo().expect("undone");
    assert_eq!(book.transaction(bought.id()).expect("read"), bought);
    assert_eq!(book.tags().expect("tags").len(), 2);
    book.redo().expect("made again");
    assert_eq!(book.tags().expect("tags").len(), 1);
}

#[test]
fn merging_tags_keeps_each_use_once_and_undo_takes_it_back() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let trip = add_tag(&mut book, "Trip");
    let holiday = add_tag(&mut book, "Holiday");
    let tax = add_tag(&mut book, "Tax");
    // One entry has both on the whole, another only "Trip", in front of
    // "Tax".
    let both = purchase(
        checking,
        groceries,
        vec![holiday, trip],
        vec![trip, holiday],
    );
    let one = purchase(checking, groceries, vec![trip, tax], vec![trip]);
    book.add_transaction(&both).expect("added");
    book.add_transaction(&one).expect("added");

    assert!(matches!(
        book.merge_tags(trip, trip),
        Err(StoreError::TagMergedIntoItself(_))
    ));
    book.merge_tags(trip, holiday).expect("merged");
    assert_eq!(last_action(&book), "merge_tags");
    let read_both = book.transaction(both.id()).expect("read");
    assert_eq!(read_both.tags(), [holiday]);
    assert_eq!(read_both.splits()[1].tags, [holiday]);
    // "Holiday" takes "Trip"'s place.
    let read_one = book.transaction(one.id()).expect("read");
    assert_eq!(read_one.tags(), [holiday, tax]);
    assert_eq!(read_one.splits()[1].tags, [holiday]);
    assert!(book.tags().expect("tags").iter().all(|tag| tag.id != trip));
    assert_eq!(
        book.tag_uses().expect("uses")[&holiday],
        TagUse {
            transactions: 2,
            parts: 2
        }
    );

    book.undo().expect("undone");
    assert_eq!(book.transaction(both.id()).expect("read"), both);
    assert_eq!(book.transaction(one.id()).expect("read"), one);
    assert_eq!(book.tags().expect("tags").len(), 3);
}

#[test]
fn a_book_keeps_at_most_its_number_of_tags() {
    let Sample { mut book, .. } = sample();
    for number in 0..kassensturz_core::MAX_TAGS_IN_BOOK {
        add_tag(&mut book, &format!("Tag {number}"));
    }
    let one_more = Tag {
        id: TagId::generate(),
        name: "One more".into(),
    };
    assert!(matches!(
        book.add_tag(&one_more),
        Err(StoreError::TooManyTags {
            limit: "book",
            max: kassensturz_core::MAX_TAGS_IN_BOOK
        })
    ));
}
