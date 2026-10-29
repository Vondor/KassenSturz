// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The My groups and Hidden categories page (M6b.5c, D-290, D-309): the
//! groups the user made, the hide list, and the changes to both.
//!
//! Like the categories screen it reads the open book directly and names
//! categories and groups to QML by their ID as text. The book makes every
//! check (D-301); this file only turns text into IDs and errors into codes.

use kassensturz_core::{AccountId, GroupId, GroupKind, GroupMember, Report};
use kassensturz_store::{Book, StoreError};

use crate::categories::{CategoryRow, view as category_view};
use crate::errors::{ErrorText, report_text};
use crate::format::Locale;

/// Why a change wasn't made.
#[derive(Debug)]
pub enum GroupsError {
    /// A category or group the screen named isn't in the book any more, e.g.
    /// it was just changed elsewhere.
    NoSuchEntry,
    /// The book refused it.
    Store(StoreError),
}

impl From<StoreError> for GroupsError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// One member of a group as the page lists it.
#[derive(Debug, PartialEq, Eq)]
pub struct MemberRow {
    /// The ID of the category or group as text.
    pub id: String,
    /// A category's path, e.g. "Food › Groceries", or a group's name.
    pub name: String,
    /// Whether it is a group, otherwise a category.
    pub group: bool,
}

/// One of the user's groups.
#[derive(Debug, PartialEq, Eq)]
pub struct GroupRow {
    /// The group's ID as text.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Whether it counts money received, otherwise money spent.
    pub income: bool,
    /// What it holds, in the order the user put it.
    pub members: Vec<MemberRow>,
}

/// Everything the page shows.
#[derive(Debug)]
pub struct GroupsView {
    /// The groups in the order of the list.
    pub groups: Vec<GroupRow>,
    /// The categories in tree order, for picking members and hiding.
    pub categories: Vec<CategoryRow>,
    /// The IDs of the hidden categories as text.
    pub hidden: Vec<String>,
}

/// The groups, the categories and the hide list of `book`.
pub fn view(book: &Book) -> Result<GroupsView, StoreError> {
    let categories = category_view(book)?.rows;
    let path_of = |id: AccountId| {
        categories
            .iter()
            .find(|row| row.id == id.to_string())
            .map(|row| row.path.clone())
    };
    let stored = book.category_groups()?;
    let groups = stored
        .iter()
        .map(|group| GroupRow {
            id: group.id.to_string(),
            name: group.name.clone(),
            income: group.kind == GroupKind::Income,
            members: group
                .members
                .iter()
                .filter_map(|member| match *member {
                    GroupMember::Category(id) => path_of(id).map(|name| MemberRow {
                        id: id.to_string(),
                        name,
                        group: false,
                    }),
                    GroupMember::Group(id) => {
                        stored
                            .iter()
                            .find(|other| other.id == id)
                            .map(|other| MemberRow {
                                id: id.to_string(),
                                name: other.name.clone(),
                                group: true,
                            })
                    }
                })
                .collect(),
        })
        .collect();
    let hidden = book
        .hidden_categories()?
        .into_iter()
        .map(|id| id.to_string())
        .collect();
    Ok(GroupsView {
        groups,
        categories,
        hidden,
    })
}

/// The members the screen named by ID, in its order: each is a category or
/// a group of the book.
fn members(book: &Book, ids: &[String]) -> Result<Vec<GroupMember>, GroupsError> {
    let categories = book.categories()?;
    let groups = book.category_groups()?;
    ids.iter()
        .map(|id| {
            groups
                .iter()
                .find(|group| group.id.to_string() == *id)
                .map(|group| GroupMember::Group(group.id))
                .or_else(|| {
                    categories
                        .iter()
                        .find(|entry| entry.category.id.to_string() == *id)
                        .map(|entry| GroupMember::Category(entry.category.id))
                })
                .ok_or(GroupsError::NoSuchEntry)
        })
        .collect()
}

