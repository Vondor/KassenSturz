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

use crate::budgets::{
    self, Bar, BudgetChange, BudgetRow, BudgetsError, BudgetsView, NewBudget, PeriodChoice,
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

        /// The periods a new budget of this kind (`month`, `salary-month-D`
        /// from day D (D-651), `quarter` or `year`) can begin with, newest
        /// first. Empty for a range.
        #[qinvokable]
        #[cxx_name = "startChoices"]
        fn start_choices(self: &BudgetBook, every: &QString) -> QVariant;

        /// Makes a budget for what `chosen` names (D-639): the categories'
        /// IDs separated by commas (at least one), then optionally a `;` and
        /// the IDs of the accounts it counts from, separated by commas
        /// (none for every account). `days` is the first day, then a line
        /// break and the last day: the first day is a start from
        /// `startChoices` as `day`, empty for the current period, or for a
        /// range the typed first day; the last day is the typed last day of
        /// a range. (One text for both keeps the call within seven
        /// arguments now that it takes the name; a typed day has no line
        /// break.) Returns
        /// an empty map, or the error's `code` and its parameters.
        /// `every` is a word of `startChoices` or `range`. `name` is the
        /// name the user typed, empty for none (D-664).
        #[qinvokable]
        #[cxx_name = "addBudget"]
        fn add_budget(
            self: Pin<&mut BudgetBook>,
            name: &QString,
            chosen: &QString,
            every: &QString,
            amount: &QString,
            carry_over: bool,
            days: &QString,
        ) -> QVariant;

        /// Gives the budget the name `name` (empty for none, D-664) and,
        /// with `changes_amount`, plans `amount` for it from the period that
        /// starts on `from_day` (one of its `changes`) on, as one change.
        /// Without `changes_amount` no period's amount is touched.
        /// `from_day` may go on after a line break with what the budget
        /// counts from then on, as `addBudget`'s `chosen` (D-689): only
        /// from `countsFrom` on. Without it what the budget counts stays.
        /// (One text for both keeps the call within seven arguments.)
        /// Returns an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "changeBudget"]
        fn change_budget(
            self: Pin<&mut BudgetBook>,
            id: &QString,
            name: &QString,
            changes_amount: bool,
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
        name: &QString,
        chosen: &QString,
        every: &QString,
        amount: &QString,
        carry_over: bool,
        days: &QString,
    ) -> QVariant {
        let (categories, accounts) = split_choice(&chosen.to_string());
        let (every, amount) = (every.to_string(), amount.to_string());
        let (first_day, last_day) = split_days(&days.to_string());
        let name = name.to_string();
        let result = change(|book, locale| {
            let new = NewBudget {
                name: &name,
                categories: &categories,
                accounts: &accounts,
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
        name: &QString,
        changes_amount: bool,
        from_day: &QString,
        amount: &QString,
        carry_over: bool,
    ) -> QVariant {
        let (id, name) = (id.to_string(), name.to_string());
        let (from_day, amount) = (from_day.to_string(), amount.to_string());
        let (from_day, chosen) = match from_day.split_once('\n') {
            Some((day, chosen)) => (day.to_owned(), Some(split_choice(chosen))),
            None => (from_day, None),
        };
        let result = change(|book, locale| {
            let change = BudgetChange {
                name: &name,
                changes_amount,
                from_day: &from_day,
                amount: &amount,
                carry_over,
                counts: chosen
                    .as_ref()
                    .map(|(categories, accounts)| (categories.as_slice(), accounts.as_slice())),
            };
            budgets::change_budget(book, &id, &change, locale)
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

/// The first and last day of `addBudget`'s `days`: the first, then a line
/// break and the last; without a line break all of it is the first day.
fn split_days(days: &str) -> (String, String) {
    let (first, last) = days.split_once('\n').unwrap_or((days, ""));
    (first.to_owned(), last.to_owned())
}

/// IDs sent by QML separated by commas, without empty ones.
fn id_list(ids: &str) -> Vec<String> {
    ids.split(',')
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The categories and accounts of `addBudget`'s `chosen`: categories, then
/// optionally `;` and accounts, each separated by commas.
fn split_choice(chosen: &str) -> (Vec<String>, Vec<String>) {
    let (categories, accounts) = chosen.split_once(';').unwrap_or((chosen, ""));
    (id_list(categories), id_list(accounts))
}

/// A bar as QML reads it. Shared with the bars of the components
/// (`budget_bars_model.rs`).
pub fn bar_object(bar: &Bar) -> QVariant {
    map([
        ("budget", text(&bar.budget)),
        ("categories", list(bar.categories.iter().map(|id| text(id)))),
        ("name", text(&bar.name)),
        ("ownName", text(&bar.own_name)),
        ("categoryNames", text(&bar.category_names)),
        ("accounts", text(&bar.accounts)),
        ("every", text(bar.every)),
        ("salaryDay", QVariant::from(&bar.salary_day)),
        ("currency", text(&bar.currency)),
        ("firstDay", text(&bar.first_day)),
        ("lastDay", text(&bar.last_day)),
        ("countedTo", text(&bar.counted_to)),
        ("running", QVariant::from(&bar.running)),
        ("startsLater", QVariant::from(&bar.starts_later)),
        ("ended", QVariant::from(&bar.ended)),
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
        ("categories", list(row.categories.iter().map(|id| text(id)))),
        ("name", text(&row.name)),
        ("ownName", text(&row.own_name)),
        ("categoryNames", text(&row.category_names)),
        ("accounts", text(&row.accounts)),
        (
            "accountIds",
            list(row.account_ids.iter().map(|id| text(id))),
        ),
        ("countsFrom", text(&row.counts_from)),
        (
            "history",
            list(row.history.iter().map(|line| {
                map([
                    ("earlier", QVariant::from(&line.earlier)),
                    ("day", text(&line.day)),
                    ("categoryNames", text(&line.category_names)),
                    ("accounts", text(&line.accounts)),
                ])
            })),
        ),
        ("every", text(row.every)),
        ("salaryDay", QVariant::from(&row.salary_day)),
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
            "accounts",
            list(
                view.accounts
                    .iter()
                    .map(|(id, name)| map([("id", text(id)), ("name", text(name))])),
            ),
        ),
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
    /// real transactions, and a second budget that has not started,
    /// repeats every salary month from day 25 (D-651) and has the name the
    /// user gave it (D-664).
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
            &[food],
            &[],
            BudgetEvery::Month,
            Date::new(2026, 9, 1).expect("a date"),
            None,
            Money::from_minor_units(40_000, EUR),
            true,
        )
        .expect("budget");
        book.add_named_budget(
            Some("Flat"),
            &[rent],
            &[],
            BudgetEvery::SalaryMonth { day: 25 },
            Date::new(2027, 1, 25).expect("a date"),
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

    #[test]
    fn the_sheet_sends_categories_then_accounts() {
        let both = split_choice("c1,c2;a1");
        assert_eq!(both, (vec!["c1".into(), "c2".into()], vec!["a1".into()]));
        assert_eq!(split_choice("c1"), (vec!["c1".into()], Vec::new()));
        assert_eq!(split_choice("c1;"), (vec!["c1".into()], Vec::new()));
        assert_eq!(split_choice(""), (Vec::new(), Vec::new()));
    }

    #[test]
    fn the_sheet_sends_the_first_day_then_the_last() {
        assert_eq!(
            split_days("1.11.2026\n30.11.2026"),
            ("1.11.2026".into(), "30.11.2026".into())
        );
        assert_eq!(
            split_days("2026-11-01\n"),
            ("2026-11-01".into(), String::new())
        );
        assert_eq!(split_days(""), (String::new(), String::new()));
    }

    /// What QML reads from the page's view, built from the real output of
    /// the logic over a real book.
    #[test]
    fn the_page_view_reads_back_the_way_qml_reads_it() {
        let book = book();
        let view = view_object(&budgets::view(&book, &TestLocale).expect("view"));
        assert_eq!(keys(&view), ["accounts", "budgets", "categories"]);
        let accounts = list_of(&view, "accounts");
        assert_eq!(keys(&accounts[0]), ["id", "name"]);
        assert_eq!(text_of(&accounts[0], "name"), "Checking");
        let rows = list_of(&view, "budgets");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            keys(&rows[0]),
            [
                "accountIds",
                "accounts",
                "amount",
                "amountInput",
                "bar",
                "canStop",
                "carryOver",
                "categories",
                "categoryNames",
                "changes",
                "countsFrom",
                "currency",
                "every",
                "firstDay",
                "history",
                "id",
                "lastDay",
                "name",
                "ownName",
                "salaryDay",
                "state"
            ]
        );
        assert_eq!(text_of(&rows[0], "name"), "Food");
        // Without a name of its own it is called by its categories; with one
        // by that name, and its categories stay in their field (D-664).
        assert_eq!(text_of(&rows[0], "ownName"), "");
        assert_eq!(text_of(&rows[0], "categoryNames"), "Food");
        assert_eq!(text_of(&rows[1], "name"), "Flat");
        assert_eq!(text_of(&rows[1], "ownName"), "Flat");
        assert_eq!(text_of(&rows[1], "categoryNames"), "Rent");
        assert_eq!(list_of(&rows[0], "categories").len(), 1);
        // It counts every account, so no account is named.
        assert_eq!(text_of(&rows[0], "accounts"), "");
        assert!(list_of(&rows[0], "accountIds").is_empty());
        // What it counts may change from October on; it never changed.
        assert_eq!(text_of(&rows[0], "countsFrom"), "2026-10-01");
        assert!(list_of(&rows[0], "history").is_empty());
        assert_eq!(text_of(&rows[0], "every"), "month");
        assert_eq!(number_of(&rows[0], "salaryDay"), 0);
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
        // The budget that has not started has an empty bar of its first
        // period, all of it left (D-710).
        assert_eq!(text_of(&rows[1], "state"), "upcoming");
        let later = object_of(&rows[1], "bar");
        assert!(flag_of(&later, "startsLater"));
        assert_eq!(text_of(&later, "spent"), "0,00 €");
        assert_eq!(text_of(&later, "left"), "9600,00 €");
        assert!(!flag_of(&rows[1], "canStop"));
        assert_eq!(text_of(&rows[1], "every"), "salary-month");
        assert_eq!(number_of(&rows[1], "salaryDay"), 25);
        let categories = list_of(&view, "categories");
        assert_eq!(
            keys(&categories[0]),
            ["currency", "depth", "id", "name", "path"]
        );
        assert_eq!(text_of(&categories[0], "currency"), "EUR");
    }

    /// A budget whose set changed from October on (D-689), changed through
    /// the page's own function, reads back its accounts and what it
    /// counted before, the way QML reads them.
    #[test]
    fn a_changed_set_reads_back_with_its_history() {
        let mut book = book();
        let budget = book.budgets().expect("budgets")[0].clone();
        let checking = book
            .accounts(false)
            .expect("accounts")
            .into_iter()
            .find(|account| account.name == "Checking")
            .expect("the checking account")
            .id
            .to_string();
        let food = budget.counts[0].categories[0].to_string();
        let (categories, accounts) = ([food], [checking.clone()]);
        let change = BudgetChange {
            name: "",
            changes_amount: false,
            from_day: "2026-10-01",
            amount: "",
            carry_over: false,
            counts: Some((&categories, &accounts)),
        };
        budgets::change_budget(&mut book, &budget.id.to_string(), &change, &TestLocale)
            .expect("changed");
        let view = view_object(&budgets::view(&book, &TestLocale).expect("view"));
        let row = &list_of(&view, "budgets")[0];
        let ids = list_of(row, "accountIds");
        assert_eq!(ids.len(), 1);
        assert_eq!(
            ids[0].value::<QString>().map(|id| id.to_string()),
            Some(checking)
        );
        assert_eq!(text_of(row, "accounts"), "Checking");
        let history = list_of(row, "history");
        assert_eq!(history.len(), 1);
        assert_eq!(
            keys(&history[0]),
            ["accounts", "categoryNames", "day", "earlier"]
        );
        assert!(flag_of(&history[0], "earlier"));
        assert_eq!(text_of(&history[0], "day"), "2026-09-30");
        assert_eq!(text_of(&history[0], "categoryNames"), "Food");
        assert_eq!(text_of(&history[0], "accounts"), "");
    }

    #[test]
    fn a_bar_reads_back_with_its_notes_and_marks() {
        let book = book();
        let bars = budgets::bars(&book, &TestLocale, "this-month", &[]).expect("bars");
        let bar = bar_object(&bars[0]);
        assert_eq!(
            keys(&bar),
            [
                "accounts",
                "available",
                "budget",
                "carriedIn",
                "categories",
                "categoryNames",
                "countedTo",
                "currency",
                "ended",
                "every",
                "firstDay",
                "hasShare",
                "lastDay",
                "left",
                "name",
                "notes",
                "over",
                "ownName",
                "permille",
                "planned",
                "running",
                "salaryDay",
                "spent",
                "startsLater",
                "used"
            ]
        );
        assert_eq!(text_of(&bar, "name"), "Food");
        assert!(!flag_of(&bar, "startsLater"));
        assert!(!flag_of(&bar, "ended"));
        assert_eq!(text_of(&bar, "ownName"), "");
        assert_eq!(text_of(&bar, "categoryNames"), "Food");
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
        // The salary-month budget starts later: an empty bar of its first
        // period, all of it left (D-710).
        let later = bar_object(&bars[1]);
        assert_eq!(text_of(&later, "name"), "Flat");
        assert!(flag_of(&later, "startsLater"));
        assert!(!flag_of(&later, "running"));
        assert_eq!(text_of(&later, "firstDay"), "2027-01-25");
        assert_eq!(text_of(&later, "spent"), "0,00 €");
        assert_eq!(text_of(&later, "left"), "9600,00 €");
    }
}
