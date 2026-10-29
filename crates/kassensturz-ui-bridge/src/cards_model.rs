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
        /// `last-12-months`, `this-year` or `last-year`) against `compare`
        /// (`previous_period` or `same_last_year`).
        #[qinvokable]
        fn show(self: Pin<&mut BookCards>, period: &QString, compare: &QString);

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
}

impl qobject::BookCards {
    fn show(mut self: Pin<&mut Self>, period: &QString, compare: &QString) {
        self.as_mut().rust_mut().request = Some(Request {
            period: period.to_string(),
            compare: compare.to_string(),
        });
        self.refresh();
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let Some(request) = self.rust().request.clone() else {
            return;
        };
        let locale = app_locale();
        let view = match session::with_book(|book| cards::cards(book, &locale, &request)) {
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
        ("previous", text(&card.previous)),
        ("change", text(&card.change)),
        ("changePercent", text(&card.change_percent)),
        ("direction", text(card.direction.key())),
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

fn view_object(result: &Result<Vec<CardSet>, ChartsError>) -> QVariant {
    match result {
        Ok(sets) => map([
            ("empty", QVariant::from(&sets.is_empty())),
            ("sets", list(sets.iter().map(set_object))),
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

    use super::*;
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{keys, list_of, number_of, text_of};

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
            period: period.into(),
            compare: "previous_period".into(),
        }
    }

    fn view_of(book: &Book, request: &Request) -> QVariant {
        view_object(&cards::cards(book, &TestLocale, request))
    }

    #[test]
    fn the_cards_read_back_the_way_qml_reads_them() {
        let view = view_of(&book(true), &request("last-month"));
        assert_eq!(keys(&view), ["empty", "sets"]);
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
                "measure",
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
                "measure",
                "permille",
                "previous",
                "value"
            ]
        );
        assert_eq!(text_of(&cards[3], "value"), "65,9 %");
        assert_eq!(number_of(&cards[3], "permille"), 659);
        assert_eq!(text_of(&cards[3], "change"), "");
    }

    #[test]
    fn a_savings_rate_without_income_has_no_value_and_no_bar() {
        let view = view_of(&book(false), &request("last-month"));
        let sets = list_of(&view, "sets");
        let cards = list_of(&sets[0], "cards");
        assert_eq!(text_of(&cards[3], "value"), "");
        assert_eq!(text_of(&cards[3], "direction"), "none");
        assert!(!keys(&cards[3]).contains(&"permille".to_owned()));
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
}
