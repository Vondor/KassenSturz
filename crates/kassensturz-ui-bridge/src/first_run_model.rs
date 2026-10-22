// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first-run mock (M1.M) as a QML object, on the book the app has open
//! (`session.rs`).
//!
//! QML gets everything as text, flags and lists of them (D-065). The mock's
//! lists hold a handful of rows, so they go to QML as plain lists of maps,
//! which QML views take as models; the real register in M3 uses list models.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QStringList, QVariant};
use kassensturz_core::CurrencyCode;

use crate::first_run::{ErrorText, FirstRun, FirstRunError, NewTransaction, type_from_key};
use crate::format::Locale;
use crate::locale::QtLocale;
use crate::number_format::app_locale;
use crate::session;

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
        /// The first-run mock, on the book the app has open.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, overview, READ, NOTIFY)]
        #[qproperty(QStringList, accounts, READ, NOTIFY)]
        #[qproperty(QString, today, READ, CONSTANT)]
        #[qproperty(QString, amount_example, cxx_name = "amountExample", READ, NOTIFY)]
        type FirstRunMock = super::FirstRunMockRust;
    }

    extern "RustQt" {
        /// Reads the open book again. A book without categories gets the
        /// given ones, named in the user's language.
        #[qinvokable]
        fn start(self: Pin<&mut FirstRunMock>, spending: &QStringList, income: &QStringList);

        /// Adds an account. Returns an empty map, or the error's `code` and
        /// its parameters.
        #[qinvokable]
        #[cxx_name = "addAccount"]
        fn add_account(
            self: Pin<&mut FirstRunMock>,
            account_type: &QString,
            name: &QString,
            starting_balance: &QString,
        ) -> QVariant;

        /// Adds a transaction to the account at `account` in `accounts`.
        /// Returns an empty map, or the error's `code` and its parameters.
        #[qinvokable]
        #[cxx_name = "addTransaction"]
        fn add_transaction(
            self: Pin<&mut FirstRunMock>,
            account: i32,
            money_in: bool,
            amount: &QString,
            other_party: &QString,
            category: i32,
            date: &QString,
        ) -> QVariant;

        /// The register of the account at `account` in `accounts`: its
        /// name, balance and transactions.
        #[qinvokable]
        fn transactions(self: &FirstRunMock, account: i32) -> QVariant;

        /// The categories for money in or money out, in the order the UI
        /// offers them.
        #[qinvokable]
        fn categories(self: &FirstRunMock, money_in: bool) -> QStringList;

        /// Reads the overview and the list of accounts again, e.g. after
        /// the number format changed.
        #[qinvokable]
        fn refresh(self: Pin<&mut FirstRunMock>);
    }

    impl cxx_qt::Initialize for FirstRunMock {}
}

/// The Rust side of [`qobject::FirstRunMock`].
#[derive(Default)]
pub struct FirstRunMockRust {
    overview: QVariant,
    accounts: QStringList,
    today: QString,
    amount_example: QString,
    state: Option<FirstRun>,
}

impl cxx_qt::Initialize for qobject::FirstRunMock {
    fn initialize(mut self: Pin<&mut Self>) {
        let today = QtLocale.short_date(QtLocale.today());
        self.as_mut().rust_mut().today = QString::from(today.as_str());
    }
}

impl qobject::FirstRunMock {
    fn start(mut self: Pin<&mut Self>, spending: &QStringList, income: &QStringList) {
        let names = |list: &QStringList| -> Vec<String> {
            QList::<QString>::from(list)
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        let (spending, income) = (names(spending), names(income));
        // In a book without accounts, the locale's own currency if
        // KassenSturz knows it, otherwise the euro: the mock has one
        // currency, as most households do.
        let loaded = QtLocale::currency()
            .or_else(|| CurrencyCode::new("EUR").ok())
            .and_then(|currency| {
                session::with_book_mut(|book| FirstRun::load(book, currency, &spending, &income))
            });
        let state = match loaded {
            Some(Ok(state)) => Some(state),
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the open book: {error:?}");
                None
            }
            None => None,
        };
        self.as_mut().rust_mut().state = state;
        self.refresh();
    }

    fn add_account(
        mut self: Pin<&mut Self>,
        account_type: &QString,
        name: &QString,
        starting_balance: &QString,
    ) -> QVariant {
        let Some(account_type) = type_from_key(&account_type.to_string()) else {
            return self.error(&FirstRunError::NoSuchEntry);
        };
        let result = match self.as_mut().rust_mut().state.as_mut() {
            Some(state) => session::with_book_mut(|book| {
                state.add_account(
                    book,
                    &name.to_string(),
                    account_type,
                    &starting_balance.to_string(),
                    &app_locale(),
                )
            })
            .unwrap_or(Err(FirstRunError::NoSuchEntry)),
            None => Err(FirstRunError::NoSuchEntry),
        };
        self.finish(result)
    }

    fn add_transaction(
        mut self: Pin<&mut Self>,
        account: i32,
        money_in: bool,
        amount: &QString,
        other_party: &QString,
        category: i32,
        date: &QString,
    ) -> QVariant {
        let (Ok(account), Ok(category)) = (usize::try_from(account), usize::try_from(category))
        else {
            return self.error(&FirstRunError::NoSuchEntry);
        };
        let (amount, other_party, date) = (
            amount.to_string(),
            other_party.to_string(),
            date.to_string(),
        );
        let entered = NewTransaction {
            account,
            money_in,
            amount: &amount,
            other_party: &other_party,
            category,
            date: &date,
        };
        let result = match self.as_mut().rust_mut().state.as_mut() {
            Some(state) => {
                session::with_book_mut(|book| state.add_transaction(book, &entered, &app_locale()))
                    .unwrap_or(Err(FirstRunError::NoSuchEntry))
            }
            None => Err(FirstRunError::NoSuchEntry),
        };
        self.finish(result)
    }