fn group_id(book: &Book, id: &str) -> Result<GroupId, GroupsError> {
    book.category_groups()?
        .into_iter()
        .find(|group| group.id.to_string() == id)
        .map(|group| group.id)
        .ok_or(GroupsError::NoSuchEntry)
}

/// Makes a group of spending or income categories with these members.
pub fn add_group(
    book: &mut Book,
    name: &str,
    income: bool,
    member_ids: &[String],
) -> Result<GroupId, GroupsError> {
    let members = members(book, member_ids)?;
    let kind = if income {
        GroupKind::Income
    } else {
        GroupKind::Expense
    };
    Ok(book.add_category_group(name, kind, &members)?)
}

/// Gives a group a new name and new members.
pub fn edit_group(
    book: &mut Book,
    id: &str,
    name: &str,
    member_ids: &[String],
) -> Result<(), GroupsError> {
    let id = group_id(book, id)?;
    let members = members(book, member_ids)?;
    Ok(book.edit_category_group(id, name, &members)?)
}

/// Moves a group to `position` in the list, counted from 0.
pub fn move_group(book: &mut Book, id: &str, position: u32) -> Result<(), GroupsError> {
    let id = group_id(book, id)?;
    Ok(book.move_category_group(id, position)?)
}

/// Removes a group. Its categories stay as they are.
pub fn remove_group(book: &mut Book, id: &str) -> Result<(), GroupsError> {
    let id = group_id(book, id)?;
    Ok(book.remove_category_group(id)?)
}

