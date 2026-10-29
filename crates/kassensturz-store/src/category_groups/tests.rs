// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of category groups and hidden categories (D-290) on books in
//! memory. All names are made up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{AccountId, AccountKind, CategoryGroup, GroupId, GroupKind, GroupMember};

use crate::tests::{EUR, Sample, account, category_in, sample};
use crate::{Action, Book, HistoryRetention, StoreError};

use GroupMember::{Category, Group};

/// Compares the error `result` holds with `expected` (`StoreError` has no
/// `PartialEq`, as it can hold an SQLite error).
#[track_caller]
fn assert_refused<T: std::fmt::Debug>(result: Result<T, StoreError>, expected: StoreError) {
    assert_eq!(
        format!("{:?}", result.err()),
        format!("{:?}", Some(expected))
    );
}

fn spending(book: &mut Book, name: &str) -> AccountId {
    account(book, name, AccountKind::Expense, EUR)
}

fn names(book: &Book) -> Vec<(String, u32)> {
    book.category_groups()
        .expect("read")
        .into_iter()
        .map(|group| (group.name, group.position))
        .collect()
}

fn listed(items: &[(&str, u32)]) -> Vec<(String, u32)> {
    items
        .iter()
        .map(|&(name, position)| (name.to_owned(), position))
        .collect()
}

fn members(book: &Book, id: GroupId) -> Vec<GroupMember> {
    book.category_group(id).expect("read").members
}

/// The rows of the three tables as they are, to compare a book before and
/// after an undo.
fn rows(book: &Book) -> (Vec<CategoryGroup>, Vec<AccountId>) {
    (
        book.category_groups().expect("read"),
        book.hidden_categories().expect("read"),
    )
}

#[test]
fn a_group_is_kept_with_its_members_in_their_order() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    assert_eq!(book.category_groups().expect("read"), Vec::new());
    let rent = spending(&mut book, "Rent");
    let phone = spending(&mut book, "Phone");
    // Spaces around the name are dropped; a member given twice keeps its
    // first place.
    let fixed = book
        .add_category_group(
            "  Fixed costs ",
            GroupKind::Expense,
            &[Category(rent), Category(phone), Category(rent)],
        )
        .expect("added");
    assert_eq!(
        book.category_group(fixed).expect("read"),
        CategoryGroup {
            id: fixed,
            name: "Fixed costs".into(),
            kind: GroupKind::Expense,
            position: 0,
            members: vec![Category(rent), Category(phone)],
        }
    );
    assert_eq!(book.history().expect("history")[0].action, Action::AddGroup);
    let household = book
        .add_category_group(
            "Household",
            GroupKind::Expense,
            &[Category(groceries), Group(fixed)],
        )
        .expect("added");
    assert_eq!(
        members(&book, household),
        [Category(groceries), Group(fixed)]
    );
    assert_eq!(
        names(&book),
        listed(&[("Fixed costs", 0), ("Household", 1)])
    );
    // Nothing is written onto the categories (D-290).
    assert_eq!(book.account(rent).expect("read").name, "Rent");
}

#[test]
fn an_unknown_group_is_named_in_the_error() {
    let mut book = sample().book;
    let history = book.history().expect("history");
    let missing = GroupId::generate();
    assert_refused(
        book.category_group(missing),
        StoreError::UnknownGroup(missing),
    );
    assert_refused(
        book.edit_category_group(missing, "Anything", &[]),
        StoreError::UnknownGroup(missing),
    );
    assert_refused(
        book.move_category_group(missing, 0),
        StoreError::UnknownGroup(missing),
    );
    assert_refused(
        book.remove_category_group(missing),
        StoreError::UnknownGroup(missing),
    );
    assert_refused(
        book.add_category_group("Nested", GroupKind::Expense, &[Group(missing)]),
        StoreError::UnknownGroup(missing),
    );
    assert_eq!(book.history().expect("history"), history);
}

