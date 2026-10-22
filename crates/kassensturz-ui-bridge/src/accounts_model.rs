// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The accounts screen as a QML object (M3.3), on the book the app has open
//! (`session.rs`).
//!
//! QML gets everything as text, flags and lists of them (D-065). Accounts are
//! named by their ID as text. Every change asks the other screens to read the
//! book again through the `changed` signal.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList, QVariant};
use kassensturz_core::CurrencyCode;
use kassensturz_store::{Book, StoreError};

use crate::accounts::{
    self, AccountCard, AccountDetails, AccountsError, AccountsView, BankCheck, CurrencySection,
    NewAccount, type_from_key,
};
use crate::errors::ErrorText;
use crate::format::Locale;
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
use crate::register::Shown;
use crate::session;
use crate::variant::{error_object, list, map, text};

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
        /// The accounts of the book the app has open.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        #[qproperty(QStringList, currencies, READ, NOTIFY)]
        #[qproperty(QString, today, READ, CONSTANT)]
        type AccountList = super::AccountListRust;
    }

    extern "RustQt" {
        /// Reads the open book again, e.g. after another screen changed it
        /// or the number format did.
        #[qinvokable]
        fn refresh(self: Pin<&mut AccountList>);

        /// Adds an account. Returns an empty map, or the error's `code` and
        /// its parameters.
        #[qinvokable]
        #[cxx_name = "addAccount"]
        fn add_account(
            self: Pin<&mut AccountList>,
            account_type: &QString,
            name: &QString,
            currency: &QString,
            balance: &QString,
            date: &QString,
        ) -> QVariant;

        /// One account in detail, or an empty map if it isn't there.
        #[qinvokable]
        fn details(self: &AccountList, id: &QString) -> QVariant;

        /// Gives an account a new name. Returns an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "renameAccount"]
        fn rename_account(self: Pin<&mut AccountList>, id: &QString, name: &QString) -> QVariant;

        /// Closes an account. Returns an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "closeAccount"]
        fn close_account(self: Pin<&mut AccountList>, id: &QString) -> QVariant;

        /// Opens a closed account again. Returns an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "reopenAccount"]
        fn reopen_account(self: Pin<&mut AccountList>, id: &QString) -> QVariant;

        /// Holds the bank's balance on a day against KassenSturz's. Returns
        /// the comparison, or the error's `code` and its parameters.
        #[qinvokable]
        #[cxx_name = "checkAgainstBank"]
        fn check_against_bank(
            self: &AccountList,
            id: &QString,
            bank: &QString,
            date: &QString,
        ) -> QVariant;

        /// An amount in `currency` written the way the user should type it.
        /// Corrects the starting balance so the account holds on `date`
        /// what the bank says, after the user confirmed the offer of
        /// `checkAgainstBank` (D-111). Returns an empty map, or the error's
        /// `code` and its parameters.
        #[qinvokable]
        #[cxx_name = "correctStartingBalance"]
        fn correct_starting_balance(
            self: Pin<&mut AccountList>,
            id: &QString,
            bank: &QString,
            date: &QString,
        ) -> QVariant;

        #[qinvokable]
        #[cxx_name = "amountExample"]
        fn amount_example(self: &AccountList, currency: &QString) -> QString;
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut AccountList>);
    }

    impl cxx_qt::Initialize for AccountList {}
}

/// The Rust side of [`qobject::AccountList`].
#[derive(Default)]
pub struct AccountListRust {
    view: QVariant,
    currencies: QStringList,
    today: QString,
}

impl cxx_qt::Initialize for qobject::AccountList {
    fn initialize(mut self: Pin<&mut Self>) {
        let today = QtLocale.short_date(QtLocale.today());
        self.as_mut().rust_mut().today = QString::from(today.as_str());
        self.refresh();
    }
}

