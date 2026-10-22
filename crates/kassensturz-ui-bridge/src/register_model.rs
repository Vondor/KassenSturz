// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The register as a QML list model (M3.5, D-118), on the book the app has
//! open (`session.rs`).
//!
//! Unlike the screens before it, the register can hold thousands of rows, so
//! it is a `QAbstractListModel` whose rows live in Rust, and QML's list view
//! asks for what it shows. Every row is text and flags (D-065): the sorting
//! and the balance after each entry are worked out in `register.rs`. The
//! writes behind the register's pages (`transaction_entry.rs`) are on the
//! same object, so one page can add, change and remove transactions; every
//! change asks the other screens to read the book again through `changed`.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QMap, QMapPair_QString_QVariant, QModelIndex,
    QString, QVariant,
};
use kassensturz_store::Book;

use crate::errors::ErrorText;
use crate::format::Locale;
use crate::locale::QtLocale;
use crate::number_format::app_locale;
use crate::register::{RegisterRow, Sort, SortKey, register_view};
use crate::session;
use crate::transaction_entry::{self, EntryError, NewEntry};
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        /// Qt's base class for list models.
        type QAbstractListModel;

        include!("cxx-qt-lib/qhash.h");
        /// Role numbers to role names, for `roleNames`.
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qmodelindex.h");
        /// A row in a model.
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// One account's transactions, and the pages that change them.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, account_id, cxx_name = "accountId", READ, NOTIFY)]
        #[qproperty(QString, account_name, cxx_name = "accountName", READ, NOTIFY)]
        #[qproperty(QString, balance, READ, NOTIFY)]
        #[qproperty(QString, exact_balance, cxx_name = "exactBalance", READ, NOTIFY)]
        #[qproperty(bool, owes, READ, NOTIFY)]
        #[qproperty(QString, sort_key, cxx_name = "sortKey", READ, NOTIFY)]
        #[qproperty(bool, descending, READ, NOTIFY)]
        #[qproperty(i32, count, READ, NOTIFY)]
        #[qproperty(QString, today, READ, CONSTANT)]
        type AccountRegister = super::AccountRegisterRust;
    }

    extern "RustQt" {
        /// Shows the account with this ID (as text); an empty ID shows
        /// nothing.
        #[qinvokable]
        fn show(self: Pin<&mut AccountRegister>, account_id: &QString);

        /// Reads the open book again, e.g. after another screen changed it
        /// or the number format did.
        #[qinvokable]
        fn refresh(self: Pin<&mut AccountRegister>);

        /// Sorts the rows by `key` (`date`, `amount`, `other-party` or
        /// `category`), largest or latest first with `descending`.
        #[qinvokable]
        #[cxx_name = "sortBy"]
        fn sort_by(self: Pin<&mut AccountRegister>, key: &QString, descending: bool);

        /// The open accounts a transaction can be added to, as a list of
        /// `id` and `name`.
        #[qinvokable]
        #[cxx_name = "accountChoices"]
        fn account_choices(self: &AccountRegister) -> QVariant;

        /// The categories a transaction in the account with `account_id`
        /// can be booked to, as a list of `id`, `path` and `depth`: those
        /// for money in or for money out, and `current` (an ID) even if it
        /// is of the other kind. An error's `code` if the account isn't
        /// there.
        #[qinvokable]
        #[cxx_name = "categoryChoices"]
        fn category_choices(
            self: &AccountRegister,
            account_id: &QString,
            money_in: bool,
            current: &QString,
        ) -> QVariant;

        /// The transaction with `transaction_id` as the page shows it, seen
        /// from the account with `account_id`, or an error's `code`.
        #[qinvokable]
        fn details(
            self: &AccountRegister,
            account_id: &QString,
            transaction_id: &QString,
        ) -> QVariant;

        /// Adds a transaction to the account with `account_id`, from the
        /// page's form: an object with `moneyIn`, `amount`, `otherParty`,
        /// `category` (an ID, empty for none yet) and `date`, all as typed.
        /// Returns an empty map, or the error's `code` and its parameters.
        #[qinvokable]
        #[cxx_name = "addTransaction"]
        fn add_transaction(
            self: Pin<&mut AccountRegister>,
            account_id: &QString,
            form: &QVariant,
        ) -> QVariant;

        /// Changes a simple transaction from the page's form, as for
        /// `addTransaction`. Returns an empty map, or the error's `code` and
        /// its parameters.
        #[qinvokable]
        #[cxx_name = "editTransaction"]
        fn edit_transaction(
            self: Pin<&mut AccountRegister>,
            account_id: &QString,
            transaction_id: &QString,
            form: &QVariant,
        ) -> QVariant;

        /// Removes a transaction. Returns an empty map, or the error.
        #[qinvokable]
        #[cxx_name = "removeTransaction"]
        fn remove_transaction(
            self: Pin<&mut AccountRegister>,
            account_id: &QString,
            transaction_id: &QString,
        ) -> QVariant;

        /// An amount in the account's currency written the way the user
        /// should type it.
        #[qinvokable]
        #[cxx_name = "amountExample"]
        fn amount_example(self: &AccountRegister, account_id: &QString) -> QString;
    }

    extern "RustQt" {
        /// How many rows the list has.
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &AccountRegister, parent: &QModelIndex) -> i32;

        /// What a row shows for a role.
        #[cxx_override]
        fn data(self: &AccountRegister, index: &QModelIndex, role: i32) -> QVariant;

        /// The names QML knows the roles by.
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &AccountRegister) -> QHash_i32_QByteArray;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut AccountRegister>);

        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut AccountRegister>);
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut AccountRegister>);
    }

    impl cxx_qt::Initialize for AccountRegister {}
}

