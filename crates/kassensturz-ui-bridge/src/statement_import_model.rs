// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind the import screens (M3.10): one file at a time,
//! from choosing it to the summary. The work is in `statement_import.rs`;
//! this file only moves text between it and QML (D-065).

use core::pin::Pin;
use std::path::PathBuf;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QUrl, QVariant};
use kassensturz_core::TransactionId;
use kassensturz_import::{Limits, read_file};

use crate::book_session::local_path;
use crate::errors::ErrorText;
use crate::number_format::app_locale;
use crate::register_model::{field, flag, suggestion_object};
use crate::session;
use crate::settings::Settings;
use crate::statement_import::{
    self, BalanceText, CsvForm, EntryText, FileKind, ImportView, Loaded, StatementImportError,
};
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The import of one file from the user's bank into one account
        /// (M3.10). `step` says which screen is next: `statements` (the
        /// file has statements for several accounts), `csv` (the column
        /// screen), `preview`, `done`, or empty when nothing is loaded.
        /// `view` holds what that screen shows, all text.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, step, READ, NOTIFY)]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        #[qproperty(QString, account_id, cxx_name = "accountId", READ, NOTIFY)]
        #[qproperty(QString, file_name, cxx_name = "fileName", READ, NOTIFY)]
        type StatementImport = super::StatementImportRust;
    }

    extern "RustQt" {
        /// Reads `file` for `account_id` and recognizes its format. An
        /// error object says why it couldn't be used; otherwise `step` says
        /// what comes next.
        #[qinvokable]
        fn start(self: Pin<&mut StatementImport>, account_id: &QString, file: &QUrl) -> QVariant;

        /// The statement at `index` in the file is the account's.
        #[qinvokable]
        #[cxx_name = "chooseStatement"]
        fn choose_statement(self: Pin<&mut StatementImport>, index: i32) -> QVariant;

        /// Imports the entry `key` names although it looks like a transaction
        /// the account has (D-092), or leaves it out again; the preview is
        /// read again.
        #[qinvokable]
        #[cxx_name = "setImportAnyway"]
        fn set_import_anyway(self: Pin<&mut StatementImport>, key: &QString, on: bool) -> QVariant;

        /// Imports what the preview shows, as one user action.
        #[qinvokable]
        #[cxx_name = "importNow"]
        fn import_now(self: Pin<&mut StatementImport>) -> QVariant;

        /// The Transfers the new transactions suggest, for `TransferPrompt`.
        #[qinvokable]
        #[cxx_name = "transferSuggestions"]
        fn transfer_suggestions(self: &StatementImport) -> QVariant;

        /// The banks whose CSV settings are kept, by name (D-111).
        #[qinvokable]
        #[cxx_name = "csvBanks"]
        fn csv_banks(self: &StatementImport) -> QVariant;

        /// The column screen's form for the bank `name`, or the usual
        /// choices for an empty or unknown name.
        #[qinvokable]
        #[cxx_name = "csvForm"]
        fn csv_form(self: &StatementImport, name: &QString) -> QVariant;

        /// The first rows of the file as `form`'s encoding, separator and
        /// skipped lines read them: a list of lists of text, or an error.
        #[qinvokable]
        #[cxx_name = "csvRows"]
        fn csv_rows(self: &StatementImport, form: &QVariant) -> QVariant;

        /// Reads the CSV file with `form`, keeps the settings for the bank
        /// `name` unless it is empty, and goes on to the preview.
        #[qinvokable]
        #[cxx_name = "readCsv"]
        fn read_csv(self: Pin<&mut StatementImport>, form: &QVariant, name: &QString) -> QVariant;

        /// Forgets the file.
        #[qinvokable]
        fn cancel(self: Pin<&mut StatementImport>);
    }

    unsafe extern "RustQt" {
        /// The book changed: an import was made.
        #[qsignal]
        fn changed(self: Pin<&mut StatementImport>);
    }

    impl cxx_qt::Initialize for StatementImport {}
}

/// The Rust side of [`qobject::StatementImport`].
#[derive(Default)]
pub struct StatementImportRust {
    step: QString,
    view: QVariant,
    account_id: QString,
    file_name: QString,
    /// The file's bytes, kept for the column screen.
    input: Vec<u8>,
    path: PathBuf,
    loaded: Option<Loaded>,
    /// The transactions the last import added.
    new: Vec<TransactionId>,
}

