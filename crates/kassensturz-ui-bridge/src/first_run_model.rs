// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first-run mock (M1.M) as a QML object.
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
        /// The first-run mock: a book in memory that is never saved.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, overview, READ, NOTIFY)]
        #[qproperty(QStringList, accounts, READ, NOTIFY)]
        #[qproperty(QString, today, READ, CONSTANT)]
        #[qproperty(QString, amount_example, cxx_name = "amountExample", READ, NOTIFY)]
        type FirstRunMock = super::FirstRunMockRust;
    }

    extern "RustQt" {
        /// Starts over with an empty book and the given categories, named in
        /// the user's language.
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

        /// The categories for money in or money out, in the order `start`
        /// got them.
        #[qinvokable]
        fn categories(self: &FirstRunMock, money_in: bool) -> QStringList;
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
    spending: QStringList,
    income: QStringList,
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
        // The locale's own currency if KassenSturz knows it, otherwise the
        // euro: the mock has one currency, as most households do.
        let state = QtLocale::currency()
            .and_then(|currency| FirstRun::new(currency, &names(spending), &names(income)).ok())
            .or_else(|| {
                let euro = CurrencyCode::new("EUR").ok()?;
                FirstRun::new(euro, &names(spending), &names(income)).ok()
            });
        if state.is_none() {
            // Only a bug can get here: the book is in memory.
            eprintln!("kassensturz: couldn't create the first-run mock's book");
        }
        let mut model = self.as_mut().rust_mut();
        model.state = state;
        model.spending = spending.clone();
        model.income = income.clone();
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
            Some(state) => state.add_account(
                &name.to_string(),
                account_type,
                &starting_balance.to_string(),
                &QtLocale,
            ),
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
            Some(state) => state.add_transaction(&entered, &QtLocale),
            None => Err(FirstRunError::NoSuchEntry),
        };
        self.finish(result)
    }

    fn transactions(&self, account: i32) -> QVariant {
        let view = self.rust().state.as_ref().and_then(|state| {
            let account = usize::try_from(account).ok()?;
            state.register(account, &QtLocale).ok().flatten()
        });
        let Some(view) = view else {
            return QVariant::default();
        };
        let rows = view.rows.iter().map(|row| {
            map([
                ("date", text(&row.date)),
                ("otherParty", text(&row.other_party)),
                ("category", text(&row.category)),
                ("amount", text(&row.amount)),
                ("incoming", QVariant::from(&row.incoming)),
                ("startingBalance", QVariant::from(&row.starting_balance)),
            ])
        });
        map([
            ("name", text(&view.account_name)),
            ("balance", text(&view.balance)),
            ("owes", QVariant::from(&view.owes)),
            ("rows", list(rows)),
        ])
    }

    fn categories(&self, money_in: bool) -> QStringList {
        if money_in {
            self.rust().income.clone()
        } else {
            self.rust().spending.clone()
        }
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
        let Some(state) = self.rust().state.as_ref() else {
            return map([("code", text("input.no-such-entry"))]);
        };
        let ErrorText { code, params } = state.error_text(error, &QtLocale);
        map(std::iter::once(("code", text(code)))
            .chain(params.iter().map(|(name, value)| (*name, text(value)))))
    }

    /// Reads the overview and the list of accounts again.
    fn refresh(mut self: Pin<&mut Self>) {
        let Some(state) = self.rust().state.as_ref() else {
            return;
        };
        let overview = match state.overview(&QtLocale) {
            Ok(overview) => overview,
            Err(error) => {
                // Only a bug can get here: the book is in memory.
                eprintln!("kassensturz: couldn't read the overview: {error:?}");
                return;
            }
        };
        let accounts: QStringList = state
            .account_names()
            .unwrap_or_default()
            .iter()
            .map(|name| QString::from(name.as_str()))
            .collect();
        let amount_example = state.amount_example(&QtLocale).unwrap_or_default();

        let cards = overview.accounts.iter().map(|card| {
            map([
                ("name", text(&card.name)),
                ("type", text(card.account_type)),
                ("balance", text(&card.balance)),
                ("owes", QVariant::from(&card.owes)),
            ])
        });
        let spending = overview
            .spending
            .iter()
            .map(|total| map([("name", text(&total.name)), ("amount", text(&total.amount))]));
        let latest = overview.latest.iter().map(|row| {
            map([
                ("account", text(&row.account)),
                ("date", text(&row.date)),
                ("otherParty", text(&row.other_party)),
                ("category", text(&row.category)),
                ("amount", text(&row.amount)),
                ("incoming", QVariant::from(&row.incoming)),
            ])
        });
        let overview = map([
            ("accounts", list(cards)),
            ("together", text(&overview.together)),
            ("togetherOwes", QVariant::from(&overview.together_owes)),
            ("moneyIn", text(&overview.money_in)),
            ("moneyOut", text(&overview.money_out)),
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