/// The roles a row has, numbered from Qt's first user role.
const ROLES: [(&str, i32); 11] = [
    ("transactionId", 0x0100),
    ("date", 0x0101),
    ("otherParty", 0x0102),
    ("category", 0x0103),
    ("amount", 0x0104),
    ("exactAmount", 0x0105),
    ("incoming", 0x0106),
    ("startingBalance", 0x0107),
    ("kind", 0x0108),
    ("balance", 0x0109),
    ("exactBalance", 0x010A),
];

/// The Rust side of [`qobject::AccountRegister`].
#[derive(Default)]
pub struct AccountRegisterRust {
    account_id: QString,
    account_name: QString,
    balance: QString,
    exact_balance: QString,
    owes: bool,
    sort_key: QString,
    descending: bool,
    count: i32,
    today: QString,
    sort: Sort,
    rows: Vec<RegisterRow>,
}

impl cxx_qt::Initialize for qobject::AccountRegister {
    fn initialize(mut self: Pin<&mut Self>) {
        let today = QtLocale.short_date(QtLocale.today());
        let sort = Sort::default();
        let mut model = self.as_mut().rust_mut();
        model.today = QString::from(today.as_str());
        model.sort = sort;
        model.sort_key = QString::from(sort.key.key());
        model.descending = sort.descending;
    }
}

