// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The My groups and Hidden categories page as a QML object (M6b.5c, D-309),
//! on the book the app has open (`session.rs`).
//!
//! QML gets everything as text, flags and lists of them (D-065). Categories
//! and groups are named by their ID as text, and a list of IDs comes as one
//! text with commas between them. Every change asks the other screens to read
//! the book again through the `changed` signal.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_store::Book;

use crate::category_groups::{self, GroupRow, GroupsError, GroupsView, MemberRow};
use crate::errors::ErrorText;
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The groups and the hide list of the book the app has open.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type CategoryGroups = super::CategoryGroupsRust;
    }

    extern "RustQt" {
        /// Reads the open book again, e.g. after another screen changed it.
        #[qinvokable]
        fn refresh(self: Pin<&mut CategoryGroups>);

        /// Makes a group with `members`, IDs of categories and groups
        /// separated by commas. Returns an empty map, or the error's `code`
        /// and its parameters.
        #[qinvokable]
        #[cxx_name = "addGroup"]
        fn add_group(
            self: Pin<&mut CategoryGroups>,
            name: &QString,
            income: bool,
            members: &QString,
        ) -> QVariant;

        /// Gives a group a new name and new members. Returns an empty map,
        /// or the error.
        #[qinvokable]
        #[cxx_name = "editGroup"]
        fn edit_group(
            self: Pin<&mut CategoryGroups>,
            id: &QString,
            name: &QString,
            members: &QString,
        ) -> QVariant;

        /// Moves a group to `position` in the list, counted from 0. Returns
        /// an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "moveGroup"]
        fn move_group(self: Pin<&mut CategoryGroups>, id: &QString, position: i32) -> QVariant;

        /// Removes a group; its categories stay. Returns an empty map, or the
        /// error.
        #[qinvokable]
        #[cxx_name = "removeGroup"]
        fn remove_group(self: Pin<&mut CategoryGroups>, id: &QString) -> QVariant;

        /// Makes exactly the categories in `ids` (separated by commas) the
        /// hidden ones. Returns an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "setHidden"]
        fn set_hidden(self: Pin<&mut CategoryGroups>, ids: &QString) -> QVariant;
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut CategoryGroups>);
    }

    impl cxx_qt::Initialize for CategoryGroups {}
}

/// The Rust side of [`qobject::CategoryGroups`].
#[derive(Default)]
pub struct CategoryGroupsRust {
    view: QVariant,
}

impl cxx_qt::Initialize for qobject::CategoryGroups {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

/// The IDs in a text with commas between them.
fn ids(text: &QString) -> Vec<String> {
    text.to_string()
        .split(',')
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}

impl qobject::CategoryGroups {
    fn refresh(mut self: Pin<&mut Self>) {
        let read = session::with_book(category_groups::view);
        let view = match read {
            Some(Ok(view)) => view,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the groups: {error:?}");
                return;
            }
            None => return,
        };
        self.as_mut().rust_mut().view = view_object(&view);
        self.as_mut().view_changed();
    }

    fn add_group(
        self: Pin<&mut Self>,
        name: &QString,
        income: bool,
        members: &QString,
    ) -> QVariant {
        let (name, members) = (name.to_string(), ids(members));
        let result =
            change(|book| category_groups::add_group(book, &name, income, &members).map(drop));
        self.finish(result)
    }

    fn edit_group(
        self: Pin<&mut Self>,
        id: &QString,
        name: &QString,
        members: &QString,
    ) -> QVariant {
        let (id, name, members) = (id.to_string(), name.to_string(), ids(members));
        let result = change(|book| category_groups::edit_group(book, &id, &name, &members));
        self.finish(result)
    }

    fn move_group(self: Pin<&mut Self>, id: &QString, position: i32) -> QVariant {
        let id = id.to_string();
        let position = u32::try_from(position).unwrap_or(0);
        let result = change(|book| category_groups::move_group(book, &id, position));
        self.finish(result)
    }

    fn remove_group(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        let result = change(|book| category_groups::remove_group(book, &id));
        self.finish(result)
    }

    fn set_hidden(self: Pin<&mut Self>, chosen: &QString) -> QVariant {
        let chosen = ids(chosen);
        let result = change(|book| category_groups::set_hidden(book, &chosen));
        self.finish(result)
    }

