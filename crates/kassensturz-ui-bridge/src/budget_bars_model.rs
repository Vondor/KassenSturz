// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The budget bars of a component as a QML object (M6c.5, D-357), on the
//! book the app has open (`session.rs`). The work is in `budgets.rs`; this
//! file turns the bars into the plain values QML reads. Figures are text
//! (D-065).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Report;

use crate::budgets::{self, BudgetsError};
use crate::budgets_model::bar_object;
use crate::number_format::app_locale;
use crate::scope::{FilterView, Scope};
use crate::session;
use crate::variant::{list, map, text};

/// What the component asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// A fixed period name or a month as `YYYY-MM`.
    pub period: String,
    /// IDs of categories; empty for every budget.
    pub only: Vec<String>,
    /// The filters the dashboard puts on the component (M6c.7, D-350).
    pub scope: Scope,
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The budget bars of the open book. Ask for them with `show`;
        /// `view` then holds an object with `bars` and `count` (how many
        /// budgets the book has, so a component can tell "no budget yet"
        /// from "none in this period") or an `error`.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type BookBudgets = super::BookBudgetsRust;
    }

    extern "RustQt" {
        /// Shows the bars of `period` (a fixed period name or a month as
        /// `YYYY-MM`) for the budgets of the categories in `only` (IDs
        /// separated by commas; empty for every budget).
        #[qinvokable]
        fn show(self: Pin<&mut BookBudgets>, period: &QString, only: &QString);

        /// Sets the filters the next request is narrowed by (M6c.7, D-350):
        /// the `scope` text the dashboard's layout gives a component (empty
        /// for none). Call it before `show`.
        #[qinvokable]
        #[cxx_name = "setScope"]
        fn set_scope(self: Pin<&mut BookBudgets>, scope: &QString);

        /// Reads the open book again for the last request, e.g. after a
        /// screen changed the book.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookBudgets>);
    }
}

/// The Rust side of [`qobject::BookBudgets`].
#[derive(Default)]
pub struct BookBudgetsRust {
    view: QVariant,
    request: Option<Request>,
    scope: Scope,
}

impl qobject::BookBudgets {
    fn show(mut self: Pin<&mut Self>, period: &QString, only: &QString) {
        let scope = self.rust().scope.clone();
        self.as_mut().rust_mut().request = Some(Request {
            period: period.to_string(),
            only: only
                .to_string()
                .split(',')
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect(),
            scope,
        });
        self.refresh();
    }

    fn set_scope(mut self: Pin<&mut Self>, scope: &QString) {
        self.as_mut().rust_mut().scope = Scope::from_text(&scope.to_string());
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let Some(request) = self.rust().request.clone() else {
            return;
        };
        let locale = app_locale();
        let view = match session::with_book(|book| {
            let (bars, filter) =
                budgets::bars_filtered(book, &locale, &request.period, &request.only, &request.scope)?;
            let count = budgets::count(book)?;
            Ok::<_, BudgetsError>((bars, count, filter))
        }) {
            Some(result) => view_object(&result),
            None => QVariant::default(),
        };
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }
}

fn view_object(result: &Result<(Vec<budgets::Bar>, usize, FilterView), BudgetsError>) -> QVariant {
    match result {
        Ok((bars, count, filter)) => map([
            ("filter", filter.object()),
            ("bars", list(bars.iter().map(bar_object))),
            (
                "count",
                QVariant::from(&i32::try_from(*count).unwrap_or(i32::MAX)),
            ),
        ]),
        Err(error) => {
            let report = match error {
                BudgetsError::Report(error) => Some(error.report()),
                BudgetsError::Store(error) => Some(error.report()),
                _ => None,
            };
            if report.is_none() {
                eprintln!("kassensturz: the budget bars were asked for with a wrong name");
            }
            map([(
                "error",
                map([(
                    "code",
                    text(report.map_or("charts.bad-request", |r| r.code)),
                )]),
            )])
        }
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{BudgetEvery, CurrencyCode, Date, Money};
    use kassensturz_store::Book;

    use super::*;
    use crate::categories::{NewCategory, add_category};
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{keys, list_of, number_of, text_of};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn ask(book: &Book, period: &str, only: &[String]) -> QVariant {
        view_object(
            &budgets::bars_filtered(book, &TestLocale, period, only, &Scope::default()).and_then(
                |(bars, filter)| {
                    let count = budgets::count(book)?;
                    Ok((bars, count, filter))
                },
            ),
        )
    }

    #[test]
    fn the_bars_read_back_with_how_many_budgets_the_book_has() {
        let mut book = Book::create_in_memory().expect("book");
        book.use_currency(EUR).expect("currency used");
        let new = NewCategory {
            name: "Food",
            income: false,
            parent: "",
            currency: "",
        };
        let food = add_category(&mut book, &new, EUR).expect("category");
        // With no budget, there is nothing to draw and the count says so:
        // the component takes no room on the Overview (D-357).
        let none = ask(&book, "this-month", &[]);
        assert_eq!(keys(&none), ["bars", "count", "filter"]);
        assert!(list_of(&none, "bars").is_empty());
        assert_eq!(number_of(&none, "count"), 0);
        book.add_budget(
            food,
            BudgetEvery::Month,
            Date::new(2026, 10, 1).expect("a date"),
            None,
            Money::from_minor_units(25_000, EUR),
            false,
        )
        .expect("budget");
        let one = ask(&book, "this-month", &[]);
        assert_eq!(number_of(&one, "count"), 1);
        let bars = list_of(&one, "bars");
        assert_eq!(bars.len(), 1);
        assert_eq!(text_of(&bars[0], "name"), "Food");
        assert_eq!(text_of(&bars[0], "planned"), "250,00 €");
        // A period before the budget started has no bar, but the book has a
        // budget, which the component tells from "no budget yet".
        let earlier = ask(&book, "2026-08", &[]);
        assert!(list_of(&earlier, "bars").is_empty());
        assert_eq!(number_of(&earlier, "count"), 1);
    }

    #[test]
    fn a_name_the_screen_does_not_know_is_an_error_object() {
        let book = Book::create_in_memory().expect("book");
        let wrong = ask(&book, "next-week", &[]);
        assert_eq!(keys(&wrong), ["error"]);
    }
}
