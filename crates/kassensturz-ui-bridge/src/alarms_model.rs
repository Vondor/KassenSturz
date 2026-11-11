// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The warnings tile as a QML object (US4.9, D-721), on the book the app has
//! open (`session.rs`). The work is in `alarms.rs`; this file turns the rows
//! into the plain values QML reads. Figures are text (D-065).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Report;

use crate::alarms::{self, AlarmRow, AlarmsError, AlarmsView};
use crate::number_format::app_locale;
use crate::properties::read_alarms;
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
        /// What the rules of a warnings tile find in the open book today.
        /// Ask with `show`; `view` then holds an object with `rows` (one per
        /// rule, in the tile's order), `count` (how many rules) and `shown`
        /// (how many are a line of their own: triggered, gone or not
        /// checkable), or an `error`.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type BookAlarms = super::BookAlarmsRust;
    }

    extern "RustQt" {
        /// Shows what `rules` find: the tile's rules as the document writes
        /// them, a JSON list (D-722). A text that is no such list shows no
        /// rule.
        #[qinvokable]
        fn show(self: Pin<&mut BookAlarms>, rules: &QString);

        /// Reads the open book again for the last request, e.g. after a
        /// screen changed the book.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookAlarms>);

        /// The amount the user typed for a rule on `account` (its ID), in the
        /// plain form the tile keeps (`"500.00"`), or empty when it isn't an
        /// amount in that account's currency.
        #[qinvokable]
        #[cxx_name = "readAmount"]
        fn read_amount(self: &BookAlarms, typed: &QString, account: &QString) -> QString;
    }
}

/// The Rust side of [`qobject::BookAlarms`].
#[derive(Default)]
pub struct BookAlarmsRust {
    view: QVariant,
    rules: Option<String>,
}

impl qobject::BookAlarms {
    fn show(mut self: Pin<&mut Self>, rules: &QString) {
        self.as_mut().rust_mut().rules = Some(rules.to_string());
        self.refresh();
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let Some(rules) = self.rust().rules.clone() else {
            return;
        };
        let rules = read_alarms(&rules).unwrap_or_default();
        let locale = app_locale();
        let view = match session::with_book(|book| alarms::view(book, &locale, &rules)) {
            Some(result) => view_object(&result, rules.len()),
            None => QVariant::default(),
        };
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    fn read_amount(&self, typed: &QString, account: &QString) -> QString {
        let locale = app_locale();
        session::with_book(|book| {
            alarms::read_amount(book, &locale, &typed.to_string(), &account.to_string())
        })
        .and_then(Result::ok)
        .flatten()
        .map_or_else(QString::default, |plain| QString::from(&plain))
    }
}

fn number(value: usize) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

pub(crate) fn row_object(row: &AlarmRow) -> QVariant {
    map([
        ("index", number(row.index)),
        ("reason", text(row.reason)),
        ("state", text(row.state)),
        ("why", text(row.why)),
        ("account", text(&row.account)),
        ("accountName", text(&row.account_name)),
        ("budget", text(&row.budget)),
        ("budgetName", text(&row.budget_name)),
        ("every", text(row.every)),
        ("ended", QVariant::from(&row.ended)),
        ("balance", text(&row.balance)),
        ("balanceDay", text(&row.balance_day)),
        ("lowest", QVariant::from(&row.lowest)),
        ("limit", text(&row.limit)),
        ("amountInput", text(&row.amount_input)),
        ("planned", text(&row.planned)),
        ("available", text(&row.available)),
        ("spent", text(&row.spent)),
        ("over", text(&row.over)),
        ("nothingToSpend", QVariant::from(&row.nothing_to_spend)),
        ("share", number(usize::from(row.share))),
        ("times", number(usize::from(row.times))),
    ])
}

fn view_object(result: &Result<AlarmsView, AlarmsError>, count: usize) -> QVariant {
    match result {
        Ok(view) => map([
            ("rows", list(view.rows.iter().map(row_object))),
            ("count", number(count)),
            ("shown", number(view.shown)),
        ]),
        Err(error) => {
            let report = match error {
                AlarmsError::Report(error) => error.report(),
                AlarmsError::Store(error) => error.report(),
            };
            map([("error", map([("code", text(report.code))]))])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alarms::tests::{fixture, rules};
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, text_of};

    #[test]
    fn the_view_reads_back_the_way_qml_reads_it() {
        let f = fixture();
        let rules = rules(&f);
        let view = view_object(&alarms::view(&f.book, &TestLocale, &rules), rules.len());
        assert_eq!(keys(&view), ["count", "rows", "shown"]);
        assert_eq!(number_of(&view, "count"), 4);
        assert_eq!(number_of(&view, "shown"), 4);
        let rows = list_of(&view, "rows");
        assert_eq!(rows.len(), 4);
        assert_eq!(
            keys(&rows[0]),
            [
                "account",
                "accountName",
                "amountInput",
                "available",
                "balance",
                "balanceDay",
                "budget",
                "budgetName",
                "ended",
                "every",
                "index",
                "limit",
                "lowest",
                "nothingToSpend",
                "over",
                "planned",
                "reason",
                "share",
                "spent",
                "state",
                "times",
                "why",
            ]
        );
        assert_eq!(text_of(&rows[0], "reason"), "budget_over");
        assert_eq!(text_of(&rows[0], "state"), "triggered");
        assert_eq!(text_of(&rows[0], "budgetName"), "Household");
        assert_eq!(number_of(&rows[0], "share"), 80);
        assert_eq!(text_of(&rows[1], "balance"), "200,00 €");
        assert_eq!(text_of(&rows[1], "amountInput"), "1500,00");
        assert!(flag_of(&rows[2], "lowest"));
        assert_eq!(number_of(&rows[2], "times"), 1);
        assert_eq!(number_of(&rows[3], "index"), 3);
        assert_eq!(text_of(&rows[3], "state"), "gone");
    }

    #[test]
    fn no_rule_is_an_empty_list() {
        let f = fixture();
        let view = view_object(&alarms::view(&f.book, &TestLocale, &[]), 0);
        assert!(list_of(&view, "rows").is_empty());
        assert_eq!(number_of(&view, "count"), 0);
    }
}