    fn transactions(&self, account: i32) -> QVariant {
        let view = self.rust().state.as_ref().and_then(|state| {
            let account = usize::try_from(account).ok()?;
            session::with_book(|book| state.register(book, account, &app_locale()))?
                .ok()
                .flatten()
        });
        let Some(view) = view else {
            return QVariant::default();
        };
        let rows = view.rows.iter().map(|row| {
            map([
                ("date", text(&row.date)),
                ("otherParty", text(&row.other_party)),
                ("category", text(&row.category)),
                ("amount", text(&row.amount.text)),
                ("exactAmount", text(&row.amount.exact)),
                ("incoming", QVariant::from(&row.incoming)),
                ("startingBalance", QVariant::from(&row.starting_balance)),
            ])
        });
        map([
            ("name", text(&view.account_name)),
            ("balance", text(&view.balance.text)),
            ("exactBalance", text(&view.balance.exact)),
            ("owes", QVariant::from(&view.owes)),
            ("rows", list(rows)),
        ])
    }

    fn categories(&self, money_in: bool) -> QStringList {
        self.rust()
            .state
            .as_ref()
            .map(|state| {
                state
                    .category_names(money_in)
                    .iter()
                    .map(|name| QString::from(name.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Shows the new state after a change, or returns why it failed.
    fn finish(mut self: Pin<&mut Self>, result: Result<(), FirstRunError>) -> QVariant {
        match result {
            Ok(()) => {
                self.as_mut().refresh();
                map([])
            }
            Err(error) => self.error(&error),
        }
    }

    fn error(&self, error: &FirstRunError) -> QVariant {
        let shown = self.rust().state.as_ref().and_then(|state| {
            session::with_book(|book| state.error_text(book, error, &app_locale()))
        });
        let Some(ErrorText { code, params }) = shown else {
            return map([("code", text("input.no-such-entry"))]);
        };
        map(std::iter::once(("code", text(code)))
            .chain(params.iter().map(|(name, value)| (*name, text(value)))))
    }

    /// Reads the overview and the list of accounts again.
    fn refresh(mut self: Pin<&mut Self>) {
        let Some(state) = self.rust().state.as_ref() else {
            return;
        };
        let read = session::with_book(|book| {
            Ok::<_, kassensturz_store::StoreError>((
                state.overview(book, &app_locale())?,
                state.account_names(book)?,
                state.amount_example(book, &app_locale())?,
            ))
        });
        let (overview, names, amount_example) = match read {
            Some(Ok(read)) => read,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the overview: {error:?}");
                return;
            }
            None => return,
        };
        let accounts: QStringList = names
            .iter()
            .map(|name| QString::from(name.as_str()))
            .collect();

        let cards = overview.accounts.iter().map(|card| {
            map([
                ("name", text(&card.name)),
                ("type", text(card.account_type)),
                ("balance", text(&card.balance.text)),
                ("exactBalance", text(&card.balance.exact)),
                ("owes", QVariant::from(&card.owes)),
            ])
        });
        let spending = overview.spending.iter().map(|total| {
            map([
                ("name", text(&total.name)),
                ("amount", text(&total.amount.text)),
                ("exactAmount", text(&total.amount.exact)),
            ])
        });
        let latest = overview.latest.iter().map(|row| {
            map([
                ("account", text(&row.account)),
                ("date", text(&row.date)),
                ("otherParty", text(&row.other_party)),
                ("category", text(&row.category)),
                ("amount", text(&row.amount.text)),
                ("exactAmount", text(&row.amount.exact)),
                ("incoming", QVariant::from(&row.incoming)),
            ])
        });
        let overview = map([
            ("accounts", list(cards)),
            ("together", text(&overview.together.text)),
            ("exactTogether", text(&overview.together.exact)),
            ("togetherOwes", QVariant::from(&overview.together_owes)),
            ("moneyIn", text(&overview.money_in.text)),
            ("exactMoneyIn", text(&overview.money_in.exact)),
            ("moneyOut", text(&overview.money_out.text)),
            ("exactMoneyOut", text(&overview.money_out.exact)),
            ("spending", list(spending)),
            ("latest", list(latest)),
        ]);

        let mut model = self.as_mut().rust_mut();
        model.overview = overview;
        model.accounts = accounts;
        model.amount_example = QString::from(amount_example.as_str());
        self.as_mut().overview_changed();
        self.as_mut().accounts_changed();
        self.as_mut().amount_example_changed();
    }
}

fn text(value: &str) -> QVariant {
    QVariant::from(&QString::from(value))
}

/// A QML object with these properties.
fn map<'a>(entries: impl IntoIterator<Item = (&'a str, QVariant)>) -> QVariant {
    let mut map = QMap::<QMapPair_QString_QVariant>::default();
    for (key, value) in entries {
        map.insert(QString::from(key), value);
    }
    QVariant::from(&map)
}

/// A QML array of these values.
fn list(values: impl IntoIterator<Item = QVariant>) -> QVariant {
    let mut list = QList::<QVariant>::default();
    for value in values {
        list.append(value);
    }
    QVariant::from(&list)
}
