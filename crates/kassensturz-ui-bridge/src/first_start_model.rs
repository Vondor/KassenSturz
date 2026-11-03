// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first start (M3.4) as a QML object, on the book the app has open
//! (`session.rs`): whether the first-run steps are done, which currency to
//! ask for, and writing the default categories QML names in the user's
//! language (`first_start.rs`).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QString, QStringList, QVariant};
use kassensturz_core::CurrencyCode;

use crate::accounts::{NewAccount, type_from_key};
use crate::errors::ErrorText;
use crate::first_start::{self, DefaultCategory};
use crate::locale::QtLocale;
use crate::number_format::app_locale;
use crate::session;
use crate::variant::{error_object, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        /// A list of strings.
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// Where the open book stands at the start of the app.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, has_account, cxx_name = "hasAccount", READ, NOTIFY)]
        #[qproperty(bool, needs_currency, cxx_name = "needsCurrency", READ, NOTIFY)]
        #[qproperty(QStringList, currencies, READ, NOTIFY)]
        type FirstStart = super::FirstStartRust;
    }

    extern "RustQt" {
        /// Reads the open book again.
        #[qinvokable]
        fn refresh(self: Pin<&mut FirstStart>);

        /// Completes the first start with the household's `currency` (an
        /// ISO 4217 code) and the default categories in the user's language:
        /// the income ones and the spending ones, each as the names and,
        /// alongside, the name of the category each sits inside (empty for a
        /// top-level one, which must come first). Returns an empty map, or
        /// the error's `code` and its parameters.
        #[qinvokable]
        fn start(
            self: Pin<&mut FirstStart>,
            currency: &QString,
            income_names: &QStringList,
            income_parents: &QStringList,
            spending_names: &QStringList,
            spending_parents: &QStringList,
        ) -> QVariant;

        /// The setup wizard's "Create my household book" (M7a.9): the
        /// household's `currency` and the default categories as for `start`,
        /// and the accounts as `rows`: five strings per account, as typed,
        /// one after the other: its type key (`AccountTypes.qml`), name,
        /// currency, balance and day. Everything is written in one user
        /// action, or nothing is. Returns an empty map, or the error's
        /// `code`, its parameters and `row`, the account (from 0) it is
        /// about, or -1.
        #[qinvokable]
        fn finish(
            self: Pin<&mut FirstStart>,
            currency: &QString,
            income_names: &QStringList,
            income_parents: &QStringList,
            spending_names: &QStringList,
            spending_parents: &QStringList,
            rows: &QStringList,
        ) -> QVariant;

        /// Checks one account row of the setup wizard as typed, without
        /// writing anything. Returns an empty map, or the error's `code` and
        /// its parameters.
        #[qinvokable]
        #[cxx_name = "checkAccount"]
        fn check_account(
            self: Pin<&mut FirstStart>,
            account_type: &QString,
            name: &QString,
            currency: &QString,
            balance: &QString,
            date: &QString,
        ) -> QVariant;
    }

    impl cxx_qt::Initialize for FirstStart {}
}

/// The Rust side of [`qobject::FirstStart`].
#[derive(Default)]
pub struct FirstStartRust {
    has_account: bool,
    needs_currency: bool,
    currencies: QStringList,
}

impl cxx_qt::Initialize for qobject::FirstStart {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::FirstStart {
    fn refresh(mut self: Pin<&mut Self>) {
        let regional = regional_currency();
        let read = session::with_book(|book| first_start::state(book, regional));
        let state = match read {
            Some(Ok(state)) => state,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the open book: {error:?}");
                return;
            }
            None => return,
        };
        let mut model = self.as_mut().rust_mut();
        model.has_account = state.has_account;
        model.needs_currency = state.needs_currency;
        model.currencies = state
            .currencies
            .iter()
            .map(|currency| QString::from(currency.as_str()))
            .collect();
        self.as_mut().has_account_changed();
        self.as_mut().needs_currency_changed();
        self.as_mut().currencies_changed();
    }

