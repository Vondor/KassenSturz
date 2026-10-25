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
use kassensturz_store::StoreError;

use crate::accounts_model::sections_object;
use crate::fetch_gap::{self, FetchGap};
use crate::format::Locale;
use crate::number_format::app_locale;
use crate::overview::{self, LatestRow, Month, OverviewView};
use crate::session;
use crate::settings::Settings;
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
        let limit = fetch_gap::limit_of(&Settings::user().fetch_gap_choice());
        let read = session::with_book(|book| -> Result<_, StoreError> {
            let view = overview::view(book, month, &locale)?;
            let gaps = fetch_gap::gaps(book, locale.today(), limit, &locale)?;
            Ok((view, gaps))
        });
        let view = match read {
            Some(Ok((view, gaps))) => view_object(&view, &gaps),
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

/// A transaction as seen from one of the user's accounts, as QML gets it.
pub fn latest_row_object(row: &LatestRow) -> QVariant {
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
}

/// A linked account that wasn't fetched for longer than the limit, as QML
/// gets it.
fn fetch_gap_object(gap: &FetchGap) -> QVariant {
    map([
        ("accountId", text(&gap.account_id.to_string())),
        ("account", text(&gap.account)),
        ("lastFetched", text(&gap.last_fetched)),
        (
            "days",
            QVariant::from(&i32::try_from(gap.days).unwrap_or(i32::MAX)),
        ),
    ])
}

fn view_object(view: &OverviewView, gaps: &[FetchGap]) -> QVariant {
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
    let latest = view.latest.iter().map(latest_row_object);
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
        ("fetchGaps", list(gaps.iter().map(fetch_gap_object))),
    ])
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{AccountType, BankLink, Date};
    use kassensturz_store::Book;

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::fetch_gap;
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{keys, list_of, text_of};

    /// A fetch gap read back the way QML reads it, from a book where the
    /// store's own operations made the link and noted the fetch.
    #[test]
    fn the_fetch_gaps_read_back_as_qml_reads_them() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let id = add_account(
            &mut book,
            &NewAccount {
                name: "Giro",
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "",
                date: "2026-01-01",
            },
            &TestLocale,
        )
        .expect("account added");
        book.link_bank_account(&BankLink {
            account: id,
            iban: Some("DE89370400440532013000".to_owned()),
            bank_code: None,
            number: None,
            last_fetched_on: None,
        })
        .expect("linked");
        let last = Date::new(2026, 8, 1).expect("a date");
        book.note_bank_fetch(id, last).expect("fetch noted");
        let today = Date::new(2026, 10, 25).expect("a date");

        let month = overview::Month::of(today);
        let view = overview::view(&book, month, &TestLocale).expect("overview");
        let gaps = fetch_gap::gaps(&book, today, Some(60), &TestLocale).expect("gaps");
        let object = view_object(&view, &gaps);

        let shown = list_of(&object, "fetchGaps");
        assert_eq!(shown.len(), 1);
        assert_eq!(
            keys(&shown[0]),
            ["account", "accountId", "days", "lastFetched"]
        );
        assert_eq!(text_of(&shown[0], "account"), "Giro");
        assert_eq!(text_of(&shown[0], "accountId"), id.to_string());
        assert_eq!(text_of(&shown[0], "days"), "85");
        assert_eq!(
            text_of(&shown[0], "lastFetched"),
            TestLocale.short_date(last)
        );

        // With the warning off, the list is there and empty.
        let off = view_object(&view, &[]);
        assert!(list_of(&off, "fetchGaps").is_empty());
    }
}
