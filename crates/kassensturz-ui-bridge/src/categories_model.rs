// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The categories screen as a QML object (M3.3), on the book the app has
//! open (`session.rs`).
//!
//! QML gets everything as text, flags and lists of them (D-065). Categories
//! are named by their ID as text. Every change asks the other screens to read
//! the book again through the `changed` signal.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::CurrencyCode;
use kassensturz_store::Book;

use crate::accounts::usual_currency;
use crate::categories::{
    self, CategoryError, CategoryRow, CategoryView, NewCategory, ParentChoice,
};
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
        /// The categories of the book the app has open.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type CategoryTree = super::CategoryTreeRust;
    }

    extern "RustQt" {
        /// Reads the open book again, e.g. after another screen changed it.
        #[qinvokable]
        fn refresh(self: Pin<&mut CategoryTree>);

        /// The categories a category can go inside, as a list of `id`,
        /// `path` and `depth`: those for income or for spending. With
        /// `moving`, the ID of the category to move, it and what is inside it
        /// are left out. An error's `code` if `moving` isn't there.
        #[qinvokable]
        #[cxx_name = "parentChoices"]
        fn parent_choices(self: &CategoryTree, income: bool, moving: &QString) -> QVariant;

        /// Adds a category inside `parent` (an ID, empty for the top level).
        /// Returns an empty map, or the error's `code` and its parameters.
        #[qinvokable]
        #[cxx_name = "addCategory"]
        fn add_category(
            self: Pin<&mut CategoryTree>,
            name: &QString,
            income: bool,
            parent: &QString,
            currency: &QString,
        ) -> QVariant;

        /// Gives a category a new name. Returns an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "renameCategory"]
        fn rename_category(self: Pin<&mut CategoryTree>, id: &QString, name: &QString) -> QVariant;

        /// Moves a category inside `parent` (an ID, empty for the top
        /// level). Returns an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "moveCategory"]
        fn move_category(self: Pin<&mut CategoryTree>, id: &QString, parent: &QString) -> QVariant;
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut CategoryTree>);
    }

    impl cxx_qt::Initialize for CategoryTree {}
}

/// The Rust side of [`qobject::CategoryTree`].
#[derive(Default)]
pub struct CategoryTreeRust {
    view: QVariant,
}

impl cxx_qt::Initialize for qobject::CategoryTree {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::CategoryTree {
    fn refresh(mut self: Pin<&mut Self>) {
        let read = session::with_book(categories::view);
        let view = match read {
            Some(Ok(view)) => view,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the categories: {error:?}");
                return;
            }
            None => return,
        };
        self.as_mut().rust_mut().view = view_object(&view);
        self.as_mut().view_changed();
    }

    fn parent_choices(&self, income: bool, moving: &QString) -> QVariant {
        let locale = app_locale();
        let moving = moving.to_string();
        let result = session::with_book(|book| {
            categories::parent_choices(book, income, &moving)
                .map_err(|error| categories::error_text(book, &error, &locale))
        });
        match result {
            Some(Ok(choices)) => list(choices.iter().map(choice_object)),
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn add_category(
        self: Pin<&mut Self>,
        name: &QString,
        income: bool,
        parent: &QString,
        currency: &QString,
    ) -> QVariant {
        let (name, parent, currency) = (name.to_string(), parent.to_string(), currency.to_string());
        let new = NewCategory {
            name: &name,
            income,
            parent: &parent,
            currency: &currency,
        };
        let result = change(|book| {
            let usual = usual_currency(book, fallback_currency())?;
            categories::add_category(book, &new, usual).map(drop)
        });
        self.finish(result)
    }

    fn rename_category(self: Pin<&mut Self>, id: &QString, name: &QString) -> QVariant {
        let (id, name) = (id.to_string(), name.to_string());
        let result = change(|book| categories::rename_category(book, &id, &name));
        self.finish(result)
    }

    fn move_category(self: Pin<&mut Self>, id: &QString, parent: &QString) -> QVariant {
        let (id, parent) = (id.to_string(), parent.to_string());
        let result = change(|book| categories::move_category(book, &id, &parent));
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
fn change<T>(f: impl FnOnce(&mut Book) -> Result<T, CategoryError>) -> Result<T, ErrorText> {
    let locale: UserLocale<QtLocale> = app_locale();
    session::with_book_mut(|book| {
        f(book).map_err(|error| categories::error_text(book, &error, &locale))
    })
    .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

/// The currency of a category that goes in no other category, in a book
/// without accounts: the regional settings' if KassenSturz knows it,
/// otherwise the euro.
fn fallback_currency() -> CurrencyCode {
    QtLocale::currency()
        .or_else(|| CurrencyCode::new("EUR").ok())
        .expect("EUR is a valid code")
}

fn view_object(view: &CategoryView) -> QVariant {
    map([
        ("rows", list(view.rows.iter().map(row_object))),
        (
            "severalCurrencies",
            QVariant::from(&view.several_currencies),
        ),
    ])
}

fn row_object(row: &CategoryRow) -> QVariant {
    map([
        ("id", text(&row.id)),
        ("name", text(&row.name)),
        ("path", text(&row.path)),
        (
            "depth",
            QVariant::from(&i32::try_from(row.depth).unwrap_or(i32::MAX)),
        ),
        ("parent", text(&row.parent)),
        ("income", QVariant::from(&row.income)),
        ("currency", text(&row.currency)),
    ])
}

fn choice_object(choice: &ParentChoice) -> QVariant {
    map([
        ("id", text(&choice.id)),
        ("path", text(&choice.path)),
        (
            "depth",
            QVariant::from(&i32::try_from(choice.depth).unwrap_or(i32::MAX)),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use cxx_qt_lib::{QMap, QMapPair_QString_QVariant};

    use super::*;
    use crate::first_start::tests::started;

    fn field(object: &QVariant, key: &str) -> String {
        object
            .value::<QMap<QMapPair_QString_QVariant>>()
            .and_then(|map| map.get(&QString::from(key)))
            .and_then(|value| value.value::<QString>())
            .map(|value| value.to_string())
            .unwrap_or_else(|| panic!("the row has no text `{key}`"))
    }

    /// The Categories screen opens a category with the rows it is given, so
    /// the one a category sits inside must be among them (#76).
    #[test]
    fn a_row_tells_the_category_it_sits_inside() {
        let book = started();
        let shown = categories::view(&book).expect("view");
        let food = shown
            .rows
            .iter()
            .find(|row| row.name == "Food")
            .expect("Food");
        let groceries = shown
            .rows
            .iter()
            .find(|row| row.name == "Groceries")
            .expect("Groceries");
        assert_eq!(field(&row_object(food), "parent"), "");
        assert_eq!(field(&row_object(groceries), "parent"), food.id);
        for row in &shown.rows {
            let object = row_object(row);
            for key in ["id", "name", "path", "currency", "parent"] {
                field(&object, key);
            }
        }
    }
}
