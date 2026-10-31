// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Budgets page as a QML object (M6c.5, D-357), on the book the app has
//! open (`session.rs`). The work is in `budgets.rs`.
//!
//! QML gets everything as text and flags (D-065); a bar's `permille` only
//! places the mark and is never read back. Every change asks the other
//! screens to read the book again through the `changed` signal.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_store::Book;

use crate::budgets::{self, Bar, BudgetRow, BudgetsError, BudgetsView, NewBudget, PeriodChoice};
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
        /// The budgets of the book the app has open.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type BudgetBook = super::BudgetBookRust;
    }

    extern "RustQt" {
        /// Reads the open book again, e.g. after another screen changed it.
        #[qinvokable]
        fn refresh(self: Pin<&mut BudgetBook>);

        /// The periods a new budget of this kind (`month`, `quarter` or
        /// `year`) can begin with, newest first. Empty for a range.
        #[qinvokable]
        #[cxx_name = "startChoices"]
        fn start_choices(self: &BudgetBook, every: &QString) -> QVariant;

        /// Makes a budget. `first_day` is a start from `startChoices` as
        /// `day`, empty for the current period, or for a range the typed
        /// first day; `last_day` is the typed last day of a range. Returns
        /// an empty map, or the error's `code` and its parameters.
        #[qinvokable]
        #[cxx_name = "addBudget"]
        fn add_budget(
            self: Pin<&mut BudgetBook>,
            category: &QString,
            every: &QString,
            amount: &QString,
            carry_over: bool,
            first_day: &QString,
            last_day: &QString,
        ) -> QVariant;

        /// Plans `amount` for the budget from the period that starts on
        /// `from_day` (one of its `changes`) on. Returns an empty map, or the
        /// error.
        #[qinvokable]
        #[cxx_name = "changeBudget"]
        fn change_budget(
            self: Pin<&mut BudgetBook>,
            id: &QString,
            from_day: &QString,
            amount: &QString,
            carry_over: bool,
        ) -> QVariant;

        /// Ends the budget after the current period. Returns an empty map,
        /// or the error.
        #[qinvokable]
        #[cxx_name = "stopBudget"]
        fn stop_budget(self: Pin<&mut BudgetBook>, id: &QString) -> QVariant;

        /// Deletes the budget with its past results. Returns an empty map,
        /// or the error.
        #[qinvokable]
        #[cxx_name = "removeBudget"]
        fn remove_budget(self: Pin<&mut BudgetBook>, id: &QString) -> QVariant;
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut BudgetBook>);
    }

    impl cxx_qt::Initialize for BudgetBook {}
}

/// The Rust side of [`qobject::BudgetBook`].
#[derive(Default)]
pub struct BudgetBookRust {
    view: QVariant,
}

impl cxx_qt::Initialize for qobject::BudgetBook {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::BudgetBook {
    fn refresh(mut self: Pin<&mut Self>) {
        let locale: UserLocale<QtLocale> = app_locale();
        let read = session::with_book(|book| budgets::view(book, &locale));
        let view = match read {
            Some(Ok(view)) => view,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the budgets: {error:?}");
                return;
            }
            None => return,
        };
        self.as_mut().rust_mut().view = view_object(&view);
        self.as_mut().view_changed();
    }

    fn start_choices(&self, every: &QString) -> QVariant {
        let locale: UserLocale<QtLocale> = app_locale();
        match budgets::start_choices(&every.to_string(), &locale) {
            Ok(choices) => list(choices.iter().map(choice_object)),
            Err(_) => list([]),
        }
    }

    fn add_budget(
        self: Pin<&mut Self>,
        category: &QString,
        every: &QString,
        amount: &QString,
        carry_over: bool,
        first_day: &QString,
        last_day: &QString,
    ) -> QVariant {
        let (category, every, amount) =
            (category.to_string(), every.to_string(), amount.to_string());
        let (first_day, last_day) = (first_day.to_string(), last_day.to_string());
        let result = change(|book, locale| {
            let new = NewBudget {
                category: &category,
                every: &every,
                amount: &amount,
                carry_over,
                first_day: &first_day,
                last_day: &last_day,
            };
            budgets::add_budget(book, &new, locale).map(drop)
        });
        self.finish(result)
    }

    fn change_budget(
        self: Pin<&mut Self>,
        id: &QString,
        from_day: &QString,
        amount: &QString,
        carry_over: bool,
    ) -> QVariant {
        let (id, from_day, amount) = (id.to_string(), from_day.to_string(), amount.to_string());
        let result = change(|book, locale| {
            budgets::change_budget(book, &id, &from_day, &amount, carry_over, locale)
        });
        self.finish(result)
    }

