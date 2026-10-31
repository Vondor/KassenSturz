// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The key figure cards as a QML object (M6a.5, D-285), on the book the app
//! has open (`session.rs`). The work is in `cards.rs`; this file turns the
//! cards into the plain values QML reads. Figures are text; only `permille`
//! is a number, and it only places the mark of a progress bar.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};

use crate::cards::{self, Card, CardSet, Request};
use crate::charts::ChartsError;
use crate::charts_model::error_object;
use crate::number_format::app_locale;
use crate::scope::{FilterView, Scope};
use crate::session;
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The key figures of the open book, one set per currency. Ask for
        /// them with `show`; `view` then holds an object with `sets` (empty
        /// when there was nothing to show) or an `error`.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type BookCards = super::BookCardsRust;
    }

    extern "RustQt" {
        /// Shows the figures of `period` (`this-month`, `last-month`,
        /// `last-12-months`, `this-year`, `last-year` or a month as
        /// `YYYY-MM`) against `compare` (`previous_period` or
        /// `same_last_year`).
        #[qinvokable]
        fn show(self: Pin<&mut BookCards>, period: &QString, compare: &QString);

        /// As `show`, for the figures of `dataset`: `income_expense_by_month`
        /// or `account_balances` (what you have, owe and are worth at the end
        /// of the period; `period` may also be `today`, M6c.2).
        #[qinvokable]
        #[cxx_name = "showDataset"]
        fn show_dataset(
            self: Pin<&mut BookCards>,
            dataset: &QString,
            period: &QString,
            compare: &QString,
        );

        /// Sets the filters the next requests are narrowed by (M6c.7,
        /// D-350): the `scope` text the dashboard's layout gives a component
        /// (empty for none). Call it before `show` or `showDataset`.
        #[qinvokable]
        #[cxx_name = "setScope"]
        fn set_scope(self: Pin<&mut BookCards>, scope: &QString);

        /// Reads the open book again for the last request, e.g. after a
        /// screen changed the book or the number format changed.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookCards>);
    }
}

/// The Rust side of [`qobject::BookCards`].
#[derive(Default)]
pub struct BookCardsRust {
    view: QVariant,
    request: Option<Request>,
    scope: Scope,
}

impl qobject::BookCards {
    fn show(self: Pin<&mut Self>, period: &QString, compare: &QString) {
        self.show_dataset(&QString::from(""), period, compare);
    }

