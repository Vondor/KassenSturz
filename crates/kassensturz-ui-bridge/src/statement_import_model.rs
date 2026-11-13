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

use crate::bank_fetch;
use crate::book_session::local_path;
use crate::errors::ErrorText;
use crate::number_format::app_locale;
use crate::register_model::{field, flag, suggestion_object};
use crate::session;
use crate::settings::Settings;
use crate::statement_import::{
    self, BalanceText, CsvForm, EntryText, FileKind, ImportView, JoinedText, Loaded, PlannedText,
    StatementImportError,
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

        /// Looks at the statement just fetched from the bank for
        /// `account_id` (M4.7, D-153) the way a file's is looked at: the
        /// preview comes next. An error object says why it can't be shown,
        /// e.g. `input.no-such-entry` when nothing was fetched for the
        /// account.
        #[qinvokable]
        #[cxx_name = "startFromBank"]
        fn start_from_bank(self: Pin<&mut StatementImport>, account_id: &QString) -> QVariant;

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

        /// Takes back the pairs the import joined by themselves
        /// (D-749), the entries stay. Refused with an error object when
        /// something else was changed since. Returns an empty map when it
        /// worked.
        #[qinvokable]
        #[cxx_name = "undoJoins"]
        fn undo_joins(self: Pin<&mut StatementImport>) -> QVariant;

        /// Takes back the matches of new entries to planned payments the
        /// import made (D-769); the entries stay and the payments wait
        /// again. Refused with an error object when something else was
        /// changed since. Returns an empty map when it worked.
        #[qinvokable]
        #[cxx_name = "undoMatches"]
        fn undo_matches(self: Pin<&mut StatementImport>) -> QVariant;

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

    fn start_from_bank(mut self: Pin<&mut Self>, account_id: &QString) -> QVariant {
        self.as_mut().cancel();
        let account = account_id.to_string();
        let found = session::with_book(|book| {
            crate::transaction_entry::find_account(book, &account)
                .map(|account| account.id)
                .map_err(|_| StatementImportError::NoSuchEntry)
        })
        .unwrap_or(Err(StatementImportError::NoSuchEntry));
        let fetched = found.and_then(|id| {
            bank_fetch::take(id)
                .map(|fetch| Loaded::from_bank(id, fetch.statement, fetch.day))
                .ok_or(StatementImportError::NoSuchEntry)
        });
        let loaded = match fetched {
            Ok(loaded) => loaded,
            Err(error) => return self.fail(&error),
        };
        {
            let mut rust = self.as_mut().rust_mut();
            rust.account_id = QString::from(account.as_str());
            rust.file_name = QString::default();
            rust.path = PathBuf::new();
            rust.loaded = Some(loaded);
        }
        self.as_mut().account_id_changed();
        self.as_mut().file_name_changed();
        self.show_preview()
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

    fn undo_joins(self: Pin<&mut Self>) -> QVariant {
        self.undo_with(statement_import::undo_joins)
    }

    fn undo_matches(self: Pin<&mut Self>) -> QVariant {
        self.undo_with(statement_import::undo_matches)
    }

    /// Runs `undo` on the open book and tells the screens.
    fn undo_with(
        mut self: Pin<&mut Self>,
        undo: fn(&mut kassensturz_store::Book) -> Result<(), StatementImportError>,
    ) -> QVariant {
        let undone = session::with_book_mut(|book| {
            undo(book).map_err(|error| statement_import::error_text(book, &error, &app_locale()))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match undone {
            Ok(()) => {
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
        ("fromBank", QVariant::from(&view.from_bank)),
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
        ("joined", list(view.joined.iter().map(joined_object))),
        ("needsYou", count(view.needs_you)),
        ("planned", list(view.planned.iter().map(planned_object))),
        ("plannedNeedsYou", count(view.planned_needs_you)),
    ])
}

/// A new entry matched to a planned payment, as `ImportDonePage.qml` lists
/// it (D-769).
fn planned_object(planned: &PlannedText) -> QVariant {
    map([
        ("name", text(&planned.name)),
        ("due", text(&planned.due)),
        ("date", text(&planned.date)),
        ("amount", text(&planned.amount.text)),
        ("exactAmount", text(&planned.amount.exact)),
        ("categorySet", QVariant::from(&planned.category_set)),
    ])
}

/// A pair that was joined by itself, as `ImportDonePage.qml` lists it.
fn joined_object(joined: &JoinedText) -> QVariant {
    map([
        ("amount", text(&joined.amount.text)),
        ("exactAmount", text(&joined.amount.exact)),
        ("from", text(&joined.from)),
        ("fromDate", text(&joined.from_date)),
        ("to", text(&joined.to)),
        ("toDate", text(&joined.to_date)),
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
                    ("kind", text(similar.kind)),
                    ("note", text(&similar.note)),
                    ("bankName", text(&similar.bank_name)),
                    ("purpose", text(&similar.purpose)),
                    (
                        "parts",
                        list(similar.parts.iter().map(|part| {
                            map([
                                ("name", text(&part.name)),
                                ("amount", text(&part.amount.text)),
                                ("exactAmount", text(&part.amount.exact)),
                            ])
                        })),
                    ),
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

#[cfg(test)]
mod tests {
    use cxx_qt_lib::QList;

    use super::*;
    use crate::register::Shown;
    use crate::statement_import::{JoinedText, SimilarPart, SimilarText};

    fn object(value: &QVariant) -> QMap<QMapPair_QString_QVariant> {
        value
            .value::<QMap<QMapPair_QString_QVariant>>()
            .expect("an object")
    }

    fn text_of(value: &QVariant, key: &str) -> String {
        object(value)
            .get(&QString::from(key))
            .and_then(|value| value.value::<QString>())
            .map(|value| value.to_string())
            .unwrap_or_else(|| panic!("no text `{key}`"))
    }

    fn list_of(value: &QVariant, key: &str) -> Vec<QVariant> {
        object(value)
            .get(&QString::from(key))
            .and_then(|value| value.value::<QList<QVariant>>())
            .map(|list| list.iter().cloned().collect())
            .unwrap_or_else(|| panic!("no list `{key}`"))
    }

    fn shown(exact: &str) -> Shown {
        Shown {
            text: exact.to_owned(),
            exact: exact.to_owned(),
        }
    }

    /// The summary lists the pairs joined by themselves with the fields
    /// `ImportDonePage.qml` reads (US6.3, D-749).
    #[test]
    fn a_joined_pair_reaches_qml_with_both_accounts_and_dates() {
        let joined = joined_object(&JoinedText {
            amount: shown("100,00 €"),
            from: "Checking account".to_owned(),
            from_date: "2026-11-02".to_owned(),
            to: "Savings".to_owned(),
            to_date: "2026-11-03".to_owned(),
        });
        assert_eq!(text_of(&joined, "exactAmount"), "100,00 €");
        assert_eq!(text_of(&joined, "from"), "Checking account");
        assert_eq!(text_of(&joined, "fromDate"), "2026-11-02");
        assert_eq!(text_of(&joined, "to"), "Savings");
        assert_eq!(text_of(&joined, "toDate"), "2026-11-03");
        assert_eq!(text_of(&joined, "amount"), "100,00 €");
    }

    #[test]
    fn the_summary_carries_the_joins_and_the_moves_that_wait_for_the_user() {
        let view = ImportView {
            file_name: String::new(),
            from_bank: false,
            account_name: "Savings".to_owned(),
            debt: false,
            entries: Vec::new(),
            imported: 1,
            to_decide: 0,
            duplicates: 0,
            pending: 0,
            rules: 0,
            transfers: 0,
            balances: Vec::new(),
            new: Vec::new(),
            joined: vec![JoinedText {
                amount: shown("100,00 €"),
                from: "Checking account".to_owned(),
                from_date: "2026-11-02".to_owned(),
                to: "Savings".to_owned(),
                to_date: "2026-11-02".to_owned(),
            }],
            needs_you: 2,
            planned: vec![PlannedText {
                name: "Rent".to_owned(),
                due: "2026-10-01".to_owned(),
                date: "2026-10-02".to_owned(),
                amount: shown("-750,00 €"),
                category_set: true,
            }],
            planned_needs_you: 3,
        };
        let object = view_object(&view);
        assert_eq!(list_of(&object, "joined").len(), 1);
        let count = |key: &str| {
            object
                .value::<QMap<QMapPair_QString_QVariant>>()
                .and_then(|map| map.get(&QString::from(key)))
                .and_then(|value| value.value::<i32>())
        };
        assert_eq!(count("needsYou"), Some(2));
        assert_eq!(count("plannedNeedsYou"), Some(3));
        let planned = list_of(&object, "planned");
        assert_eq!(planned.len(), 1);
        assert_eq!(text_of(&planned[0], "name"), "Rent");
        assert_eq!(text_of(&planned[0], "due"), "2026-10-01");
        assert_eq!(text_of(&planned[0], "date"), "2026-10-02");
        assert_eq!(text_of(&planned[0], "amount"), "-750,00 €");
        assert_eq!(text_of(&planned[0], "exactAmount"), "-750,00 €");
        assert_eq!(
            planned[0]
                .value::<QMap<QMapPair_QString_QVariant>>()
                .and_then(|map| map.get(&QString::from("categorySet")))
                .and_then(|value| value.value::<bool>()),
            Some(true)
        );
    }

    /// The preview shows the book's side of a possible double with the
    /// fields QML reads; each is read back here the way QML does.
    #[test]
    fn a_possible_double_reaches_qml_with_what_the_book_entry_says() {
        let entry = EntryText {
            key: "0.0".to_owned(),
            date: "2026-10-06".to_owned(),
            other_party: "EXAMPLE SHOP".to_owned(),
            purpose: "Order 1".to_owned(),
            amount: shown("-12,00 €"),
            incoming: false,
            outcome: "similar",
            category: String::new(),
            similar: vec![SimilarText {
                id: "t1".to_owned(),
                date: "2026-10-05".to_owned(),
                other_party: "Shop".to_owned(),
                category: String::new(),
                amount: shown("-12,00 €"),
                kind: "parts",
                note: "For the party".to_owned(),
                bank_name: "EXAMPLE SHOP 7".to_owned(),
                purpose: "Order 1".to_owned(),
                parts: vec![
                    SimilarPart {
                        name: "Groceries".to_owned(),
                        amount: shown("-10,00 €"),
                    },
                    SimilarPart {
                        name: String::new(),
                        amount: shown("-2,00 €"),
                    },
                ],
            }],
            import_anyway: false,
        };
        let object = entry_object(&entry);
        let similar = list_of(&object, "similar");
        assert_eq!(similar.len(), 1);
        let book = &similar[0];
        assert_eq!(text_of(book, "date"), "2026-10-05");
        assert_eq!(text_of(book, "otherParty"), "Shop");
        assert_eq!(text_of(book, "category"), "");
        assert_eq!(text_of(book, "exactAmount"), "-12,00 €");
        assert_eq!(text_of(book, "kind"), "parts");
        assert_eq!(text_of(book, "note"), "For the party");
        assert_eq!(text_of(book, "bankName"), "EXAMPLE SHOP 7");
        assert_eq!(text_of(book, "purpose"), "Order 1");
        let parts = list_of(book, "parts");
        let read: Vec<(String, String)> = parts
            .iter()
            .map(|part| (text_of(part, "name"), text_of(part, "exactAmount")))
            .collect();
        assert_eq!(
            read,
            vec![
                ("Groceries".to_owned(), "-10,00 €".to_owned()),
                (String::new(), "-2,00 €".to_owned()),
            ]
        );
    }

    /// A statement can hold thousands of entries. A `Repeater` in a scrolling
    /// page makes a row for every one of them before the first frame
    /// (100 s for 3,000 entries, D-158), so the preview lists them in a
    /// `ListView`, which makes only the rows in view. The test reads the QML
    /// as text, like `register_model.rs` does for D-156.
    #[test]
    fn the_import_preview_lists_the_entries_in_a_list_view_that_makes_only_the_rows_in_view() {
        let page = include_str!("../qml/ImportPreviewPage.qml");
        assert!(page.contains("ListView {\n        id: entryList"));
        assert!(page.contains("model: page.entries"));
        assert!(
            !page.contains("Repeater {\n                model: page.entries"),
            "the entries must not be made all at once"
        );
    }
}