    fn start(
        mut self: Pin<&mut Self>,
        currency: &QString,
        income_names: &QStringList,
        income_parents: &QStringList,
        spending_names: &QStringList,
        spending_parents: &QStringList,
    ) -> QVariant {
        let lists = [
            income_names,
            income_parents,
            spending_names,
            spending_parents,
        ]
        .map(strings);
        let Some(categories) = default_categories(&lists) else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        let currency = currency.to_string();
        let locale = app_locale();
        let result = session::with_book_mut(|book| {
            first_start::start(book, &currency, &categories)
                .map_err(|error| first_start::error_text(book, &error, &locale))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match result {
            Ok(()) => {
                self.as_mut().refresh();
                map([])
            }
            Err(error) => error_object(&error),
        }
    }
}

impl qobject::FirstStart {
    fn finish(
        mut self: Pin<&mut Self>,
        currency: &QString,
        income_names: &QStringList,
        income_parents: &QStringList,
        spending_names: &QStringList,
        spending_parents: &QStringList,
        rows: &QStringList,
    ) -> QVariant {
        let lists = [
            income_names,
            income_parents,
            spending_names,
            spending_parents,
        ]
        .map(strings);
        let rows = strings(rows);
        let result = finish_with(&currency.to_string(), &lists, &rows, |finish| {
            session::with_book_mut(finish)
        });
        if result.is_ok() {
            self.as_mut().refresh();
        }
        finish_result(result)
    }

    fn check_account(
        self: Pin<&mut Self>,
        account_type: &QString,
        name: &QString,
        currency: &QString,
        balance: &QString,
        date: &QString,
    ) -> QVariant {
        let Some(account_type) = type_from_key(&account_type.to_string()) else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        let (name, currency, balance, date) = (
            name.to_string(),
            currency.to_string(),
            balance.to_string(),
            date.to_string(),
        );
        let new = NewAccount {
            name: &name,
            account_type,
            currency: &currency,
            balance: &balance,
            date: &date,
        };
        let locale = app_locale();
        let result = session::with_book_mut(|book| {
            first_start::check_account(book, &new, &locale)
                .map_err(|error| crate::accounts::error_text(book, &error, &locale))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match result {
            Ok(()) => map([]),
            Err(error) => error_object(&error),
        }
    }
}

/// A string list as Rust strings.
fn strings(list: &QStringList) -> Vec<String> {
    QList::<QString>::from(list)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// The default categories from QML's four lists: income names and parents,
/// spending names and parents. `None` if a name has no parent beside it.
fn default_categories(lists: &[Vec<String>; 4]) -> Option<Vec<DefaultCategory<'_>>> {
    let [
        income_names,
        income_parents,
        spending_names,
        spending_parents,
    ] = lists;
    if income_names.len() != income_parents.len() || spending_names.len() != spending_parents.len()
    {
        return None;
    }
    let mut categories = Vec::new();
    for (name, parent) in income_names.iter().zip(income_parents) {
        categories.push(DefaultCategory {
            name,
            parent,
            income: true,
        });
    }
    for (name, parent) in spending_names.iter().zip(spending_parents) {
        categories.push(DefaultCategory {
            name,
            parent,
            income: false,
        });
    }
    Some(categories)
}

/// What `finish` did, as the UI gets it: `Ok`, or the error and the row.
type Finished = Result<(), (ErrorText, i32)>;

/// Writes the first setup from QML's lists into the book `with_book` hands
/// over (the open one in the app, one in memory in the tests).
fn finish_with(
    currency: &str,
    lists: &[Vec<String>; 4],
    rows: &[String],
    with_book: impl FnOnce(&mut dyn FnMut(&mut kassensturz_store::Book) -> Finished) -> Option<Finished>,
) -> Finished {
    let no_entry = || (ErrorText::plain("input.no-such-entry"), -1);
    let categories = default_categories(lists).ok_or_else(no_entry)?;
    let (whole, rest) = rows.as_chunks::<5>();
    if !rest.is_empty() {
        return Err(no_entry());
    }
    let mut accounts = Vec::with_capacity(whole.len());
    for (index, [key, name, currency, balance, date]) in whole.iter().enumerate() {
        let account_type = type_from_key(key)
            .ok_or_else(|| (ErrorText::plain("input.no-such-entry"), index as i32))?;
        accounts.push(NewAccount {
            name,
            account_type,
            currency,
            balance,
            date,
        });
    }
    let locale = app_locale();
    let mut write = |book: &mut kassensturz_store::Book| -> Finished {
        first_start::finish(book, currency, &categories, &accounts, &locale).map_err(|error| {
            let row = error.row.map_or(-1, |row| row as i32);
            (first_start::finish_error_text(book, &error, &locale), row)
        })
    };
    with_book(&mut write).unwrap_or_else(|| Err(no_entry()))
}

/// `result` as the map QML reads: empty, or the error with its `row`.
fn finish_result(result: Finished) -> QVariant {
    match result {
        Ok(()) => map([]),
        Err((error, row)) => map(std::iter::once(("code", text(error.code)))
            .chain(
                error
                    .params
                    .iter()
                    .map(|(name, value)| (*name, text(value))),
            )
            .chain(std::iter::once(("row", QVariant::from(&row))))),
    }
}

/// The regional settings' currency if KassenSturz knows it, otherwise the
/// euro (D-111).
fn regional_currency() -> CurrencyCode {
    QtLocale::currency()
        .or_else(|| CurrencyCode::new("EUR").ok())
        .expect("EUR is a valid code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::read_back::{keys, number_of, text_of};
    use kassensturz_store::Book;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn categories() -> [Vec<String>; 4] {
        [
            strings(&["Salary"]),
            strings(&[""]),
            strings(&["Food", "Groceries"]),
            strings(&["", "Food"]),
        ]
    }

    fn rows(balances: &[&str]) -> Vec<String> {
        let mut rows = strings(&["checking", "Girokonto", "EUR", balances[0], ""]);
        if let Some(second) = balances.get(1) {
            rows.extend(strings(&["credit-card", "Kreditkarte", "EUR", second, ""]));
        } else {
            // A row cut short.
            rows.extend(strings(&["credit-card", "Kreditkarte"]));
        }
        rows
    }

    fn run(book: &mut Book, balances: &[&str]) -> QVariant {
        finish_result(finish_with(
            "EUR",
            &categories(),
            &rows(balances),
            |write| Some(write(book)),
        ))
    }

    #[test]
    fn finish_reads_back_as_an_empty_map_or_an_error_with_its_row() {
        let mut book = Book::create_in_memory().expect("book in memory");
        // A wrong second row: QML reads the code and the row, and nothing is
        // written.
        let refused = run(&mut book, &["100", "nonsense"]);
        assert!(text_of(&refused, "code").starts_with("input."));
        assert_eq!(number_of(&refused, "row"), 1);
        assert!(book.history().expect("history").is_empty());

        let done = run(&mut book, &["100", "20"]);
        assert!(keys(&done).is_empty());
        assert_eq!(book.history().expect("history").len(), 1);
    }

    #[test]
    fn lists_of_different_lengths_are_refused_with_no_row() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let refused = run(&mut book, &["100"]);
        assert_eq!(text_of(&refused, "code"), "input.no-such-entry");
        assert_eq!(number_of(&refused, "row"), -1);
    }
}
