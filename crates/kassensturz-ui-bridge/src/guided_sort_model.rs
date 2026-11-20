// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Step 3 of the guided setup (#1031) as a QML object, on the book the app
//! has open (`session.rs`): the transactions without a category grouped by
//! name, and sorting a whole group at once (`guided_sort.rs`). Every amount
//! reaches QML as text (D-065).

use core::pin::Pin;

use cxx_qt_lib::{QString, QVariant};

use crate::errors::ErrorText;
use crate::guided_sort::{self, Group, Unsorted};
use crate::number_format::app_locale;
use crate::session;
use crate::transaction_entry;
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
        /// The transactions without a category, grouped by name.
        #[qobject]
        #[qml_element]
        type GuidedSort = super::GuidedSortRust;
    }

    extern "RustQt" {
        /// What the page shows: `count` (how many transactions in the whole
        /// book have no category), `more` (how many groups aren't listed)
        /// and `groups`, the largest first, each with `key`, `name` (empty
        /// for none), `moneyIn`, `count`, `total` and `exactTotal` (their
        /// sum as text), `accountId` and `transactionId` (the newest of
        /// them). With no book open: nothing.
        #[qinvokable]
        fn unsorted(self: &GuidedSort) -> QVariant;

        /// Gives every transaction of the group with `key` the category
        /// with `category_id`, as one change that one Undo takes back.
        /// Returns `sorted`, how many, or the error's `code` and parameters.
        #[qinvokable]
        fn sort(self: Pin<&mut GuidedSort>, key: &QString, category_id: &QString) -> QVariant;

        /// The book changed: a group was sorted.
        #[qsignal]
        fn changed(self: Pin<&mut GuidedSort>);
    }
}

/// The Rust side of [`qobject::GuidedSort`].
#[derive(Default)]
pub struct GuidedSortRust;

impl qobject::GuidedSort {
    fn unsorted(&self) -> QVariant {
        let locale = app_locale();
        let shown = session::with_book(|book| guided_sort::unsorted(book, &locale))
            .and_then(Result::ok)
            .unwrap_or_default();
        unsorted_object(&shown)
    }

    fn sort(mut self: Pin<&mut Self>, key: &QString, category_id: &QString) -> QVariant {
        let (key, category) = (key.to_string(), category_id.to_string());
        let locale = app_locale();
        let sorted = session::with_book_mut(|book| {
            guided_sort::sort(book, &key, &category)
                .map_err(|error| transaction_entry::error_text(book, "", &error, &locale))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match sorted {
            Ok(count) => {
                self.as_mut().changed();
                map([(
                    "sorted",
                    QVariant::from(&i32::try_from(count).unwrap_or(i32::MAX)),
                )])
            }
            Err(error) => error_object(&error),
        }
    }
}

fn count(value: usize) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

fn group_object(group: &Group) -> QVariant {
    map([
        ("key", text(&group.key)),
        ("name", text(&group.name)),
        ("moneyIn", QVariant::from(&group.money_in)),
        ("count", count(group.count)),
        ("total", text(&group.total.text)),
        ("exactTotal", text(&group.total.exact)),
        ("accountId", text(&group.account)),
        ("transactionId", text(&group.transaction)),
    ])
}

fn unsorted_object(shown: &Unsorted) -> QVariant {
    map([
        ("count", count(shown.count)),
        ("more", count(shown.more)),
        ("groups", list(shown.groups.iter().map(group_object))),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::Shown;
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, text_of};

    /// The object is read back the way QML reads it (AGENTS.md §9.4).
    #[test]
    fn the_groups_read_back_with_every_field_the_page_uses() {
        let shown = Unsorted {
            count: 7,
            more: 2,
            groups: vec![Group {
                key: "out|EUR|Corner Shop".into(),
                name: "Corner Shop".into(),
                money_in: false,
                count: 3,
                total: Shown {
                    text: "-36 €".into(),
                    exact: "-35,75 €".into(),
                },
                account: "account-1".into(),
                transaction: "transaction-9".into(),
            }],
        };
        let object = unsorted_object(&shown);
        assert_eq!(keys(&object), ["count", "groups", "more"]);
        assert_eq!(number_of(&object, "count"), 7);
        assert_eq!(number_of(&object, "more"), 2);
        let groups = list_of(&object, "groups");
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(
            keys(group),
            [
                "accountId",
                "count",
                "exactTotal",
                "key",
                "moneyIn",
                "name",
                "total",
                "transactionId"
            ]
        );
        assert_eq!(text_of(group, "key"), "out|EUR|Corner Shop");
        assert_eq!(text_of(group, "name"), "Corner Shop");
        assert!(!flag_of(group, "moneyIn"));
        assert_eq!(number_of(group, "count"), 3);
        assert_eq!(text_of(group, "total"), "-36 €");
        assert_eq!(text_of(group, "exactTotal"), "-35,75 €");
        assert_eq!(text_of(group, "accountId"), "account-1");
        assert_eq!(text_of(group, "transactionId"), "transaction-9");
    }

    #[test]
    fn nothing_to_sort_reads_back_as_an_empty_list() {
        let object = unsorted_object(&Unsorted::default());
        assert_eq!(number_of(&object, "count"), 0);
        assert!(list_of(&object, "groups").is_empty());
    }
}
