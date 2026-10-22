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

use crate::errors::ErrorText;
use crate::first_start::{self, DefaultCategory};
use crate::locale::QtLocale;
use crate::number_format::app_locale;
use crate::session;
use crate::variant::{error_object, map};

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
        let names = |list: &QStringList| -> Vec<String> {
            QList::<QString>::from(list)
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        let (income_names, income_parents) = (names(income_names), names(income_parents));
        let (spending_names, spending_parents) = (names(spending_names), names(spending_parents));
        if income_names.len() != income_parents.len()
            || spending_names.len() != spending_parents.len()
        {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        }
        let mut categories: Vec<DefaultCategory<'_>> = Vec::new();
        for (name, parent) in income_names.iter().zip(&income_parents) {
            categories.push(DefaultCategory {
                name,
                parent,
                income: true,
            });
        }
        for (name, parent) in spending_names.iter().zip(&spending_parents) {
            categories.push(DefaultCategory {
                name,
                parent,
                income: false,
            });
        }
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

/// The regional settings' currency if KassenSturz knows it, otherwise the
/// euro (D-111).
fn regional_currency() -> CurrencyCode {
    QtLocale::currency()
        .or_else(|| CurrencyCode::new("EUR").ok())
        .expect("EUR is a valid code")
}