impl qobject::AccountList {
    fn refresh(mut self: Pin<&mut Self>) {
        let locale = app_locale();
        let fallback = fallback_currency();
        let read = session::with_book(|book| {
            Ok::<_, StoreError>((
                accounts::view(book, &locale)?,
                accounts::currency_choices(book, fallback)?,
            ))
        });
        let (view, choices) = match read {
            Some(Ok(read)) => read,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the accounts: {error:?}");
                return;
            }
            None => return,
        };
        let mut model = self.as_mut().rust_mut();
        model.view = view_object(&view);
        model.currencies = choices
            .iter()
            .map(|currency| QString::from(currency.as_str()))
            .collect();
        self.as_mut().view_changed();
        self.as_mut().currencies_changed();
    }

    fn add_account(
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
        let result = change(|book, locale| accounts::add_account(book, &new, locale).map(drop));
        self.finish(result)
    }

    fn details(&self, id: &QString) -> QVariant {
        let locale = app_locale();
        let found = session::with_book(|book| accounts::details(book, &id.to_string(), &locale));
        match found {
            Some(Ok(details)) => details_object(&details),
            _ => QVariant::default(),
        }
    }

    fn rename_account(self: Pin<&mut Self>, id: &QString, name: &QString) -> QVariant {
        let (id, name) = (id.to_string(), name.to_string());
        let result = change(|book, _| accounts::rename_account(book, &id, &name));
        self.finish(result)
    }

    fn close_account(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        let result = change(|book, _| accounts::close_account(book, &id));
        self.finish(result)
    }

    fn reopen_account(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        let result = change(|book, _| accounts::reopen_account(book, &id));
        self.finish(result)
    }

    fn check_against_bank(&self, id: &QString, bank: &QString, date: &QString) -> QVariant {
        let locale = app_locale();
        let (id, bank, date) = (id.to_string(), bank.to_string(), date.to_string());
        let result = session::with_book(|book| {
            accounts::check_against_bank(book, &id, &bank, &date, &locale)
                .map_err(|error| accounts::error_text(book, &error, &locale))
        });
        match result {
            Some(Ok(check)) => check_object(&check),
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn correct_starting_balance(
        self: Pin<&mut Self>,
        id: &QString,
        bank: &QString,
        date: &QString,
    ) -> QVariant {
        let (id, bank, date) = (id.to_string(), bank.to_string(), date.to_string());
        let result = change(|book, locale| {
            accounts::correct_starting_balance(book, &id, &bank, &date, locale)
        });
        self.finish(result)
    }

    fn amount_example(&self, currency: &QString) -> QString {
        let locale = app_locale();
        let example = CurrencyCode::new(&currency.to_string())
            .ok()
            .and_then(|currency| {
                session::with_book(|book| {
                    // A currency the book hasn't used has ISO 4217's places.
                    let digits = book
                        .currency_minor_digits(currency)
                        .ok()
                        .or_else(|| currency.iso_minor_digits())?;
                    Some(crate::errors::amount_example_with(
                        digits, currency, &locale,
                    ))
                })
                .flatten()
            })
            .unwrap_or_default();
        QString::from(example.as_str())
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
    f: impl FnOnce(&mut Book, &UserLocale<QtLocale>) -> Result<T, AccountsError>,
) -> Result<T, ErrorText> {
    let locale = app_locale();
    session::with_book_mut(|book| {
        f(book, &locale).map_err(|error| accounts::error_text(book, &error, &locale))
    })
    .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

/// The currency to offer in a book without accounts: the regional settings'
/// if KassenSturz knows it, otherwise the euro.
fn fallback_currency() -> CurrencyCode {
    QtLocale::currency()
        .or_else(|| CurrencyCode::new("EUR").ok())
        .expect("EUR is a valid code")
}

fn view_object(view: &AccountsView) -> QVariant {
    map([
        ("sections", sections_object(&view.sections)),
        ("closed", list(view.closed.iter().map(card_object))),
    ])
}

/// The accounts per currency as QML gets them, here and on the overview.
pub(crate) fn sections_object(sections: &[CurrencySection]) -> QVariant {
    list(sections.iter().map(|section| {
        map([
            ("currency", text(&section.currency)),
            ("accounts", list(section.accounts.iter().map(card_object))),
            ("together", text(&section.together.text)),
            ("exactTogether", text(&section.together.exact)),
            ("togetherOwes", QVariant::from(&section.together_owes)),
        ])
    }))
}

fn card_object(card: &AccountCard) -> QVariant {
    map([
        ("id", text(&card.id)),
        ("name", text(&card.name)),
        ("type", text(card.account_type)),
        ("balance", text(&card.balance.text)),
        ("exactBalance", text(&card.balance.exact)),
        ("owes", QVariant::from(&card.owes)),
    ])
}

fn details_object(details: &AccountDetails) -> QVariant {
    map([
        ("id", text(&details.card.id)),
        ("name", text(&details.card.name)),
        ("type", text(details.card.account_type)),
        ("currency", text(&details.currency)),
        ("balance", text(&details.card.balance.text)),
        ("exactBalance", text(&details.card.balance.exact)),
        ("owes", QVariant::from(&details.card.owes)),
        ("closed", QVariant::from(&details.closed)),
        ("debt", QVariant::from(&details.debt)),
        ("canClose", QVariant::from(&details.can_close)),
        (
            "stated",
            details
                .stated
                .as_ref()
                .map_or_else(QVariant::default, |stated| {
                    map([
                        ("balance", text(&stated.balance.text)),
                        ("exactBalance", text(&stated.balance.exact)),
                        ("owes", QVariant::from(&stated.owes)),
                        ("date", text(&stated.date)),
                        ("held", text(&stated.held.text)),
                        ("exactHeld", text(&stated.held.exact)),
                        ("heldOwes", QVariant::from(&stated.held_owes)),
                        ("matches", QVariant::from(&stated.matches)),
                    ])
                }),
        ),
    ])
}

/// The comparison with the bank. It shows every amount exactly: rounding
/// would hide the small differences the check is for (D-114).
fn check_object(check: &BankCheck) -> QVariant {
    let exact = |shown: &Shown| text(&shown.exact);
    map([
        ("date", text(&check.date)),
        ("bank", exact(&check.bank)),
        ("bankOwes", QVariant::from(&check.bank_owes)),
        ("book", exact(&check.book)),
        ("bookOwes", QVariant::from(&check.book_owes)),
        ("debt", QVariant::from(&check.debt)),
        ("matches", QVariant::from(&check.matches)),
        ("difference", exact(&check.difference)),
        ("bookHigher", QVariant::from(&check.book_higher)),
        (
            "explanations",
            list(check.explanations.iter().map(error_object)),
        ),
        (
            "correction",
            check
                .correction
                .as_ref()
                .map_or_else(QVariant::default, |correction| {
                    map([
                        ("wouldBe", exact(&correction.would_be)),
                        ("owes", QVariant::from(&correction.owes)),
                        ("adds", QVariant::from(&correction.adds)),
                        ("date", text(&correction.date)),
                    ])
                }),
        ),
    ])
}