impl qobject::AccountRegister {
    fn show(mut self: Pin<&mut Self>, account_id: &QString) {
        self.as_mut().rust_mut().account_id = account_id.clone();
        self.as_mut().account_id_changed();
        self.refresh();
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let account_id = self.rust().account_id.to_string();
        let sort = self.rust().sort;
        let locale = app_locale();
        let view = session::with_book(|book| {
            let account = transaction_entry::account_choices(book)?
                .into_iter()
                .find(|choice| choice.id == account_id);
            let Some(choice) = account else {
                return Ok(None);
            };
            let found = book
                .all_accounts()?
                .into_iter()
                .find(|account| account.id.to_string() == choice.id)
                .map(|account| account.id);
            found
                .map(|id| register_view(book, id, sort, &locale))
                .transpose()
        });
        let view = match view {
            Some(Ok(Some(view))) => Some(view),
            Some(Ok(None)) | None => None,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the register: {error:?}");
                None
            }
        };
        self.as_mut().begin_reset_model();
        {
            let mut model = self.as_mut().rust_mut();
            match view {
                Some(view) => {
                    model.account_name = QString::from(view.account_name.as_str());
                    model.balance = QString::from(view.balance.text.as_str());
                    model.exact_balance = QString::from(view.balance.exact.as_str());
                    model.owes = view.owes;
                    model.rows = view.rows;
                }
                None => {
                    model.account_name = QString::default();
                    model.balance = QString::default();
                    model.exact_balance = QString::default();
                    model.owes = false;
                    model.rows = Vec::new();
                }
            }
            model.count = i32::try_from(model.rows.len()).unwrap_or(i32::MAX);
        }
        self.as_mut().end_reset_model();
        self.as_mut().account_name_changed();
        self.as_mut().balance_changed();
        self.as_mut().exact_balance_changed();
        self.as_mut().owes_changed();
        self.as_mut().count_changed();
    }

    fn sort_by(mut self: Pin<&mut Self>, key: &QString, descending: bool) {
        let Some(key) = SortKey::from_key(&key.to_string()) else {
            return;
        };
        let sort = Sort { key, descending };
        {
            let mut model = self.as_mut().rust_mut();
            model.sort = sort;
            model.sort_key = QString::from(key.key());
            model.descending = descending;
        }
        self.as_mut().sort_key_changed();
        self.as_mut().descending_changed();
        self.refresh();
    }

    fn account_choices(&self) -> QVariant {
        let found = session::with_book(transaction_entry::account_choices);
        match found {
            Some(Ok(choices)) => list(
                choices
                    .iter()
                    .map(|choice| map([("id", text(&choice.id)), ("name", text(&choice.name))])),
            ),
            _ => list([]),
        }
    }

    fn category_choices(
        &self,
        account_id: &QString,
        money_in: bool,
        current: &QString,
    ) -> QVariant {
        let (account_id, current) = (account_id.to_string(), current.to_string());
        let found = session::with_book(|book| {
            transaction_entry::category_choices(book, &account_id, money_in, &current)
                .map_err(|error| error_text(book, &account_id, &error))
        });
        match found {
            Some(Ok(choices)) => list(choices.iter().map(|choice| {
                map([
                    ("id", text(&choice.id)),
                    ("path", text(&choice.path)),
                    (
                        "depth",
                        QVariant::from(&i32::try_from(choice.depth).unwrap_or(0)),
                    ),
                ])
            })),
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn details(&self, account_id: &QString, transaction_id: &QString) -> QVariant {
        let (account_id, transaction_id) = (account_id.to_string(), transaction_id.to_string());
        let found = session::with_book(|book| {
            transaction_entry::details(book, &account_id, &transaction_id, &app_locale())
                .map_err(|error| error_text(book, &account_id, &error))
        });
        match found {
            Some(Ok(form)) => map([
                ("id", text(&form.id)),
                ("kind", text(form.kind)),
                ("account", text(&form.account)),
                ("moneyIn", QVariant::from(&form.money_in)),
                ("amount", text(&form.amount)),
                ("otherParty", text(&form.other_party)),
                ("category", text(&form.category)),
                ("otherAccount", text(&form.other_account)),
                ("date", text(&form.date)),
                ("imported", QVariant::from(&form.imported)),
            ]),
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn add_transaction(self: Pin<&mut Self>, account_id: &QString, form: &QVariant) -> QVariant {
        let account_id = account_id.to_string();
        let form = Form::read(form);
        let entered = form.entry(&account_id);
        let result = change(&account_id, |book| {
            transaction_entry::add(book, &entered, &app_locale())
        });
        self.finish(result)
    }

    fn edit_transaction(
        self: Pin<&mut Self>,
        account_id: &QString,
        transaction_id: &QString,
        form: &QVariant,
    ) -> QVariant {
        let (account_id, transaction_id) = (account_id.to_string(), transaction_id.to_string());
        let form = Form::read(form);
        let entered = form.entry(&account_id);
        let result = change(&account_id, |book| {
            transaction_entry::edit(book, &transaction_id, &entered, &app_locale())
        });
        self.finish(result)
    }

    fn remove_transaction(
        self: Pin<&mut Self>,
        account_id: &QString,
        transaction_id: &QString,
    ) -> QVariant {
        let (account_id, transaction_id) = (account_id.to_string(), transaction_id.to_string());
        let result = change(&account_id, |book| {
            transaction_entry::remove(book, &account_id, &transaction_id)
        });
        self.finish(result)
    }

    fn amount_example(&self, account_id: &QString) -> QString {
        let account_id = account_id.to_string();
        let example = session::with_book(|book| {
            transaction_entry::example(book, &account_id, &app_locale()).unwrap_or_default()
        });
        QString::from(example.unwrap_or_default().as_str())
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().count
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(row) = usize::try_from(index.row())
            .ok()
            .and_then(|row| self.rust().rows.get(row))
        else {
            return QVariant::default();
        };
        match ROLES.iter().find(|(_, number)| *number == role) {
            Some(("transactionId", _)) => text(&row.id),
            Some(("date", _)) => text(&row.date),
            Some(("otherParty", _)) => text(&row.other_party),
            Some(("category", _)) => text(&row.category),
            Some(("amount", _)) => text(&row.amount.text),
            Some(("exactAmount", _)) => text(&row.amount.exact),
            Some(("incoming", _)) => QVariant::from(&row.incoming),
            Some(("startingBalance", _)) => QVariant::from(&row.starting_balance),
            Some(("kind", _)) => text(row.kind),
            Some(("balance", _)) => text(&row.balance.text),
            Some(("exactBalance", _)) => text(&row.balance.exact),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (name, number) in ROLES {
            names.insert(number, QByteArray::from(name));
        }
        names
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

/// What the page's form holds, as typed; a missing field counts as empty.
struct Form {
    money_in: bool,
    amount: String,
    other_party: String,
    category: String,
    date: String,
}

impl Form {
    fn read(form: &QVariant) -> Self {
        let map = form
            .value::<QMap<QMapPair_QString_QVariant>>()
            .unwrap_or_default();
        let text = |key: &str| {
            map.get(&QString::from(key))
                .and_then(|value| value.value::<QString>())
                .map(|value| value.to_string())
                .unwrap_or_default()
        };
        Self {
            money_in: map
                .get(&QString::from("moneyIn"))
                .and_then(|value| value.value::<bool>())
                .unwrap_or(false),
            amount: text("amount"),
            other_party: text("otherParty"),
            category: text("category"),
            date: text("date"),
        }
    }

    fn entry<'a>(&'a self, account: &'a str) -> NewEntry<'a> {
        NewEntry {
            account,
            money_in: self.money_in,
            amount: &self.amount,
            other_party: &self.other_party,
            category: &self.category,
            date: &self.date,
        }
    }
}

/// Changes the open book, turning an error into what the UI shows.
fn change(
    account_id: &str,
    f: impl FnOnce(&mut Book) -> Result<(), EntryError>,
) -> Result<(), ErrorText> {
    session::with_book_mut(|book| f(book).map_err(|error| error_text(book, account_id, &error)))
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

fn error_text(book: &Book, account_id: &str, error: &EntryError) -> ErrorText {
    transaction_entry::error_text(book, account_id, error, &app_locale())
}