#[test]
fn every_change_to_the_groups_can_be_undone_and_redone() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let rent = spending(&mut book, "Rent");
    let phone = spending(&mut book, "Phone");
    let fixed = book
        .add_category_group("Fixed costs", GroupKind::Expense, &[Category(rent)])
        .expect("added");
    let food = book
        .add_category_group("Food", GroupKind::Expense, &[Category(groceries)])
        .expect("added");
    let household = book
        .add_category_group(
            "Household",
            GroupKind::Expense,
            &[Group(fixed), Group(food)],
        )
        .expect("added");
    let before_edit = rows(&book);
    book.edit_category_group(
        fixed,
        "Fixed monthly costs",
        &[Category(phone), Category(rent)],
    )
    .expect("edited");
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::EditGroup
    );
    assert_eq!(members(&book, fixed), [Category(phone), Category(rent)]);
    let after_edit = rows(&book);
    book.undo().expect("undone");
    assert_eq!(rows(&book), before_edit);
    book.redo().expect("redone");
    assert_eq!(rows(&book), after_edit);

    book.move_category_group(household, 0).expect("moved");
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::MoveGroup
    );
    assert_eq!(
        names(&book),
        listed(&[("Household", 0), ("Fixed monthly costs", 1), ("Food", 2)])
    );
    book.undo().expect("undone");
    assert_eq!(rows(&book), after_edit);
    book.redo().expect("redone");

    // Removing a group takes its place in the groups that hold it with it;
    // the groups behind it move up.
    let moved = rows(&book);
    book.remove_category_group(fixed).expect("removed");
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::RemoveGroup
    );
    assert_eq!(names(&book), listed(&[("Household", 0), ("Food", 1)]));
    assert_eq!(members(&book, household), [Group(food)]);
    book.undo().expect("undone");
    assert_eq!(rows(&book), moved);
    // The categories were never touched.
    assert_eq!(book.account(rent).expect("read").name, "Rent");
}

#[test]
fn an_edit_that_changes_nothing_records_nothing() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let food = book
        .add_category_group("Food", GroupKind::Expense, &[Category(groceries)])
        .expect("added");
    let history = book.history().expect("history").len();
    book.edit_category_group(food, " Food ", &[Category(groceries)])
        .expect("edited");
    book.move_category_group(food, 7).expect("moved");
    book.set_hidden_categories(&[]).expect("set");
    assert_eq!(book.history().expect("history").len(), history);
}

#[test]
fn a_removed_category_takes_its_rows_with_it_and_its_undo_brings_them_back() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let pets = spending(&mut book, "Pets");
    let food = book
        .add_category_group(
            "Food and pets",
            GroupKind::Expense,
            &[Category(groceries), Category(pets)],
        )
        .expect("added");
    book.set_hidden_categories(&[pets]).expect("hidden");
    let before = rows(&book);
    book.remove_category(pets).expect("removed");
    assert_eq!(members(&book, food), [Category(groceries)]);
    assert_eq!(book.hidden_categories().expect("read"), Vec::new());
    // One change: the category, its place in the group and its hide row.
    let history = book.history().expect("history");
    assert_eq!(history[0].action, Action::RemoveCategory);
    book.undo().expect("undone");
    assert_eq!(rows(&book), before);
    book.redo().expect("redone");
    assert_eq!(members(&book, food), [Category(groceries)]);
    assert_eq!(book.hidden_categories().expect("read"), Vec::new());
}

#[test]
fn names_are_checked() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    assert_refused(
        book.add_category_group("  ", GroupKind::Expense, &[]),
        StoreError::EmptyName,
    );
    let long = "x".repeat(201);
    assert_refused(
        book.add_category_group(&long, GroupKind::Expense, &[]),
        StoreError::GroupNameTooLong { max_chars: 200 },
    );
    // 200 characters, some of them outside ASCII, are fine.
    let longest = "ü".repeat(200);
    book.add_category_group(&longest, GroupKind::Expense, &[])
        .expect("added");
    let fixed = book
        .add_category_group("Fixed costs", GroupKind::Expense, &[])
        .expect("added");
    // Another group's name, ignoring capitals, also outside ASCII.
    assert_refused(
        book.add_category_group("FIXED COSTS", GroupKind::Income, &[]),
        StoreError::GroupNameTaken {
            name: "FIXED COSTS".into(),
        },
    );
    assert_refused(
        book.add_category_group(&"Ü".repeat(200), GroupKind::Income, &[]),
        StoreError::GroupNameTaken {
            name: "Ü".repeat(200),
        },
    );
    // A category's name, so a formula's name stays clear (D-292).
    assert_refused(
        book.add_category_group("groceries", GroupKind::Expense, &[]),
        StoreError::GroupNameTaken {
            name: "groceries".into(),
        },
    );
    assert_refused(
        book.edit_category_group(fixed, "Salary", &[]),
        StoreError::GroupNameTaken {
            name: "Salary".into(),
        },
    );
    // A category made later may have a group's name; the group keeps it and
    // can still change its capitals and members.
    let later = spending(&mut book, "Fixed Costs");
    book.edit_category_group(fixed, "FIXED costs", &[Category(groceries)])
        .expect("edited");
    assert_eq!(
        book.category_group(fixed).expect("read").name,
        "FIXED costs"
    );
    assert_ne!(later, groceries);
}

