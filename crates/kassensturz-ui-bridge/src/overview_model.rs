// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The overview as a QML object (M3.9), on the book the app has open
//! (`session.rs`).
//!
//! QML gets everything as text, flags and short lists of them (D-065). The
//! month shown is kept here: the current one until the user goes to an
//! earlier or later one.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QVariant;

use crate::accounts_model::sections_object;
use crate::format::Locale;
use crate::number_format::app_locale;
use crate::overview::{self, Month, OverviewView};
use crate::session;
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The overview of the book the app has open: accounts, one month's
        /// money in and out, the newest transactions.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type BookOverview = super::BookOverviewRust;
    }

    extern "RustQt" {
        /// Reads the open book again, e.g. after a screen changed it or the
        /// number format did.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookOverview>);

        /// Shows the month before the one shown.
        #[qinvokable]
        #[cxx_name = "showEarlierMonth"]
        fn show_earlier_month(self: Pin<&mut BookOverview>);

        /// Shows the month after the one shown.
        #[qinvokable]
        #[cxx_name = "showLaterMonth"]
        fn show_later_month(self: Pin<&mut BookOverview>);

        /// Shows the current month again, and follows it from now on.
        #[qinvokable]
        #[cxx_name = "showThisMonth"]
        fn show_this_month(self: Pin<&mut BookOverview>);
    }

    impl cxx_qt::Initialize for BookOverview {}
}

/// The Rust side of [`qobject::BookOverview`].
#[derive(Default)]
pub struct BookOverviewRust {
    view: QVariant,
    /// The month the user went to; `None` follows the current month.
    month: Option<Month>,
}

impl cxx_qt::Initialize for qobject::BookOverview {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::BookOverview {
    fn refresh(mut self: Pin<&mut Self>) {
        let locale = app_locale();
        let month = self
            .rust()
            .month
            .unwrap_or_else(|| Month::of(locale.today()));
        let read = session::with_book(|book| overview::view(book, month, &locale));
        let view = match read {
            Some(Ok(view)) => view_object(&view),
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the overview: {error:?}");
                return;
            }
            None => QVariant::default(),
        };
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    fn show_earlier_month(self: Pin<&mut Self>) {
        self.show_month(|month| month.earlier());
    }

    fn show_later_month(self: Pin<&mut Self>) {
        self.show_month(|month| month.later());
    }

    fn show_this_month(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().month = None;
        self.refresh();
    }

    fn show_month(mut self: Pin<&mut Self>, step: impl FnOnce(Month) -> Option<Month>) {
        let shown = self
            .rust()
            .month
            .unwrap_or_else(|| Month::of(app_locale().today()));
        if let Some(month) = step(shown) {
            self.as_mut().rust_mut().month = Some(month);
            self.refresh();
        }
    }
}

fn view_object(view: &OverviewView) -> QVariant {
    let totals = view.totals.iter().map(|totals| {
        let categories = |list_of: &[overview::CategoryTotal]| {
            list(list_of.iter().map(|total| {
                map([
                    ("name", text(&total.name)),
                    ("unsorted", QVariant::from(&total.unsorted)),
                    ("amount", text(&total.amount.text)),
                    ("exactAmount", text(&total.amount.exact)),
                ])
            }))
        };
        map([
            ("currency", text(&totals.currency)),
            ("moneyIn", text(&totals.money_in.text)),
            ("exactMoneyIn", text(&totals.money_in.exact)),
            ("moneyOut", text(&totals.money_out.text)),
            ("exactMoneyOut", text(&totals.money_out.exact)),
            ("left", text(&totals.left.text)),
            ("exactLeft", text(&totals.left.exact)),
            ("overspent", QVariant::from(&totals.overspent)),
            ("income", categories(&totals.income)),
            ("spending", categories(&totals.spending)),
        ])
    });
    let latest = view.latest.iter().map(|row| {
        map([
            ("accountId", text(&row.account_id)),
            ("account", text(&row.account)),
            ("id", text(&row.id)),
            ("date", text(&row.date)),
            ("otherParty", text(&row.other_party)),
            ("category", text(&row.category)),
            ("amount", text(&row.amount.text)),
            ("exactAmount", text(&row.amount.exact)),
            ("incoming", QVariant::from(&row.incoming)),
            ("kind", text(row.kind)),
        ])
    });
    let transfers = i32::try_from(view.transfers).unwrap_or(i32::MAX);
    map([
        ("month", text(&view.month)),
        ("hasEarlier", QVariant::from(&view.has_earlier)),
        ("hasLater", QVariant::from(&view.has_later)),
        ("thisMonth", QVariant::from(&view.this_month)),
        ("empty", QVariant::from(&view.empty)),
        ("sections", sections_object(&view.accounts)),
        ("totals", list(totals)),
        ("transfers", QVariant::from(&transfers)),
        ("latest", list(latest)),
    ])
}