    fn show_dataset(
        mut self: Pin<&mut Self>,
        dataset: &QString,
        period: &QString,
        compare: &QString,
    ) {
        let scope = self.rust().scope.clone();
        self.as_mut().rust_mut().request = Some(Request {
            dataset: dataset.to_string(),
            period: period.to_string(),
            compare: compare.to_string(),
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
        let view = match session::with_book(|book| cards::cards_filtered(book, &locale, &request)) {
            Some(result) => view_object(&result),
            None => QVariant::default(),
        };
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }
}

fn card_object(card: &Card) -> QVariant {
    let mut entries = vec![
        ("measure", text(card.measure)),
        ("currency", text(&card.currency)),
        ("value", text(&card.value)),
        ("exactValue", text(&card.exact_value)),
        ("negative", QVariant::from(&card.negative)),
        ("previous", text(&card.previous)),
        ("change", text(&card.change)),
        ("changePercent", text(&card.change_percent)),
        ("direction", text(card.direction.key())),
        // What you are worth without its sign; empty for the other figures.
        ("magnitude", text(&card.magnitude)),
        ("exactMagnitude", text(&card.exact_magnitude)),
    ];
    // Only the savings rate has a bar, and only when it has a value.
    if let Some(permille) = card.permille {
        entries.push(("permille", QVariant::from(&permille)));
    }
    map(entries)
}

fn set_object(set: &CardSet) -> QVariant {
    map([
        ("currency", text(&set.currency)),
        ("cards", list(set.cards.iter().map(card_object))),
    ])
}

fn view_object(result: &Result<(Vec<CardSet>, FilterView), ChartsError>) -> QVariant {
    match result {
        Ok((sets, filter)) => map([
            ("empty", QVariant::from(&sets.is_empty())),
            ("sets", list(sets.iter().map(set_object))),
            ("filter", filter.object()),
        ]),
        Err(error) => error_object(error),
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{
        Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Split,
        Transaction, TransactionId,
    };
    use kassensturz_store::Book;

    use kassensturz_report::{Filter, Period, Source, account_balances};

    use super::*;
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, text_of};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: name.into(),
            kind,
            currency: EUR,
            account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
        id
    }

    /// Salary and rent in two months before `TestLocale`'s today, and
    /// groceries in the later one.
    fn book(with_income: bool) -> Book {
        let mut book = Book::create_in_memory().expect("book created");
        book.use_currency(EUR).expect("currency used");
        let checking = account(&mut book, "Checking account", AccountKind::Asset);
        let rent = account(&mut book, "Rent", AccountKind::Expense);
        let salary = account(&mut book, "Salary", AccountKind::Income);
        let add = |book: &mut Book, month: i8, other: AccountId, amount: i64| {
            let day = Date::new(2026, month, 3).expect("a date");
            let transaction = Transaction::new(
                TransactionId::generate(),
                day,
                None,
                vec![
                    Split::new(checking, Money::from_minor_units(amount, EUR)),
                    Split::new(other, Money::from_minor_units(-amount, EUR)),
                ],
            )
            .expect("balanced");
            book.add_transaction(&transaction).expect("added");
        };
        for month in [8, 9] {
            if with_income {
                add(&mut book, month, salary, 250_000);
            }
            add(&mut book, month, rent, -80_000);
        }
        add(&mut book, 9, rent, -5_320);
        book
    }

    fn request(period: &str) -> Request {
        Request {
            dataset: String::new(),
            period: period.into(),
            compare: "previous_period".into(),
            ..Request::default()
        }
    }

    fn view_of(book: &Book, request: &Request) -> QVariant {
        view_object(&cards::cards_filtered(book, &TestLocale, request))
    }

    /// What the book's readings are kept for (M6-B30) must never show old
    /// figures: the open book is read twice, changed through the session
    /// the way every screen changes it, and read again.
    #[test]
    fn cards_read_after_a_change_show_the_change_though_the_book_was_read_before() {
        let expenses = |view: &QVariant| {
            let sets = list_of(view, "sets");
            text_of(&list_of(&sets[0], "cards")[1], "value")
        };
        let open = crate::books::OpenBook {
            book: book(true),
            path: std::path::PathBuf::new(),
            on_network_drive: false,
        };
        crate::session::set(open);
        let ask = || {
            crate::session::with_book(|book| view_of(book, &request("last-month")))
                .expect("a book is open")
        };
        assert_eq!(expenses(&ask()), "853,20 €");
        assert_eq!(expenses(&ask()), "853,20 €");

        crate::session::with_book_mut(|book| {
            let accounts = book.all_accounts().expect("accounts");
            let id_of = |name: &str| {
                accounts
                    .iter()
                    .find(|account| account.name == name)
                    .expect("account")
                    .id
            };
            let transaction = Transaction::new(
                TransactionId::generate(),
                Date::new(2026, 9, 4).expect("a date"),
                None,
                vec![
                    Split::new(
                        id_of("Checking account"),
                        Money::from_minor_units(-1_000, EUR),
                    ),
                    Split::new(id_of("Rent"), Money::from_minor_units(1_000, EUR)),
                ],
            )
            .expect("balanced");
            book.add_transaction(&transaction).expect("added");
        })
        .expect("a book is open");
        assert_eq!(expenses(&ask()), "863,20 €");

        // Undoing it is a change like any other.
        crate::session::with_book_mut(|book| book.undo().expect("undone")).expect("open");
        assert_eq!(expenses(&ask()), "853,20 €");
        crate::session::close();
    }

    #[test]
    fn the_cards_read_back_the_way_qml_reads_them() {
        let view = view_of(&book(true), &request("last-month"));
        assert_eq!(keys(&view), ["empty", "filter", "sets"]);
        let sets = list_of(&view, "sets");
        assert_eq!(sets.len(), 1);
        assert_eq!(keys(&sets[0]), ["cards", "currency"]);
        assert_eq!(text_of(&sets[0], "currency"), "EUR");
        let cards = list_of(&sets[0], "cards");
        let measures: Vec<String> = cards.iter().map(|card| text_of(card, "measure")).collect();
        assert_eq!(measures, ["income", "expenses", "net", "savings-rate"]);
        assert_eq!(
            keys(&cards[0]),
            [
                "change",
                "changePercent",
                "currency",
                "direction",
                "exactMagnitude",
                "exactValue",
                "magnitude",
                "measure",
                "negative",
                "previous",
                "value"
            ]
        );
        // Amounts and the percentage are text.
        assert_eq!(text_of(&cards[0], "value"), "2500,00 €");
        assert_eq!(text_of(&cards[1], "value"), "853,20 €");
        assert_eq!(text_of(&cards[1], "previous"), "800,00 €");
        assert_eq!(text_of(&cards[1], "change"), "53,20 €");
        assert_eq!(text_of(&cards[1], "direction"), "up");
        assert_eq!(text_of(&cards[1], "exactValue"), "853,20 €");
        assert!(!flag_of(&cards[2], "negative"));
        // Income is the same in both months: no change, 0 %.
        assert_eq!(text_of(&cards[0], "direction"), "same");
        assert_eq!(text_of(&cards[0], "changePercent"), "0,0 %");
        // The savings rate has the bar and no change.
        assert_eq!(
            keys(&cards[3]),
            [
                "change",
                "changePercent",
                "currency",
                "direction",
                "exactMagnitude",
                "exactValue",
                "magnitude",
                "measure",
                "negative",
                "permille",
                "previous",
                "value"
            ]
        );
        assert_eq!(text_of(&cards[3], "value"), "65,9 %");
        assert_eq!(number_of(&cards[3], "permille"), 659);
        // August kept 68,0 % of its income, so September kept less.
        assert_eq!(text_of(&cards[3], "previous"), "68,0 %");
        assert_eq!(text_of(&cards[3], "direction"), "down");
        assert_eq!(text_of(&cards[3], "change"), "");
    }

    #[test]
    fn a_savings_rate_without_income_has_no_value_and_no_bar() {
        let view = view_of(&book(false), &request("last-month"));
        let sets = list_of(&view, "sets");
        let cards = list_of(&sets[0], "cards");
        assert_eq!(text_of(&cards[3], "value"), "");
        assert_eq!(text_of(&cards[3], "direction"), "none");
        // Only money out: what is left is below zero.
        assert!(flag_of(&cards[2], "negative"));
        assert_eq!(text_of(&cards[2], "value"), "-853,20 €");
        assert!(!keys(&cards[3]).contains(&"permille".to_owned()));
    }

    #[test]
    fn a_month_key_shows_that_month() {
        // `TestLocale`'s today is in October, so September is last month.
        let by_key = view_of(&book(true), &request("2026-09"));
        let by_name = view_of(&book(true), &request("last-month"));
        assert_eq!(
            text_of(&list_of(&by_key, "sets")[0], "currency"),
            text_of(&list_of(&by_name, "sets")[0], "currency")
        );
        let cards = |view: &QVariant| list_of(&list_of(view, "sets")[0], "cards");
        for (a, b) in cards(&by_key).iter().zip(cards(&by_name).iter()) {
            assert_eq!(text_of(a, "value"), text_of(b, "value"));
        }
        // A key that isn't a month is a bad request.
        for bad in ["2026-13", "26-09", "2026-9", "+026-09", "september"] {
            assert_eq!(
                keys(&view_of(&book(true), &request(bad))),
                ["error"],
                "{bad}"
            );
        }
    }

    #[test]
    fn a_book_without_money_in_the_period_has_no_sets() {
        let view = view_of(&book(true), &request("last-year"));
        assert!(list_of(&view, "sets").is_empty());
    }

    #[test]
    fn an_unknown_name_is_a_bad_request() {
        let mut bad = request("last-month");
        bad.compare = "yesterday".into();
        let view = view_of(&book(true), &bad);
        assert_eq!(keys(&view), ["error"]);
    }

    /// The monthly sample plus a credit card: bought with in September,
    /// paid from checking in October before `TestLocale`'s today.
    fn book_with_card() -> Book {
        let mut book = book(true);
        let card = account(&mut book, "Credit card", AccountKind::Liability);
        let named = |book: &Book, name: &str| {
            book.all_accounts()
                .expect("accounts")
                .into_iter()
                .find(|account| account.name == name)
                .expect("account")
                .id
        };
        let (checking, rent) = (named(&book, "Checking account"), named(&book, "Rent"));
        let mut add = |day: Date, other: AccountId, amount: i64| {
            let transaction = Transaction::new(
                TransactionId::generate(),
                day,
                None,
                vec![
                    Split::new(other, Money::from_minor_units(amount, EUR)),
                    Split::new(card, Money::from_minor_units(-amount, EUR)),
                ],
            )
            .expect("balanced");
            book.add_transaction(&transaction).expect("added");
        };
        // Bought for 200,00 with the card, then 150,00 paid from checking.
        add(Date::new(2026, 9, 15).expect("a date"), rent, 20_000);
        add(Date::new(2026, 10, 10).expect("a date"), checking, -15_000);
        book
    }

    fn balances(period: &str) -> Request {
        Request {
            dataset: "account_balances".into(),
            ..request(period)
        }
    }

    #[test]
    fn the_figures_of_the_balances_read_back_and_agree_with_the_dataset() {
        let book = book_with_card();
        let view = view_of(&book, &balances("last-month"));
        assert_eq!(keys(&view), ["empty", "filter", "sets"]);
        let sets = list_of(&view, "sets");
        assert_eq!(sets.len(), 1);
        let cards = list_of(&sets[0], "cards");
        let measures: Vec<String> = cards.iter().map(|card| text_of(card, "measure")).collect();
        assert_eq!(measures, ["net_worth", "assets", "liabilities"]);
        // The same keys as every other card, so QML reads them alike.
        assert_eq!(
            keys(&cards[0]),
            keys(
                &list_of(
                    &list_of(&view_of(&book, &request("last-month")), "sets")[0],
                    "cards"
                )[0]
            )
        );
        // End of September against the end of August: checking holds
        // 3346,80 (1700,00 before), the card owes 200,00 (nothing before).
        assert_eq!(text_of(&cards[0], "value"), "3146,80 €");
        assert_eq!(text_of(&cards[0], "previous"), "1700,00 €");
        assert_eq!(text_of(&cards[0], "change"), "1446,80 €");
        assert_eq!(text_of(&cards[0], "direction"), "up");
        assert_eq!(text_of(&cards[1], "value"), "3346,80 €");
        assert_eq!(text_of(&cards[2], "value"), "200,00 €");
        assert_eq!(text_of(&cards[2], "previous"), "0,00 €");
        assert_eq!(text_of(&cards[2], "direction"), "up");
        // A positive worth has the same figure without a sign.
        assert!(!flag_of(&cards[0], "negative"));
        assert_eq!(text_of(&cards[0], "magnitude"), "3146,80 €");
        assert_eq!(text_of(&cards[1], "magnitude"), "");
        // The figures are the dataset's own (tests where steps meet).
        let source = Source::read_with_balances(
            &book,
            Period::new(
                Date::new(2026, 8, 1).expect("a date"),
                Date::new(2026, 9, 30).expect("a date"),
            )
            .expect("a period"),
        )
        .expect("source");
        let period = Period::new(
            Date::new(2026, 9, 1).expect("a date"),
            Date::new(2026, 9, 30).expect("a date"),
        )
        .expect("a period");
        let data = account_balances(&source, period, &Filter::default(), None).expect("dataset");
        assert_eq!(data.results[0].worth.net_worth.minor_units(), 314_680);
    }

    #[test]
    fn today_is_the_balance_at_the_end_of_today_and_a_debt_has_a_figure_without_its_sign() {
        let view = view_of(&book_with_card(), &balances("today"));
        let cards = list_of(&list_of(&view, "sets")[0], "cards");
        // 3346,80 less the 150,00 paid on 10 October; the card owes 50,00.
        assert_eq!(text_of(&cards[1], "value"), "3196,80 €");
        assert_eq!(text_of(&cards[2], "value"), "50,00 €");
        assert_eq!(text_of(&cards[0], "value"), "3146,80 €");
        // Nothing happened yesterday: the same as before.
        assert_eq!(text_of(&cards[0], "direction"), "same");

        // Only a debt: below zero, red, and "You owe" has the figure alone.
        let mut debt = Book::create_in_memory().expect("book created");
        debt.use_currency(EUR).expect("currency used");
        let card = account(&mut debt, "Credit card", AccountKind::Liability);
        let shop = account(&mut debt, "Groceries", AccountKind::Expense);
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::new(2026, 9, 15).expect("a date"),
            None,
            vec![
                Split::new(card, Money::from_minor_units(-50_000, EUR)),
                Split::new(shop, Money::from_minor_units(50_000, EUR)),
            ],
        )
        .expect("balanced");
        debt.add_transaction(&transaction).expect("added");
        let view = view_of(&debt, &balances("today"));
        let cards = list_of(&list_of(&view, "sets")[0], "cards");
        assert_eq!(text_of(&cards[0], "value"), "-500,00 €");
        assert!(flag_of(&cards[0], "negative"));
        assert_eq!(text_of(&cards[0], "magnitude"), "500,00 €");
        assert_eq!(text_of(&cards[0], "exactMagnitude"), "500,00 €");
        assert_eq!(text_of(&cards[1], "value"), "0,00 €");
        assert_eq!(text_of(&cards[2], "value"), "500,00 €");
    }

    #[test]
    fn a_dataset_the_cards_do_not_know_is_a_bad_request() {
        let bad = Request {
            dataset: "spending_by_category".into(),
            ..request("last-month")
        };
        assert_eq!(keys(&view_of(&book(true), &bad)), ["error"]);
    }
}