#[test]
fn members_must_be_categories_of_the_groups_kind() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    let missing = AccountId::generate();
    assert_refused(
        book.add_category_group("Food", GroupKind::Expense, &[Category(missing)]),
        StoreError::UnknownAccount(missing),
    );
    assert_refused(
        book.add_category_group("Food", GroupKind::Expense, &[Category(checking)]),
        StoreError::NotACategory(checking),
    );
    assert_refused(
        book.add_category_group(
            "Food",
            GroupKind::Expense,
            &[Category(groceries), Category(salary)],
        ),
        StoreError::GroupWrongKind {
            name: "Salary".into(),
        },
    );
    let income = book
        .add_category_group("Earned", GroupKind::Income, &[Category(salary)])
        .expect("added");
    assert_refused(
        book.add_category_group("Food", GroupKind::Expense, &[Group(income)]),
        StoreError::GroupWrongKind {
            name: "Earned".into(),
        },
    );
    // A subcategory is a category like any other; it counts with its parent
    // in the reports, not here (D-291).
    let food = category_in(&mut book, "Food", AccountKind::Expense, None);
    let snacks = category_in(&mut book, "Snacks", AccountKind::Expense, Some(food));
    let group = book
        .add_category_group(
            "Eating",
            GroupKind::Expense,
            &[Category(food), Category(snacks)],
        )
        .expect("added");
    assert_eq!(members(&book, group), [Category(food), Category(snacks)]);
    // Only the group added last is in the history; the refused ones left
    // nothing behind.
    assert_eq!(names(&book), listed(&[("Earned", 0), ("Eating", 1)]));
}

#[test]
fn a_group_never_ends_up_inside_itself() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let inner = book
        .add_category_group("Inner", GroupKind::Expense, &[Category(groceries)])
        .expect("added");
    let middle = book
        .add_category_group("Middle", GroupKind::Expense, &[Group(inner)])
        .expect("added");
    let outer = book
        .add_category_group("Outer", GroupKind::Expense, &[Group(middle)])
        .expect("added");
    assert_refused(
        book.edit_category_group(inner, "Inner", &[Group(inner)]),
        StoreError::GroupLoop {
            name: "Inner".into(),
        },
    );
    let before = rows(&book);
    let history = book.history().expect("history").len();
    assert_refused(
        book.edit_category_group(inner, "Inner", &[Category(groceries), Group(outer)]),
        StoreError::GroupLoop {
            name: "Inner".into(),
        },
    );
    // The refused edit left nothing behind.
    assert_eq!(rows(&book), before);
    assert_eq!(book.history().expect("history").len(), history);
}

#[test]
fn groups_go_at_most_four_levels_deep() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let mut inner = book
        .add_category_group("Level 1", GroupKind::Expense, &[Category(groceries)])
        .expect("added");
    for level in 2..=4 {
        inner = book
            .add_category_group(
                &format!("Level {level}"),
                GroupKind::Expense,
                &[Group(inner)],
            )
            .expect("added");
    }
    assert_refused(
        book.add_category_group("Level 5", GroupKind::Expense, &[Group(inner)]),
        StoreError::GroupTooDeep { max_levels: 4 },
    );
    // Also when the deepest group grows from below: a group put into
    // "Level 1" would make "Level 4" five levels deep.
    let lowest = book.category_groups().expect("read")[0].id;
    let side = book
        .add_category_group("Side", GroupKind::Expense, &[])
        .expect("added");
    assert_refused(
        book.edit_category_group(lowest, "Level 1", &[Group(side)]),
        StoreError::GroupTooDeep { max_levels: 4 },
    );
}