impl cxx_qt::Initialize for qobject::StatementImport {
    fn initialize(self: Pin<&mut Self>) {}
}

impl qobject::StatementImport {
    fn start(mut self: Pin<&mut Self>, account_id: &QString, file: &QUrl) -> QVariant {
        self.as_mut().cancel();
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        let account = account_id.to_string();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        {
            let mut rust = self.as_mut().rust_mut();
            rust.account_id = QString::from(account.as_str());
            rust.file_name = QString::from(name.as_str());
            rust.path = path.clone();
        }
        self.as_mut().account_id_changed();
        self.as_mut().file_name_changed();
        let input = match read_file(&path, &Limits::default()) {
            Ok(input) => input,
            Err(error) => return self.fail(&StatementImportError::Import(error)),
        };
        let kind = match statement_import::recognize(&input, &name) {
            Ok(kind) => kind,
            Err(error) => return self.fail(&error),
        };
        self.as_mut().rust_mut().input = input;
        match kind {
            FileKind::Csv => {
                self.set_step("csv", QVariant::default());
                map([])
            }
            FileKind::Statements(importer) => {
                let read = statement_import::read_statements(importer, &self.rust().input);
                self.loaded(read)
            }
        }
    }

    fn choose_statement(mut self: Pin<&mut Self>, index: i32) -> QVariant {
        let chosen = usize::try_from(index)
            .map_err(|_| StatementImportError::NoSuchEntry)
            .and_then(|index| {
                self.as_mut()
                    .rust_mut()
                    .loaded
                    .as_mut()
                    .ok_or(StatementImportError::NoSuchEntry)?
                    .choose_statement(index)
            });
        match chosen {
            Ok(()) => self.show_preview(),
            Err(error) => self.fail(&error),
        }
    }

    fn set_import_anyway(mut self: Pin<&mut Self>, key: &QString, on: bool) -> QVariant {
        let key = key.to_string();
        let set = self
            .as_mut()
            .rust_mut()
            .loaded
            .as_mut()
            .ok_or(StatementImportError::NoSuchEntry)
            .and_then(|loaded| loaded.set_import_anyway(&key, on));
        match set {
            Ok(()) => self.show_preview(),
            Err(error) => self.fail(&error),
        }
    }