    fn stop_budget(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        let result = change(|book, locale| budgets::stop_budget(book, &id, locale));
        self.finish(result)
    }

    fn remove_budget(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        let result = change(|book, _| budgets::remove_budget(book, &id));
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
fn change<T>(
    f: impl FnOnce(&mut Book, &UserLocale<QtLocale>) -> Result<T, BudgetsError>,
) -> Result<T, ErrorText> {
    let locale: UserLocale<QtLocale> = app_locale();
    session::with_book_mut(|book| {
        f(book, &locale).map_err(|error| budgets::error_text(book, &error, &locale))
    })
    .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

/// A bar as QML reads it. Shared with the bars of the components
/// (`budget_bars_model.rs`).
pub fn bar_object(bar: &Bar) -> QVariant {
    map([
        ("budget", text(&bar.budget)),
        ("category", text(&bar.category)),
        ("name", text(&bar.name)),
        ("every", text(bar.every)),
        ("currency", text(&bar.currency)),
        ("firstDay", text(&bar.first_day)),
        ("lastDay", text(&bar.last_day)),
        ("countedTo", text(&bar.counted_to)),
        ("running", QVariant::from(&bar.running)),
        ("planned", text(&bar.planned)),
        ("carriedIn", text(&bar.carried_in)),
        ("available", text(&bar.available)),
        ("spent", text(&bar.spent)),
        ("left", text(&bar.left)),
        ("used", text(&bar.used)),
        ("permille", QVariant::from(&bar.permille)),
        ("over", QVariant::from(&bar.over)),
        ("hasShare", QVariant::from(&bar.has_share)),
        (
            "notes",
            list(bar.notes.iter().map(|note| {
                map([
                    ("word", text(note.word)),
                    ("amount", text(&note.amount)),
                    ("negative", QVariant::from(&note.negative)),
                ])
            })),
        ),
    ])
}

fn choice_object(choice: &PeriodChoice) -> QVariant {
    map([
        ("day", text(&choice.day)),
        ("firstDay", text(&choice.first_day)),
        ("lastDay", text(&choice.last_day)),
        ("current", QVariant::from(&choice.current)),
    ])
}

fn row_object(row: &BudgetRow) -> QVariant {
    let mut entries = vec![
        ("id", text(&row.id)),
        ("category", text(&row.category)),
        ("name", text(&row.name)),
        ("every", text(row.every)),
        ("currency", text(&row.currency)),
        ("amount", text(&row.amount)),
        ("amountInput", text(&row.amount_input)),
        ("carryOver", QVariant::from(&row.carry_over)),
        ("state", text(row.state)),
        ("firstDay", text(&row.first_day)),
        ("lastDay", text(&row.last_day)),
        ("canStop", QVariant::from(&row.can_stop)),
        ("changes", list(row.changes.iter().map(choice_object))),
    ];
    // Only a running budget has a bar.
    if let Some(bar) = &row.bar {
        entries.push(("bar", bar_object(bar)));
    }
    map(entries)
}

fn view_object(view: &BudgetsView) -> QVariant {
    map([
        ("budgets", list(view.budgets.iter().map(row_object))),
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
                    ("currency", text(&row.currency)),
                ])
            })),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{Account, AccountKind, Split, Transaction, TransactionId};
    use kassensturz_core::{AccountId, BudgetEvery, CurrencyCode, Date, Money};