/// Makes exactly these categories the hidden ones.
pub fn set_hidden(book: &mut Book, ids: &[String]) -> Result<(), GroupsError> {
    let categories = book.categories()?;
    let hidden = ids
        .iter()
        .map(|id| {
            categories
                .iter()
                .find(|entry| entry.category.id.to_string() == *id)
                .map(|entry| entry.category.id)
                .ok_or(GroupsError::NoSuchEntry)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(book.set_hidden_categories(&hidden)?)
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &GroupsError, locale: &impl Locale) -> ErrorText {
    match error {
        GroupsError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        GroupsError::Store(error) => report_text(book, &error.report(), locale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{NewCategory, add_category};
    use crate::register::tests::TestLocale;
    use kassensturz_core::CurrencyCode;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn book() -> Book {
        Book::create_in_memory().expect("book in memory")
    }

    fn add(book: &mut Book, name: &str, income: bool, parent: &str) -> String {
        let new = NewCategory {
            name,
            income,
            parent,
            currency: "",
        };
        add_category(book, &new, EUR)
            .expect("category added")
            .to_string()
    }

    fn code(book: &Book, error: &GroupsError) -> &'static str {
        error_text(book, error, &TestLocale).code
    }

    #[test]
    fn a_group_is_made_listed_changed_moved_and_removed() {
        let mut book = book();
        let food = add(&mut book, "Food", false, "");
        let groceries = add(&mut book, "Groceries", false, &food);
        let rent = add(&mut book, "Rent", false, "");
        let salary = add(&mut book, "Salary", true, "");

        let household =
            add_group(&mut book, "Household", false, std::slice::from_ref(&food)).expect("added");
        let fixed =
            add_group(&mut book, "Fixed costs", false, std::slice::from_ref(&rent)).expect("added");
        let shown = view(&book).expect("view");
        assert_eq!(shown.groups.len(), 2);
        assert_eq!(shown.groups[0].name, "Household");
        assert!(!shown.groups[0].income);
        assert_eq!(
            shown.groups[0].members,
            [MemberRow {
                id: food.clone(),
                name: "Food".into(),
                group: false
            }]
        );
        // The categories come in tree order with their paths, for the picker.
        let paths: Vec<_> = shown.categories.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["Salary", "Food", "Food › Groceries", "Rent"]);

        // A group can hold a group; a path is a member's name.
        edit_group(
            &mut book,
            &fixed.to_string(),
            "Fixed and food",
            &[rent.clone(), household.to_string(), groceries.clone()],
        )
        .expect("edited");
        let shown = view(&book).expect("view");
        let names: Vec<_> = shown.groups[1]
            .members
            .iter()
            .map(|m| (m.name.as_str(), m.group))
            .collect();
        assert_eq!(
            names,
            [
                ("Rent", false),
                ("Household", true),
                ("Food › Groceries", false)
            ]
        );
        assert_eq!(shown.groups[1].name, "Fixed and food");

        move_group(&mut book, &fixed.to_string(), 0).expect("moved");
        assert_eq!(view(&book).expect("view").groups[0].name, "Fixed and food");
        remove_group(&mut book, &household.to_string()).expect("removed");
        let shown = view(&book).expect("view");
        assert_eq!(shown.groups.len(), 1);
        // Removing a group takes it out of the group that held it; the
        // categories stay.
        assert_eq!(shown.groups[0].members.len(), 2);
        assert_eq!(shown.categories.len(), 4);
        let income = add_group(&mut book, "Pay", true, &[salary]).expect("added");
        assert!(
            view(&book)
                .expect("view")
                .groups
                .iter()
                .any(|g| g.id == income.to_string() && g.income)
        );
    }

    #[test]
    fn the_hide_list_is_set_whole_and_read_back() {
        let mut book = book();
        let food = add(&mut book, "Food", false, "");
        let rent = add(&mut book, "Rent", false, "");
        set_hidden(&mut book, &[rent.clone(), food.clone()]).expect("hidden");
        let mut hidden = view(&book).expect("view").hidden;
        hidden.sort();
        let mut expected = vec![rent.clone(), food];
        expected.sort();
        assert_eq!(hidden, expected);
        set_hidden(&mut book, std::slice::from_ref(&rent)).expect("hidden");
        assert_eq!(view(&book).expect("view").hidden, [rent]);
        set_hidden(&mut book, &[]).expect("nothing hidden");
        assert!(view(&book).expect("view").hidden.is_empty());
    }

    #[test]
    fn the_books_refusals_come_back_as_codes_and_unknown_ids_are_no_such_entry() {
        let mut book = book();
        let food = add(&mut book, "Food", false, "");
        let salary = add(&mut book, "Salary", true, "");
        let error =
            add_group(&mut book, "Mixed", false, std::slice::from_ref(&salary)).unwrap_err();
        assert_eq!(code(&book, &error), "store.group-wrong-kind");
        let error = add_group(&mut book, "Food", false, &[]).unwrap_err();
        assert_eq!(code(&book, &error), "store.group-name-taken");
        let error = add_group(&mut book, "  ", false, &[]).unwrap_err();
        assert_eq!(code(&book, &error), "store.empty-name");
        let error = add_group(&mut book, "Ghost", false, &["no-such".into()]).unwrap_err();
        assert_eq!(code(&book, &error), "input.no-such-entry");
        let error = remove_group(&mut book, "no-such").unwrap_err();
        assert_eq!(code(&book, &error), "input.no-such-entry");
        let error = set_hidden(&mut book, &["no-such".into()]).unwrap_err();
        assert_eq!(code(&book, &error), "input.no-such-entry");
        // Nothing was written by the refused changes.
        assert!(view(&book).expect("view").groups.is_empty());
        let one = add_group(&mut book, "One", false, std::slice::from_ref(&food)).expect("added");
        let error = edit_group(&mut book, &one.to_string(), "One", &[one.to_string()]).unwrap_err();
        assert_eq!(code(&book, &error), "store.group-loop");
    }

    #[test]
    fn a_removed_category_leaves_its_groups_and_the_hide_list() {
        let mut book = book();
        let food = add(&mut book, "Food", false, "");
        let rent = add(&mut book, "Rent", false, "");
        add_group(&mut book, "Both", false, &[food, rent.clone()]).expect("added");
        set_hidden(&mut book, std::slice::from_ref(&rent)).expect("hidden");
        let target = book
            .categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.category.id.to_string() == rent)
            .expect("Rent")
            .category
            .id;
        book.remove_category(target).expect("removed");
        let shown = view(&book).expect("view");
        assert_eq!(shown.groups[0].members.len(), 1);
        assert!(shown.hidden.is_empty());
    }
}