    fn import_now(mut self: Pin<&mut Self>) -> QVariant {
        let Some(loaded) = self.rust().loaded.as_ref() else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        let imported = session::with_book_mut(|book| {
            statement_import::import(book, loaded, &app_locale())
                .map_err(|error| statement_import::error_text(book, &error, &app_locale()))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match imported {
            Ok(view) => {
                let shown = view_object(&view);
                self.as_mut().rust_mut().new = view.new;
                self.as_mut().set_step("done", shown);
                self.as_mut().changed();
                map([])
            }
            Err(error) => error_object(&error),
        }
    }

    fn transfer_suggestions(&self) -> QVariant {
        let new = &self.rust().new;
        let found = session::with_book(|book| {
            statement_import::transfer_suggestions(book, new, &app_locale())
                .map_err(|error| statement_import::error_text(book, &error, &app_locale()))
        });
        match found {
            Some(Ok(suggestions)) => list(suggestions.iter().map(suggestion_object)),
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn csv_banks(&self) -> QVariant {
        list(
            statement_import::csv_banks(&Settings::user())
                .iter()
                .map(|name| text(name)),
        )
    }

    fn csv_form(&self, name: &QString) -> QVariant {
        let form = statement_import::csv_bank(&Settings::user(), &name.to_string())
            .map_or_else(CsvForm::usual, |settings| CsvForm::from_settings(&settings));
        form_object(&form)
    }

    fn csv_rows(&self, form: &QVariant) -> QVariant {
        let form = read_form(form);
        match form.rows(&self.rust().input) {
            Ok(rows) => list(
                rows.iter()
                    .map(|row| list(row.iter().map(|cell| text(cell)))),
            ),
            Err(error) => self.fail_text(&error),
        }
    }

    fn read_csv(self: Pin<&mut Self>, form: &QVariant, name: &QString) -> QVariant {
        let form = read_form(form);
        let settings = match form.settings() {
            Ok(settings) => settings,
            Err(error) => return self.fail(&error),
        };
        let account = self.rust().account_id.to_string();
        let read = session::with_book(|book| {
            let account = crate::transaction_entry::find_account(book, &account)
                .map_err(|_| StatementImportError::NoSuchEntry)?;
            statement_import::read_csv(book, account.id, &self.rust().input, &settings)
        })
        .unwrap_or(Err(StatementImportError::NoSuchEntry));
        if read.is_ok()
            && !statement_import::save_csv_bank(&Settings::user(), &name.to_string(), &settings)
        {
            eprintln!("kassensturz: couldn't keep the bank's CSV settings");
        }
        self.loaded(read)
    }

    fn cancel(mut self: Pin<&mut Self>) {
        {
            let mut rust = self.as_mut().rust_mut();
            rust.input = Vec::new();
            rust.loaded = None;
            rust.new = Vec::new();
        }
        self.set_step("", QVariant::default());
    }

    /// Takes the statements just read, and shows the next screen: the
    /// choice of statement, or the preview.
    fn loaded(
        mut self: Pin<&mut Self>,
        read: Result<Vec<kassensturz_import::Statement>, StatementImportError>,
    ) -> QVariant {
        let statements = match read {
            Ok(statements) => statements,
            Err(error) => return self.fail(&error),
        };
        let account = self.rust().account_id.to_string();
        let path = self.rust().path.clone();
        let loaded = session::with_book(|book| {
            crate::transaction_entry::find_account(book, &account)
                .map(|account| Loaded::new(account.id, path, statements))
                .map_err(|_| StatementImportError::NoSuchEntry)
        })
        .unwrap_or(Err(StatementImportError::NoSuchEntry));
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => return self.fail(&error),
        };
        if loaded.needs_statement_choice() {
            let choices = session::with_book(|book| {
                statement_import::statement_choices(book, &loaded, &app_locale())
            })
            .unwrap_or_default();
            let view = list(choices.iter().map(|choice| {
                map([
                    (
                        "index",
                        QVariant::from(&i32::try_from(choice.index).unwrap_or(0)),
                    ),
                    ("account", text(&choice.account)),
                    ("firstDate", text(&choice.first_date)),
                    ("lastDate", text(&choice.last_date)),
                    (
                        "entries",
                        QVariant::from(&i32::try_from(choice.entries).unwrap_or(0)),
                    ),
                    (
                        "closing",
                        text(choice.closing.as_ref().map_or("", |closing| &closing.text)),
                    ),
                    ("closingDate", text(&choice.closing_date)),
                ])
            }));
            self.as_mut().rust_mut().loaded = Some(loaded);
            self.set_step("statements", view);
            return map([]);
        }
        self.as_mut().rust_mut().loaded = Some(loaded);
        self.show_preview()
    }

    fn show_preview(mut self: Pin<&mut Self>) -> QVariant {
        let Some(loaded) = self.rust().loaded.as_ref() else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        let previewed = session::with_book_mut(|book| {
            statement_import::preview(book, loaded, &app_locale())
                .map_err(|error| statement_import::error_text(book, &error, &app_locale()))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match previewed {
            Ok(view) => {
                let shown = view_object(&view);
                self.as_mut().set_step("preview", shown);
                map([])
            }
            Err(error) => error_object(&error),
        }
    }

    fn set_step(mut self: Pin<&mut Self>, step: &str, view: QVariant) {
        {
            let mut rust = self.as_mut().rust_mut();
            rust.step = QString::from(step);
            rust.view = view;
        }
        self.as_mut().step_changed();
        self.as_mut().view_changed();
    }

    /// Forgets the file and says why it couldn't be used.
    fn fail(mut self: Pin<&mut Self>, error: &StatementImportError) -> QVariant {
        let shown = self.fail_text(error);
        self.as_mut().rust_mut().loaded = None;
        self.set_step("", QVariant::default());
        shown
    }

    fn fail_text(&self, error: &StatementImportError) -> QVariant {
        let shown =
            session::with_book(|book| statement_import::error_text(book, error, &app_locale()))
                .unwrap_or_else(|| ErrorText::plain("input.no-such-entry"));
        error_object(&shown)
    }
}

fn view_object(view: &ImportView) -> QVariant {
    let count = |value: usize| QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX));
    map([
        ("fileName", text(&view.file_name)),
        ("accountName", text(&view.account_name)),
        ("debt", QVariant::from(&view.debt)),
        ("entries", list(view.entries.iter().map(entry_object))),
        ("imported", count(view.imported)),
        ("toDecide", count(view.to_decide)),
        ("duplicates", count(view.duplicates)),
        ("pending", count(view.pending)),
        ("rules", count(view.rules)),
        ("transfers", count(view.transfers)),
        ("balances", list(view.balances.iter().map(balance_object))),
    ])
}

fn entry_object(entry: &EntryText) -> QVariant {
    map([
        ("key", text(&entry.key)),
        ("date", text(&entry.date)),
        ("otherParty", text(&entry.other_party)),
        ("purpose", text(&entry.purpose)),
        ("amount", text(&entry.amount.text)),
        ("exactAmount", text(&entry.amount.exact)),
        ("incoming", QVariant::from(&entry.incoming)),
        ("outcome", text(entry.outcome)),
        ("category", text(&entry.category)),
        (
            "similar",
            list(entry.similar.iter().map(|similar| {
                map([
                    ("id", text(&similar.id)),
                    ("date", text(&similar.date)),
                    ("otherParty", text(&similar.other_party)),
                    ("category", text(&similar.category)),
                    ("amount", text(&similar.amount.text)),
                    ("exactAmount", text(&similar.amount.exact)),
                ])
            })),
        ),
        ("importAnyway", QVariant::from(&entry.import_anyway)),
    ])
}

fn balance_object(balance: &BalanceText) -> QVariant {
    map([
        ("which", text(balance.which)),
        ("date", text(&balance.date)),
        ("bank", text(&balance.bank.exact)),
        ("bankOwes", QVariant::from(&balance.bank_owes)),
        ("book", text(&balance.book.exact)),
        ("bookOwes", QVariant::from(&balance.book_owes)),
        ("matches", QVariant::from(&balance.matches)),
        ("difference", text(&balance.difference.exact)),
        ("bookHigher", QVariant::from(&balance.book_higher)),
        ("checkAmount", text(&balance.check_amount)),
        ("checkDate", text(&balance.check_date)),
        ("stated", QVariant::from(&balance.stated)),
    ])
}

/// The column screen's form as QML sends it: every field as text, except
/// the header switch.
fn read_form(form: &QVariant) -> CsvForm {
    let map = form
        .value::<QMap<QMapPair_QString_QVariant>>()
        .unwrap_or_default();
    CsvForm {
        encoding: field(&map, "encoding"),
        delimiter: field(&map, "delimiter"),
        skip_lines: field(&map, "skipLines"),
        skip_last_lines: field(&map, "skipLastLines"),
        header: flag(&map, "header"),
        decimal_mark: field(&map, "decimalMark"),
        date_order: field(&map, "dateOrder"),
        booking_date: field(&map, "bookingDate"),
        value_date: field(&map, "valueDate"),
        amount_mode: field(&map, "amountMode"),
        amount: field(&map, "amount"),
        debit: field(&map, "debit"),
        credit: field(&map, "credit"),
        direction: field(&map, "direction"),
        money_out: field(&map, "moneyOut"),
        money_in: field(&map, "moneyIn"),
        other_party_name: field(&map, "otherPartyName"),
        other_party_iban: field(&map, "otherPartyIban"),
        purpose: field(&map, "purpose"),
        purpose_2: field(&map, "purpose2"),
        status: field(&map, "status"),
        booked: field(&map, "booked"),
        currency: field(&map, "currency"),
        account: field(&map, "account"),
    }
}

fn form_object(form: &CsvForm) -> QVariant {
    map([
        ("encoding", text(&form.encoding)),
        ("delimiter", text(&form.delimiter)),
        ("skipLines", text(&form.skip_lines)),
        ("skipLastLines", text(&form.skip_last_lines)),
        ("header", QVariant::from(&form.header)),
        ("decimalMark", text(&form.decimal_mark)),
        ("dateOrder", text(&form.date_order)),
        ("bookingDate", text(&form.booking_date)),
        ("valueDate", text(&form.value_date)),
        ("amountMode", text(&form.amount_mode)),
        ("amount", text(&form.amount)),
        ("debit", text(&form.debit)),
        ("credit", text(&form.credit)),
        ("direction", text(&form.direction)),
        ("moneyOut", text(&form.money_out)),
        ("moneyIn", text(&form.money_in)),
        ("otherPartyName", text(&form.other_party_name)),
        ("otherPartyIban", text(&form.other_party_iban)),
        ("purpose", text(&form.purpose)),
        ("purpose2", text(&form.purpose_2)),
        ("status", text(&form.status)),
        ("booked", text(&form.booked)),
        ("currency", text(&form.currency)),
        ("account", text(&form.account)),
    ])
}