    /// Shows the new state after a change, or returns why it failed.
    fn finish(mut self: Pin<&mut Self>, result: Result<(), ErrorText>) -> QVariant {
        match result {
            Ok(()) => {
                self.as_mut().refresh();
                self.as_mut().changed();
                map([])
            }
            Err(error) => error_object(&error),
        }
    }
}

/// Changes the open book, or says why not.
fn change<T>(f: impl FnOnce(&mut Book) -> Result<T, GroupsError>) -> Result<T, ErrorText> {
    let locale: UserLocale<QtLocale> = app_locale();
    session::with_book_mut(|book| {
        f(book).map_err(|error| category_groups::error_text(book, &error, &locale))
    })
    .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

fn view_object(view: &GroupsView) -> QVariant {
    map([
        (
            "accounts",
            list(view.accounts.iter().map(|row| {
                map([
                    ("id", text(&row.id)),
                    ("name", text(&row.name)),
                    ("closed", QVariant::from(&row.closed)),
                ])
            })),
        ),
        ("groups", list(view.groups.iter().map(group_object))),
        (
            "categories",
            list(view.categories.iter().map(|row| {
                map([
                    ("id", text(&row.id)),
                    ("name", text(&row.name)),
                    ("path", text(&row.path)),
                    (
                        "depth",
                        QVariant::from(&i32::try_from(row.depth).unwrap_or(i32::MAX)),
                    ),
                    ("income", QVariant::from(&row.income)),
                    ("hidden", QVariant::from(&view.hidden.contains(&row.id))),
                ])
            })),
        ),
    ])
}

fn group_object(group: &GroupRow) -> QVariant {
    map([
        ("id", text(&group.id)),
        ("name", text(&group.name)),
        ("income", QVariant::from(&group.income)),
        ("members", list(group.members.iter().map(member_object))),
    ])
}

fn member_object(member: &MemberRow) -> QVariant {
    map([
        ("id", text(&member.id)),
        ("name", text(&member.name)),
        ("group", QVariant::from(&member.group)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{NewCategory, add_category};
    use crate::variant::read_back::{flag_of, keys, list_of, text_of};
    use kassensturz_core::CurrencyCode;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// What QML reads from the page's view, built from the real output of
    /// the logic over a real book.
    #[test]
    fn the_view_reads_back_the_way_qml_reads_it() {
        let mut book = Book::create_in_memory().expect("book");
        let mut make = |name: &str, income: bool| {
            let new = NewCategory {
                name,
                income,
                parent: "",
                currency: "",
            };
            add_category(&mut book, &new, EUR)
                .expect("category")
                .to_string()
        };
        let rent = make("Rent", false);
        let food = make("Food", false);
        category_groups::add_group(&mut book, "Fixed costs", false, std::slice::from_ref(&rent))
            .expect("group");
        category_groups::set_hidden(&mut book, std::slice::from_ref(&food)).expect("hidden");
        let view = view_object(&category_groups::view(&book).expect("view"));
        assert_eq!(keys(&view), ["accounts", "categories", "groups"]);
        let groups = list_of(&view, "groups");
        assert_eq!(groups.len(), 1);
        assert_eq!(keys(&groups[0]), ["id", "income", "members", "name"]);
        assert_eq!(text_of(&groups[0], "name"), "Fixed costs");
        assert!(!flag_of(&groups[0], "income"));
        let members = list_of(&groups[0], "members");
        assert_eq!(keys(&members[0]), ["group", "id", "name"]);
        assert_eq!(text_of(&members[0], "id"), rent);
        assert_eq!(text_of(&members[0], "name"), "Rent");
        assert!(!flag_of(&members[0], "group"));
        let categories = list_of(&view, "categories");
        assert_eq!(
            keys(&categories[0]),
            ["depth", "hidden", "id", "income", "name", "path"]
        );
        let hidden: Vec<(String, bool)> = categories
            .iter()
            .map(|row| (text_of(row, "name"), flag_of(row, "hidden")))
            .collect();
        assert_eq!(hidden, [("Food".into(), true), ("Rent".into(), false)]);
    }

    #[test]
    fn a_text_of_ids_is_split_at_commas_and_empty_means_none() {
        assert_eq!(ids(&QString::from("a,b,c")), ["a", "b", "c"]);
        assert!(ids(&QString::from("")).is_empty());
        assert_eq!(ids(&QString::from(",a,,b,")), ["a", "b"]);
    }
}