    use super::*;
    use crate::categories::{NewCategory, add_category};
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, object_of, text_of};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// A book where the real store holds a monthly budget that is carried
    /// over from September, with September and October spending booked by
    /// real transactions, and a second budget that has not started.
    fn book() -> Book {
        let mut book = Book::create_in_memory().expect("book");
        book.use_currency(EUR).expect("currency used");
        let checking = AccountId::generate();
        book.add_account(&Account {
            id: checking,
            name: "Checking".into(),
            kind: AccountKind::Asset,
            currency: EUR,
            account_type: Some(kassensturz_core::AccountType::Checking),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account");
        let mut make = |name: &str| {
            let new = NewCategory {
                name,
                income: false,
                parent: "",
                currency: "",
            };
            add_category(&mut book, &new, EUR).expect("category")
        };
        let food = make("Food");
        let rent = make("Rent");
        book.add_budget(
            food,
            BudgetEvery::Month,
            Date::new(2026, 9, 1).expect("a date"),
            None,
            Money::from_minor_units(40_000, EUR),
            true,
        )
        .expect("budget");
        book.add_budget(
            rent,
            BudgetEvery::Year,
            Date::new(2027, 1, 1).expect("a date"),
            None,
            Money::from_minor_units(960_000, EUR),
            false,
        )
        .expect("budget");
        for (month, cents) in [(9, 50_000), (10, 15_000)] {
            let transaction = Transaction::new(
                TransactionId::generate(),
                Date::new(2026, month, 5).expect("a date"),
                None,
                vec![
                    Split::new(checking, Money::from_minor_units(-cents, EUR)),
                    Split::new(food, Money::from_minor_units(cents, EUR)),
                ],
            )
            .expect("balanced");
            book.add_transaction(&transaction).expect("added");
        }
        book
    }

    /// What QML reads from the page's view, built from the real output of
    /// the logic over a real book.
    #[test]
    fn the_page_view_reads_back_the_way_qml_reads_it() {
        let book = book();
        let view = view_object(&budgets::view(&book, &TestLocale).expect("view"));
        assert_eq!(keys(&view), ["budgets", "categories"]);
        let rows = list_of(&view, "budgets");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            keys(&rows[0]),
            [
                "amount",
                "amountInput",
                "bar",
                "canStop",
                "carryOver",
                "category",
                "changes",
                "currency",
                "every",
                "firstDay",
                "id",
                "lastDay",
                "name",
                "state"
            ]
        );
        assert_eq!(text_of(&rows[0], "name"), "Food");
        assert_eq!(text_of(&rows[0], "every"), "month");
        assert_eq!(text_of(&rows[0], "state"), "running");
        assert_eq!(text_of(&rows[0], "amount"), "400,00 €");
        assert_eq!(text_of(&rows[0], "amountInput"), "400,00");
        assert!(flag_of(&rows[0], "carryOver"));
        assert!(flag_of(&rows[0], "canStop"));
        let changes = list_of(&rows[0], "changes");
        assert_eq!(keys(&changes[0]), ["current", "day", "firstDay", "lastDay"]);
        assert_eq!(text_of(&changes[0], "day"), "2026-10-01");
        assert!(flag_of(&changes[0], "current"));
        // October carries 400 - 500 = -100 in from September, which is
        // overspent: 300 available, 150 spent.
        let bar = object_of(&rows[0], "bar");
        assert_eq!(text_of(&bar, "available"), "300,00 €");
        assert_eq!(text_of(&bar, "spent"), "150,00 €");
        assert_eq!(text_of(&bar, "left"), "150,00 €");
        assert_eq!(text_of(&bar, "used"), "50,0 %");
        assert_eq!(number_of(&bar, "permille"), 500);
        // The budget that has not started has no bar.
        assert_eq!(text_of(&rows[1], "state"), "upcoming");
        assert!(!keys(&rows[1]).contains(&"bar".to_owned()));
        assert!(!flag_of(&rows[1], "canStop"));
        let categories = list_of(&view, "categories");
        assert_eq!(
            keys(&categories[0]),
            ["currency", "depth", "id", "name", "path"]
        );
        assert_eq!(text_of(&categories[0], "currency"), "EUR");
    }

    #[test]
    fn a_bar_reads_back_with_its_notes_and_marks() {
        let book = book();
        let bars = budgets::bars(&book, &TestLocale, "this-month", &[]).expect("bars");
        let bar = bar_object(&bars[0]);
        assert_eq!(
            keys(&bar),
            [
                "available",
                "budget",
                "carriedIn",
                "category",
                "countedTo",
                "currency",
                "every",
                "firstDay",
                "hasShare",
                "lastDay",
                "left",
                "name",
                "notes",
                "over",
                "permille",
                "planned",
                "running",
                "spent",
                "used"
            ]
        );
        assert!(flag_of(&bar, "running"));
        assert!(flag_of(&bar, "hasShare"));
        assert!(!flag_of(&bar, "over"));
        assert_eq!(text_of(&bar, "carriedIn"), "-100,00 €");
        let notes = list_of(&bar, "notes");
        assert_eq!(notes.len(), 1);
        assert_eq!(keys(&notes[0]), ["amount", "negative", "word"]);
        assert_eq!(text_of(&notes[0], "word"), "carried-in");
        assert_eq!(text_of(&notes[0], "amount"), "100,00 €");
        assert!(flag_of(&notes[0], "negative"));
    }
}
