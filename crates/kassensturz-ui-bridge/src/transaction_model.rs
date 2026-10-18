// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An account's transactions as a Qt list model for QML.
//!
//! Every value QML gets is finished text or a flag, formatted in Rust
//! (D-065). For the walking skeleton the model reads the sample book itself
//! (D-068); once the app opens real books (M3), it gets its data from there.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

use crate::locale::QtLocale;
use crate::register::{RegisterView, register_view};
use crate::sample_book;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        /// Qt's base class for list models.
        #[qobject]
        type QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qhash.h");
        /// Role numbers and their names in QML.
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qmodelindex.h");
        /// A position in a model.
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// An account's transactions, newest first.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, account_name, cxx_name = "accountName", READ, CONSTANT)]
        #[qproperty(QString, balance, READ, CONSTANT)]
        type TransactionListModel = super::TransactionListModelRust;
    }

    impl cxx_qt::Initialize for TransactionListModel {}

    extern "RustQt" {
        /// The value of `role` in the row at `index`.
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TransactionListModel, index: &QModelIndex, role: i32) -> QVariant;

        /// The names QML uses for the roles.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TransactionListModel) -> QHash_i32_QByteArray;

        /// How many transactions there are.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TransactionListModel, parent: &QModelIndex) -> i32;
    }
}

/// Qt::UserRole: the first role number free for a model's own use.
const USER_ROLE: i32 = 0x0100;
const DATE_ROLE: i32 = USER_ROLE + 1;
const OTHER_PARTY_ROLE: i32 = USER_ROLE + 2;
const AMOUNT_ROLE: i32 = USER_ROLE + 3;
const INCOMING_ROLE: i32 = USER_ROLE + 4;

/// The Rust side of [`qobject::TransactionListModel`].
#[derive(Default)]
pub struct TransactionListModelRust {
    account_name: QString,
    balance: QString,
    rows: Vec<Row>,
}

struct Row {
    date: QString,
    other_party: QString,
    amount: QString,
    incoming: bool,
}

impl cxx_qt::Initialize for qobject::TransactionListModel {
    fn initialize(self: Pin<&mut Self>) {
        let view = sample_book::create()
            .and_then(|sample| register_view(&sample.book, sample.checking, &QtLocale));
        match view {
            Ok(view) => self.show(view),
            // Only a bug can get here: the sample data is fixed. The window
            // then shows an empty list.
            Err(error) => eprintln!("kassensturz: couldn't load the sample book: {error:?}"),
        }
    }
}

impl qobject::TransactionListModel {
    fn show(mut self: Pin<&mut Self>, view: RegisterView) {
        let mut model = self.as_mut().rust_mut();
        model.account_name = QString::from(view.account_name.as_str());
        model.balance = QString::from(view.balance.as_str());
        model.rows = view
            .rows
            .into_iter()
            .map(|row| Row {
                date: QString::from(row.date.as_str()),
                other_party: QString::from(row.other_party.as_str()),
                amount: QString::from(row.amount.as_str()),
                incoming: row.incoming,
            })
            .collect();
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let row = usize::try_from(index.row())
            .ok()
            .and_then(|row| self.rust().rows.get(row));
        let Some(row) = row else {
            return QVariant::default();
        };
        match role {
            DATE_ROLE => QVariant::from(&row.date),
            OTHER_PARTY_ROLE => QVariant::from(&row.other_party),
            AMOUNT_ROLE => QVariant::from(&row.amount),
            INCOMING_ROLE => QVariant::from(&row.incoming),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(DATE_ROLE, QByteArray::from("date"));
        roles.insert(OTHER_PARTY_ROLE, QByteArray::from("otherParty"));
        roles.insert(AMOUNT_ROLE, QByteArray::from("amount"));
        roles.insert(INCOMING_ROLE, QByteArray::from("incoming"));
        roles
    }

    fn row_count(&self, parent: &QModelIndex) -> i32 {
        // A list has rows at the top level only.
        if parent.is_valid() {
            return 0;
        }
        i32::try_from(self.rust().rows.len()).unwrap_or(i32::MAX)
    }
}