#[test]
fn a_book_keeps_at_most_200_groups_of_at_most_500_members() {
    let mut book = sample().book;
    for number in 0..200 {
        book.add_category_group(&format!("Group {number}"), GroupKind::Expense, &[])
            .expect("added");
    }
    assert_refused(
        book.add_category_group("One more", GroupKind::Expense, &[]),
        StoreError::TooManyGroups { max: 200 },
    );
    let first = book.category_groups().expect("read")[0].id;
    let categories: Vec<GroupMember> = (0..501)
        .map(|number| Category(spending(&mut book, &format!("Category {number}"))))
        .collect();
    assert_refused(
        book.edit_category_group(first, "Group 0", &categories),
        StoreError::TooManyGroupMembers { max: 500 },
    );
    book.edit_category_group(first, "Group 0", &categories[..500])
        .expect("edited");
    assert_eq!(members(&book, first).len(), 500);
}

#[test]
fn hidden_categories_are_a_set_of_categories() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    assert_eq!(book.hidden_categories().expect("read"), Vec::new());
    book.set_hidden_categories(&[groceries, salary, groceries])
        .expect("set");
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::SetHiddenCategories
    );
    let mut both = vec![groceries, salary];
    both.sort_by_key(|id| id.to_bytes());
    assert_eq!(book.hidden_categories().expect("read"), both);
    book.set_hidden_categories(&[salary]).expect("set");
    assert_eq!(book.hidden_categories().expect("read"), [salary]);
    book.undo().expect("undone");
    assert_eq!(book.hidden_categories().expect("read"), both);
    let missing = AccountId::generate();
    assert_refused(
        book.set_hidden_categories(&[missing]),
        StoreError::UnknownAccount(missing),
    );
    assert_refused(
        book.set_hidden_categories(&[groceries, checking]),
        StoreError::NotACategory(checking),
    );
    assert_eq!(book.hidden_categories().expect("read"), both);
}

#[test]
fn a_hidden_category_stays_in_its_groups() {
    // Q-225 = A: hiding only takes a category out where it is shown on its
    // own; the groups keep it.
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let food = book
        .add_category_group("Food", GroupKind::Expense, &[Category(groceries)])
        .expect("added");
    book.set_hidden_categories(&[groceries]).expect("hidden");
    assert_eq!(members(&book, food), [Category(groceries)]);
}

#[test]
fn groups_come_back_with_their_ids_when_a_book_is_filled() {
    let mut book = Book::create_in_memory().expect("book created");
    let first = GroupId::generate();
    let second = GroupId::generate();
    let groceries = AccountId::generate();
    let groups = vec![
        // "Household" holds a group that comes later in the list.
        CategoryGroup {
            id: first,
            name: "Household".into(),
            kind: GroupKind::Expense,
            position: 0,
            members: vec![Group(second), Category(groceries)],
        },
        // A group may share a category's name in a book it comes from.
        CategoryGroup {
            id: second,
            name: "Groceries".into(),
            kind: GroupKind::Expense,
            position: 1,
            members: vec![Category(groceries)],
        },
    ];
    book.fill_new(HistoryRetention::Forever, |book| {
        book.use_currency(EUR)?;
        add_with_id(book, groceries, "Groceries")?;
        book.restore_category_groups(&groups)?;
        book.set_hidden_categories(&[groceries])
    })
    .expect("filled");
    assert_eq!(book.category_groups().expect("read"), groups);
    assert_eq!(book.hidden_categories().expect("read"), [groceries]);
    assert_eq!(book.history().expect("history"), Vec::new());
    assert!(!book.is_empty().expect("read"));

    // A loop among the groups of a file is refused, and the book stays
    // empty.
    let mut looped = groups.clone();
    looped[1].members = vec![Group(first)];
    let mut empty = Book::create_in_memory().expect("book created");
    let filled = empty.fill_new(HistoryRetention::Forever, |book| {
        book.use_currency(EUR)?;
        add_with_id(book, groceries, "Groceries")?;
        book.restore_category_groups(&looped)
    });
    assert_refused(
        filled,
        StoreError::GroupLoop {
            name: "Household".into(),
        },
    );
    assert!(empty.is_empty().expect("read"));
}

fn add_with_id(book: &mut Book, id: AccountId, name: &str) -> Result<(), StoreError> {
    book.add_account(&kassensturz_core::Account {
        id,
        name: name.into(),
        kind: AccountKind::Expense,
        currency: EUR,
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
}
