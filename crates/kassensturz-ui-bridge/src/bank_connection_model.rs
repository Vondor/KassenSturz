// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind the bank screens (M4.6, D-151): setting up a bank
//! login, the TAN and question dialogs the bank asks for on the way, and
//! removing a login and its PIN in Settings. Since M4.7 it also fetches the
//! statements of the linked accounts (D-153), asks for the PIN when the
//! bank needs it, and links an account of the book to a bank account from
//! the fetch screen. The work is in
//! `bank_connection.rs`, on its own thread; this file moves text between it
//! and QML (D-065) and brings the worker's requests to the GUI thread as
//! signals.

use core::pin::Pin;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use kassensturz_bank::{BankError, BankQuestion, Pin as BankPin, Tan, TanRequest};
use kassensturz_core::Report;

use crate::bank_connection::{
    self, Answer, Connection, Event, FetchOutcome, FetchRow, FoundAccount, ListedBankAccount,
    LoginRow, Lookup, PinRequest, SetupDone, SetupForm, Status, Worker,
};
use crate::errors::ErrorText;
use crate::format::Locale;
use crate::number_format::app_locale;
use crate::platform;
use crate::register_model::field;
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The bank connection: one per app.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy, READ, WRITE, NOTIFY)]
        #[qproperty(bool, fake, READ, WRITE, NOTIFY)]
        type BankConnection = super::BankConnectionRust;
    }

    impl cxx_qt::Threading for BankConnection {}

    extern "RustQt" {
        /// Whether bank connections can be used: an object with `ready`
        /// and, when not, the error's `code` and parameters (D-054, D-058).
        #[qinvokable]
        fn status(self: Pin<&mut BankConnection>) -> QVariant;

        /// The logins set up on this computer: an object with `logins`, a
        /// list of objects with `id`, `bank`, `bankCode` and `kind`
        /// (`pin-tan` or `chip-card`), or an error object.
        #[qinvokable]
        fn logins(self: Pin<&mut BankConnection>) -> QVariant;

        /// Looks the bank of `iban` up: an object with `state`, `iban`,
        /// `country`, `bankCode`, `bankName`, `url`, `askUrl` and
        /// `customerId` (`none`, `required`, `optional`), or an error
        /// object.
        #[qinvokable]
        #[cxx_name = "lookUp"]
        fn look_up(self: Pin<&mut BankConnection>, iban: &QString) -> QVariant;

        /// Starts setting up a login from `form`, an object with `iban`,
        /// `url`, `loginName`, `customerId`, `customerIdRule`, `pin` and
        /// `pinMode` (`ask`, `remember`, `save`). An error object when the
        /// form isn't complete, else an empty object; the result comes
        /// with `setupFinished`.
        #[qinvokable]
        #[cxx_name = "setUp"]
        fn set_up(self: Pin<&mut BankConnection>, form: &QVariant) -> QVariant;

        /// Sends the TAN the user typed to the bank.
        #[qinvokable]
        #[cxx_name = "answerTan"]
        fn answer_tan(self: &BankConnection, tan: &QString);

        /// The user cancelled the TAN dialog.
        #[qinvokable]
        #[cxx_name = "cancelTan"]
        fn cancel_tan(self: &BankConnection);

        /// The user chose the button with `index`, or closed the question
        /// (`-1`).
        #[qinvokable]
        #[cxx_name = "answerQuestion"]
        fn answer_question(self: &BankConnection, index: i32);

        /// Removes the login `id` and its PIN (D-057). An empty object, or
        /// an error object.
        #[qinvokable]
        #[cxx_name = "removeLogin"]
        fn remove_login(self: Pin<&mut BankConnection>, id: &QString) -> QVariant;

        /// Forgets the PIN of the login `id`, remembered and saved. An
        /// empty object, or an error object.
        #[qinvokable]
        #[cxx_name = "forgetPin"]
        fn forget_pin(self: Pin<&mut BankConnection>, id: &QString) -> QVariant;

        /// What the accounts the last setup found can be linked to: an
        /// object with `accounts` (the book's, `id` and `name`) and
        /// `found` (the bank's, with `suggested`, the book account's ID or
        /// empty), or an error object.
        #[qinvokable]
        #[cxx_name = "linkChoices"]
        fn link_choices(self: &BankConnection) -> QVariant;

        /// Links the accounts the last setup found to the book accounts
        /// `choices` names, a list of IDs in the same order, empty for
        /// none. An object with `linked`, or an error object.
        #[qinvokable]
        fn link(self: &BankConnection, choices: &QVariant) -> QVariant;

        /// The open accounts of the book with what the fetch screen needs:
        /// an object with `accounts`, a list of objects with `id`, `name`,
        /// `linked`, `bankAccount` (the IBAN or number it is linked to) and
        /// `lastFetched` (a date as text, empty if never), or an error
        /// object.
        #[qinvokable]
        #[cxx_name = "fetchAccounts"]
        fn fetch_accounts(self: &BankConnection) -> QVariant;

        /// The bank accounts the logins list that the account `account_id`
        /// of the book could be linked to: an object with `choices`, a list
        /// of objects with `key`, `bank`, `iban`, `number`, `name` and
        /// `kind`, and `suggested`, the key of the one whose IBAN or number
        /// is the account's own, or empty; or an error object.
        #[qinvokable]
        #[cxx_name = "bankAccountChoices"]
        fn bank_account_choices(self: Pin<&mut BankConnection>, account_id: &QString) -> QVariant;

        /// Links the account `account_id` of the book to the bank account
        /// `key` named (one of `bankAccountChoices`), as a change the user
        /// can undo. An empty object, or an error object.
        #[qinvokable]
        #[cxx_name = "linkAccount"]
        fn link_account(
            self: Pin<&mut BankConnection>,
            account_id: &QString,
            key: &QString,
        ) -> QVariant;

        /// Starts fetching the statements of the accounts with these IDs (a
        /// list), which must be linked. An error object when it can't
        /// start, else an empty object; `fetchStarted` says which account
        /// the bank is asked about, `pinRequested` and `tanRequested` what
        /// it needs on the way, and `fetchFinished` the result.
        #[qinvokable]
        fn fetch(self: Pin<&mut BankConnection>, accounts: &QVariant) -> QVariant;

        /// Sends the PIN the user typed, to be kept as `mode` says (`ask`,
        /// `remember`, `save`).
        #[qinvokable]
        #[cxx_name = "answerPin"]
        fn answer_pin(self: &BankConnection, pin: &QString, mode: &QString);

        /// The user cancelled the PIN dialog.
        #[qinvokable]
        #[cxx_name = "cancelPin"]
        fn cancel_pin(self: &BankConnection);

        /// The bank asks for a TAN: an object with `title`, `challenge`,
        /// `minLength` and `maxLength`.
        #[qsignal]
        #[cxx_name = "tanRequested"]
        fn tan_requested(self: Pin<&mut BankConnection>, request: QVariant);

        /// The bank asks a question: an object with `level`, `title`,
        /// `text` and `buttons`, a list of labels.
        #[qsignal]
        #[cxx_name = "questionAsked"]
        fn question_asked(self: Pin<&mut BankConnection>, question: QVariant);

        /// The bank needs the PIN of a login: an object with `bank` (its
        /// name) and `rejected`, true when the bank has just called the PIN
        /// typed before wrong.
        #[qsignal]
        #[cxx_name = "pinRequested"]
        fn pin_requested(self: Pin<&mut BankConnection>, request: QVariant);

        /// The fetch has reached the account with this ID.
        #[qsignal]
        #[cxx_name = "fetchStarted"]
        fn fetch_started(self: Pin<&mut BankConnection>, account: QString);

        /// A fetch ended: an object with `outcomes`, a list of objects with
        /// `account`, `name`, `state` (`ready`, `failed` or `skipped`),
        /// `transactions` and `error` (an error object, empty unless it
        /// failed); or an error object when none was tried.
        #[qsignal]
        #[cxx_name = "fetchFinished"]
        fn fetch_finished(self: Pin<&mut BankConnection>, result: QVariant);

        /// Setting up a login ended: an object with `login` and `accounts`,
        /// or an error object.
        #[qsignal]
        #[cxx_name = "setupFinished"]
        fn setup_finished(self: Pin<&mut BankConnection>, result: QVariant);
    }

    impl cxx_qt::Initialize for BankConnection {}
}

/// The Rust side of [`qobject::BankConnection`].
#[derive(Default)]
pub struct BankConnectionRust {
    busy: bool,
    fake: bool,
    /// The worker, started when a screen first needs it.
    worker: Option<Worker>,
    /// What the last setup found, for linking.
    found: Vec<FoundAccount>,
}

impl cxx_qt::Initialize for qobject::BankConnection {
    fn initialize(self: Pin<&mut Self>) {}
}

impl qobject::BankConnection {
    /// The worker, started on first use. Its requests come back to this
    /// object on the GUI thread.
    fn worker(mut self: Pin<&mut Self>) -> &Worker {
        if self.worker.is_none() {
            let thread = self.qt_thread();
            let notify: bank_connection::Notify = Arc::new(move |event| {
                // A window that is closing has no one left to ask; the call
                // then waits until the app ends.
                let _ = thread.queue(move |qobject| deliver(qobject, event));
            });
            let folder = platform::app_folder();
            let worker = Worker::start(move || Connection::for_app(folder), notify);
            self.as_mut().rust_mut().worker = Some(worker);
        }
        self.into_ref()
            .get_ref()
            .worker
            .as_ref()
            .expect("started above")
    }

    fn status(mut self: Pin<&mut Self>) -> QVariant {
        let status = self
            .as_mut()
            .worker()
            .run(|connection| Ok(connection.status()));
        let status = status.unwrap_or_else(Status::Unusable);
        let fake = matches!(status, Status::Ready { fake: true });
        if self.fake != fake {
            self.as_mut().set_fake(fake);
        }
        status_object(&status)
    }

    fn logins(mut self: Pin<&mut Self>) -> QVariant {
        match self.as_mut().worker().run(bank_connection::logins) {
            Ok(rows) => logins_object(&rows),
            Err(error) => error_object(&bank_connection::error_text(&error)),
        }
    }

    fn look_up(mut self: Pin<&mut Self>, iban: &QString) -> QVariant {
        let iban = iban.to_string();
        match self
            .as_mut()
            .worker()
            .run(move |connection| bank_connection::look_up(connection, &iban))
        {
            Ok(lookup) => lookup_object(&lookup),
            Err(error) => error_object(&bank_connection::error_text(&error)),
        }
    }

    fn set_up(mut self: Pin<&mut Self>, form: &QVariant) -> QVariant {
        let fields = form
            .value::<QMap<QMapPair_QString_QVariant>>()
            .unwrap_or_default();
        let form = SetupForm {
            iban: field(&fields, "iban"),
            url: field(&fields, "url"),
            login_name: field(&fields, "loginName"),
            customer_id: field(&fields, "customerId"),
            pin: kassensturz_bank::Pin::new(field(&fields, "pin")),
            pin_mode: bank_connection::pin_mode(&field(&fields, "pinMode")),
        };
        let rule = field(&fields, "customerIdRule");
        let rule = Lookup {
            customer_id: if rule == "required" {
                "required"
            } else {
                "optional"
            },
            ..empty_lookup()
        };
        if let Err(error) = bank_connection::check_form(&form, &rule) {
            return error_object(&error);
        }
        let started = self.as_mut().worker().spawn(move |connection, notify| {
            let result = bank_connection::set_up(connection, form);
            notify(Event::SetupDone(result));
        });
        match started {
            Ok(()) => {
                self.as_mut().set_busy(true);
                map([])
            }
            Err(error) => error_object(&bank_connection::error_text(&error)),
        }
    }

    fn answer_tan(&self, tan: &QString) {
        if let Some(worker) = &self.worker {
            worker.answer(Answer::Tan(Some(Tan::new(
                tan.to_string().trim().to_owned(),
            ))));
        }
    }

    fn cancel_tan(&self) {
        if let Some(worker) = &self.worker {
            worker.answer(Answer::Tan(None));
        }
    }

    fn answer_question(&self, index: i32) {
        if let Some(worker) = &self.worker {
            worker.answer(Answer::Button(usize::try_from(index).ok()));
        }
    }

    fn fetch_accounts(&self) -> QVariant {
        match session::with_book(|book| {
            bank_connection::fetch_rows(book, &app_locale())
                .map_err(|error| crate::errors::report_text(book, &error.report(), &app_locale()))
        }) {
            Some(Ok(rows)) => map([("accounts", list(rows.iter().map(fetch_row_object)))]),
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn bank_account_choices(mut self: Pin<&mut Self>, account_id: &QString) -> QVariant {
        let listed = match self
            .as_mut()
            .worker()
            .run(bank_connection::bank_account_choices)
        {
            Ok(listed) => listed,
            Err(error) => return error_object(&bank_connection::error_text(&error)),
        };
        let account_id = account_id.to_string();
        match session::with_book(|book| {
            let account = crate::transaction_entry::find_account(book, &account_id)
                .map_err(|_| ErrorText::plain("input.no-such-entry"))?;
            bank_connection::linkable(book, account.id, &listed)
                .map(|(free, suggested)| bank_account_choices_object(&free, &suggested))
                .map_err(|error| crate::errors::report_text(book, &error.report(), &app_locale()))
        }) {
            Some(Ok(choices)) => choices,
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn link_account(mut self: Pin<&mut Self>, account_id: &QString, key: &QString) -> QVariant {
        let listed = match self
            .as_mut()
            .worker()
            .run(bank_connection::bank_account_choices)
        {
            Ok(listed) => listed,
            Err(error) => return error_object(&bank_connection::error_text(&error)),
        };
        let (account_id, key) = (account_id.to_string(), key.to_string());
        let Some(chosen) = listed.iter().find(|entry| entry.key == key) else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        match session::with_book_mut(|book| {
            let account = crate::transaction_entry::find_account(book, &account_id)
                .map_err(|_| ErrorText::plain("input.no-such-entry"))?;
            bank_connection::link_one(book, &chosen.found, account.id)
                .map_err(|error| crate::errors::report_text(book, &error.report(), &app_locale()))
        }) {
            Some(Ok(())) => map([]),
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn fetch(mut self: Pin<&mut Self>, accounts: &QVariant) -> QVariant {
        let ids = strings(accounts);
        let locale = app_locale();
        let requests = match session::with_book(|book| {
            bank_connection::fetch_requests(book, &ids, &locale)
        }) {
            Some(Ok(requests)) if !requests.is_empty() => requests,
            Some(Ok(_)) | None => return error_object(&ErrorText::plain("input.no-such-entry")),
            Some(Err(error)) => return error_object(&error),
        };
        let today = locale.today();
        let started = self.as_mut().worker().spawn(move |connection, notify| {
            let result = bank_connection::fetch(connection, requests, today);
            notify(Event::FetchDone(result));
        });
        match started {
            Ok(()) => {
                self.as_mut().set_busy(true);
                map([])
            }
            Err(error) => error_object(&bank_connection::error_text(&error)),
        }
    }

    fn answer_pin(&self, pin: &QString, mode: &QString) {
        if let Some(worker) = &self.worker {
            let pin = BankPin::new(pin.to_string());
            worker.answer(Answer::Pin(Some((
                pin,
                bank_connection::pin_mode(&mode.to_string()),
            ))));
        }
    }

    fn cancel_pin(&self) {
        if let Some(worker) = &self.worker {
            worker.answer(Answer::Pin(None));
        }
    }

    fn remove_login(mut self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        done_object(
            self.as_mut()
                .worker()
                .run(move |connection| bank_connection::remove(connection, &id)),
        )
    }

    fn forget_pin(mut self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        done_object(
            self.as_mut()
                .worker()
                .run(move |connection| bank_connection::forget_pin(connection, &id)),
        )
    }

    fn link_choices(&self) -> QVariant {
        let found = &self.found;
        match session::with_book(|book| bank_connection::link_choices(book, found)) {
            Some(Ok((accounts, suggested))) => link_choices_object(found, &accounts, &suggested),
            Some(Err(_)) | None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn link(&self, choices: &QVariant) -> QVariant {
        let choices = strings(choices);
        let found = &self.found;
        match session::with_book_mut(|book| {
            bank_connection::link(book, found, &choices).map_err(|error| {
                crate::errors::report_text(
                    book,
                    &error.report(),
                    &crate::number_format::app_locale(),
                )
            })
        }) {
            Some(Ok(linked)) => map([(
                "linked",
                QVariant::from(&i32::try_from(linked).unwrap_or(i32::MAX)),
            )]),
            Some(Err(error)) => error_object(&error),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }
}

/// Brings what the worker asks or reports to QML, on the GUI thread.
fn deliver(mut qobject: Pin<&mut qobject::BankConnection>, event: Event) {
    match event {
        Event::Tan(request) => qobject.tan_requested(tan_object(&request)),
        Event::Question(question) => qobject.question_asked(question_object(&question)),
        Event::Pin(request) => qobject.pin_requested(pin_object(&request)),
        Event::FetchStarted(account) => qobject.fetch_started(QString::from(account.as_str())),
        Event::FetchDone(result) => {
            qobject.as_mut().set_busy(false);
            qobject.fetch_finished(fetch_object(&result));
        }
        Event::SetupDone(result) => {
            qobject.as_mut().rust_mut().found = result
                .as_ref()
                .map(|done| done.accounts.clone())
                .unwrap_or_default();
            qobject.as_mut().set_busy(false);
            qobject.setup_finished(setup_object(&result));
        }
    }
}

/// A form's customer ID rule, with nothing else known.
fn empty_lookup() -> Lookup {
    Lookup {
        state: "ready",
        iban: String::new(),
        country: String::new(),
        bank_code: String::new(),
        bank_name: String::new(),
        url: String::new(),
        ask_url: false,
        customer_id: "optional",
    }
}

fn status_object(status: &Status) -> QVariant {
    match status {
        Status::Ready { fake } => map([
            ("ready", QVariant::from(&true)),
            ("fake", QVariant::from(fake)),
        ]),
        Status::Unusable(error) => {
            let error = bank_connection::error_text(error);
            map([
                ("ready", QVariant::from(&false)),
                ("code", text(error.code)),
            ]
            .into_iter()
            .chain(
                error
                    .params
                    .iter()
                    .map(|(name, value)| (*name, text(value))),
            ))
        }
    }
}

fn logins_object(rows: &[LoginRow]) -> QVariant {
    map([(
        "logins",
        list(rows.iter().map(|row| {
            map([
                ("id", text(&row.id)),
                ("bank", text(&row.bank)),
                ("bankCode", text(&row.bank_code)),
                ("kind", text(row.kind)),
            ])
        })),
    )])
}

fn lookup_object(lookup: &Lookup) -> QVariant {
    map([
        ("state", text(lookup.state)),
        ("iban", text(&lookup.iban)),
        ("country", text(&lookup.country)),
        ("bankCode", text(&lookup.bank_code)),
        ("bankName", text(&lookup.bank_name)),
        ("url", text(&lookup.url)),
        ("askUrl", QVariant::from(&lookup.ask_url)),
        ("customerId", text(lookup.customer_id)),
    ])
}

fn tan_object(request: &TanRequest) -> QVariant {
    let length = |value: usize| QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX));
    map([
        ("title", text(&request.title)),
        ("challenge", text(&request.challenge)),
        ("minLength", length(request.min_len)),
        ("maxLength", length(request.max_len)),
    ])
}

fn question_object(question: &BankQuestion) -> QVariant {
    map([
        ("level", text(bank_connection::level_key(question.level))),
        ("title", text(&question.title)),
        ("text", text(&question.text)),
        (
            "buttons",
            list(question.buttons.iter().map(|button| text(button))),
        ),
    ])
}

fn found_object(account: &FoundAccount, suggested: Option<&String>) -> QVariant {
    map([
        ("iban", text(&account.iban)),
        ("number", text(&account.number)),
        ("name", text(&account.name)),
        ("kind", text(account.kind)),
        ("currency", text(&account.currency)),
        ("suggested", text(suggested.map_or("", String::as_str))),
    ])
}

fn setup_object(result: &Result<SetupDone, ErrorText>) -> QVariant {
    match result {
        Ok(done) => map([
            ("login", text(&done.login)),
            (
                "accounts",
                list(
                    done.accounts
                        .iter()
                        .map(|account| found_object(account, None)),
                ),
            ),
        ]),
        Err(error) => error_object(error),
    }
}

fn link_choices_object(
    found: &[FoundAccount],
    accounts: &[bank_connection::BookAccount],
    suggested: &[String],
) -> QVariant {
    map([
        (
            "accounts",
            list(
                accounts
                    .iter()
                    .map(|account| map([("id", text(&account.id)), ("name", text(&account.name))])),
            ),
        ),
        (
            "found",
            list(
                found
                    .iter()
                    .enumerate()
                    .map(|(index, account)| found_object(account, suggested.get(index))),
            ),
        ),
    ])
}

/// The texts of a list of text values from QML.
fn strings(value: &QVariant) -> Vec<String> {
    value
        .value::<QList<QVariant>>()
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .value::<QString>()
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn pin_object(request: &PinRequest) -> QVariant {
    map([
        ("bank", text(&request.bank)),
        ("rejected", QVariant::from(&request.rejected)),
    ])
}

fn fetch_row_object(row: &FetchRow) -> QVariant {
    map([
        ("id", text(&row.id)),
        ("name", text(&row.name)),
        ("linked", QVariant::from(&row.linked)),
        ("bankAccount", text(&row.bank_account)),
        ("lastFetched", text(&row.last_fetched)),
    ])
}

fn bank_account_choices_object(free: &[&ListedBankAccount], suggested: &str) -> QVariant {
    map([
        (
            "choices",
            list(free.iter().map(|entry| {
                map([
                    ("key", text(&entry.key)),
                    ("bank", text(&entry.bank)),
                    ("iban", text(&entry.found.iban)),
                    ("number", text(&entry.found.number)),
                    ("name", text(&entry.found.name)),
                    ("kind", text(entry.found.kind)),
                ])
            })),
        ),
        ("suggested", text(suggested)),
    ])
}

fn outcome_object(outcome: &FetchOutcome) -> QVariant {
    map([
        ("account", text(&outcome.account)),
        ("name", text(&outcome.name)),
        ("state", text(outcome.state)),
        (
            "transactions",
            QVariant::from(&i32::try_from(outcome.transactions).unwrap_or(i32::MAX)),
        ),
        (
            "error",
            outcome.error.as_ref().map_or_else(|| map([]), error_object),
        ),
    ])
}

fn fetch_object(result: &Result<Vec<FetchOutcome>, ErrorText>) -> QVariant {
    match result {
        Ok(outcomes) => map([("outcomes", list(outcomes.iter().map(outcome_object)))]),
        Err(error) => error_object(error),
    }
}

/// An empty object when `done` worked, else the error object.
fn done_object(done: Result<(), BankError>) -> QVariant {
    match done {
        Ok(()) => map([]),
        Err(error) => error_object(&bank_connection::error_text(&error)),
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_bank::{
        BankAccount, BankAccountId, BankAccountKind, BankCode, Login, LoginId, LoginKind,
        QuestionLevel,
    };

    use super::*;

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

    fn flag_of(value: &QVariant, key: &str) -> bool {
        object(value)
            .get(&QString::from(key))
            .and_then(|value| value.value::<bool>())
            .unwrap_or_else(|| panic!("no flag `{key}`"))
    }

    fn list_of(value: &QVariant, key: &str) -> Vec<QVariant> {
        object(value)
            .get(&QString::from(key))
            .and_then(|value| value.value::<QList<QVariant>>())
            .map(|list| list.iter().cloned().collect())
            .unwrap_or_else(|| panic!("no list `{key}`"))
    }

    fn found() -> FoundAccount {
        FoundAccount {
            iban: "DE89 3704 0044 0532 0130 00".to_owned(),
            number: "0532013000".to_owned(),
            name: "Current account".to_owned(),
            kind: "current",
            currency: "EUR".to_owned(),
            bank_code: "37040044".to_owned(),
        }
    }

    #[test]
    fn the_status_and_the_logins_read_back_as_qml_reads_them() {
        let ready = status_object(&Status::Ready { fake: true });
        assert!(flag_of(&ready, "ready"));
        assert!(flag_of(&ready, "fake"));
        let unusable = status_object(&Status::Unusable(BankError::NotRegistered));
        assert!(!flag_of(&unusable, "ready"));
        assert_eq!(text_of(&unusable, "code"), "bank.not-registered");
        let logins = logins_object(&[LoginRow {
            id: "7".to_owned(),
            bank: "Testbank".to_owned(),
            bank_code: "37040044".to_owned(),
            kind: "pin-tan",
        }]);
        let rows = list_of(&logins, "logins");
        assert_eq!(rows.len(), 1);
        for (key, expected) in [
            ("id", "7"),
            ("bank", "Testbank"),
            ("bankCode", "37040044"),
            ("kind", "pin-tan"),
        ] {
            assert_eq!(text_of(&rows[0], key), expected, "{key}");
        }
        assert_eq!(
            text_of(&done_object(Err(BankError::Busy)), "code"),
            "bank.busy"
        );
        assert!(object(&done_object(Ok(()))).is_empty());
    }

    #[test]
    fn a_lookup_reads_back_as_qml_reads_it() {
        let lookup = lookup_object(&Lookup {
            state: "no-server",
            iban: "DE89 3704 0044 0532 0130 00".to_owned(),
            country: "DE".to_owned(),
            bank_code: "37040044".to_owned(),
            bank_name: "Testbank".to_owned(),
            url: String::new(),
            ask_url: true,
            customer_id: "required",
        });
        for (key, expected) in [
            ("state", "no-server"),
            ("iban", "DE89 3704 0044 0532 0130 00"),
            ("country", "DE"),
            ("bankCode", "37040044"),
            ("bankName", "Testbank"),
            ("url", ""),
            ("customerId", "required"),
        ] {
            assert_eq!(text_of(&lookup, key), expected, "{key}");
        }
        assert!(flag_of(&lookup, "askUrl"));
    }

    #[test]
    fn what_the_bank_asks_reads_back_as_qml_reads_it() {
        let tan = tan_object(&TanRequest {
            title: "Enter TAN".to_owned(),
            challenge: "Challenge 4711".to_owned(),
            min_len: 6,
            max_len: 64,
        });
        assert_eq!(text_of(&tan, "title"), "Enter TAN");
        assert_eq!(text_of(&tan, "challenge"), "Challenge 4711");
        let number = |key: &str| {
            object(&tan)
                .get(&QString::from(key))
                .and_then(|value| value.value::<i32>())
        };
        assert_eq!(number("minLength"), Some(6));
        assert_eq!(number("maxLength"), Some(64));
        let question = question_object(&BankQuestion {
            level: QuestionLevel::Info,
            title: "Approval".to_owned(),
            text: "Approve in your app".to_owned(),
            buttons: vec!["Approved".to_owned(), "Abort".to_owned()],
        });
        assert_eq!(text_of(&question, "level"), "info");
        assert_eq!(text_of(&question, "title"), "Approval");
        assert_eq!(text_of(&question, "text"), "Approve in your app");
        let buttons: Vec<String> = list_of(&question, "buttons")
            .iter()
            .map(|button| button.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(buttons, vec!["Approved", "Abort"]);
    }

    #[test]
    fn a_setup_and_the_link_choices_read_back_as_qml_reads_them() {
        let done = setup_object(&Ok(SetupDone {
            login: "3".to_owned(),
            accounts: vec![found()],
        }));
        assert_eq!(text_of(&done, "login"), "3");
        let accounts = list_of(&done, "accounts");
        for (key, expected) in [
            ("iban", "DE89 3704 0044 0532 0130 00"),
            ("number", "0532013000"),
            ("name", "Current account"),
            ("kind", "current"),
            ("currency", "EUR"),
            ("suggested", ""),
        ] {
            assert_eq!(text_of(&accounts[0], key), expected, "{key}");
        }
        let failed = setup_object(&Err(ErrorText::plain("bank.aborted")));
        assert_eq!(text_of(&failed, "code"), "bank.aborted");
        let choices = link_choices_object(
            &[found()],
            &[bank_connection::BookAccount {
                id: "a1".to_owned(),
                name: "Giro".to_owned(),
            }],
            &["a1".to_owned()],
        );
        let accounts = list_of(&choices, "accounts");
        assert_eq!(text_of(&accounts[0], "id"), "a1");
        assert_eq!(text_of(&accounts[0], "name"), "Giro");
        let found = list_of(&choices, "found");
        assert_eq!(text_of(&found[0], "suggested"), "a1");
        assert_eq!(text_of(&found[0], "iban"), "DE89 3704 0044 0532 0130 00");
    }

    #[test]
    fn the_fetch_screen_reads_back_as_qml_reads_it() {
        // The PIN dialog never gets more than the bank's name and whether
        // the last PIN was wrong.
        let pin = pin_object(&PinRequest {
            bank: "Testbank".to_owned(),
            rejected: true,
        });
        assert_eq!(text_of(&pin, "bank"), "Testbank");
        assert!(flag_of(&pin, "rejected"));
        assert_eq!(object(&pin).len(), 2);

        let row = fetch_row_object(&FetchRow {
            id: "a1".to_owned(),
            name: "Current account".to_owned(),
            linked: true,
            bank_account: "DE89 3704 0044 0532 0130 00".to_owned(),
            last_fetched: "2026-10-09".to_owned(),
        });
        for (key, expected) in [
            ("id", "a1"),
            ("name", "Current account"),
            ("bankAccount", "DE89 3704 0044 0532 0130 00"),
            ("lastFetched", "2026-10-09"),
        ] {
            assert_eq!(text_of(&row, key), expected, "{key}");
        }
        assert!(flag_of(&row, "linked"));

        let listed = ListedBankAccount {
            key: "7\n9".to_owned(),
            bank: "Testbank".to_owned(),
            login: Login {
                id: LoginId::new("7"),
                bank: BankCode::new("DE", "37040044"),
                kind: LoginKind::ChipCard,
            },
            account: BankAccount {
                id: BankAccountId::new("9"),
                login: LoginId::new("7"),
                kind: BankAccountKind::Current,
                iban: None,
                number: None,
                currency: None,
                name: None,
            },
            found: found(),
        };
        let choices = bank_account_choices_object(&[&listed], "7\n9");
        assert_eq!(text_of(&choices, "suggested"), "7\n9");
        let entries = list_of(&choices, "choices");
        assert_eq!(entries.len(), 1);
        for (key, expected) in [
            ("key", "7\n9"),
            ("bank", "Testbank"),
            ("iban", "DE89 3704 0044 0532 0130 00"),
            ("number", "0532013000"),
            ("name", "Current account"),
            ("kind", "current"),
        ] {
            assert_eq!(text_of(&entries[0], key), expected, "{key}");
        }
    }

    #[test]
    fn a_fetchs_result_reads_back_as_qml_reads_it() {
        let result = fetch_object(&Ok(vec![
            FetchOutcome {
                account: "a1".to_owned(),
                name: "Current account".to_owned(),
                state: "ready",
                transactions: 3,
                error: None,
            },
            FetchOutcome {
                account: "a2".to_owned(),
                name: "Card".to_owned(),
                state: "failed",
                transactions: 0,
                error: Some(ErrorText {
                    code: "bank.refused",
                    params: vec![("text", "made-up message".to_owned())],
                }),
            },
            FetchOutcome {
                account: "a3".to_owned(),
                name: "Savings".to_owned(),
                state: "skipped",
                transactions: 0,
                error: None,
            },
        ]));
        let outcomes = list_of(&result, "outcomes");
        assert_eq!(outcomes.len(), 3);
        let number = |value: &QVariant| {
            object(value)
                .get(&QString::from("transactions"))
                .and_then(|value| value.value::<i32>())
        };
        assert_eq!(text_of(&outcomes[0], "account"), "a1");
        assert_eq!(text_of(&outcomes[0], "name"), "Current account");
        assert_eq!(text_of(&outcomes[0], "state"), "ready");
        assert_eq!(number(&outcomes[0]), Some(3));
        // An error is an object QML can hand to FormError as it is; without
        // one the object is empty, so `error.code` is undefined.
        let error = object(&outcomes[1]).get(&QString::from("error"));
        let error = error.expect("an error object");
        assert_eq!(text_of(&error, "code"), "bank.refused");
        assert_eq!(text_of(&error, "text"), "made-up message");
        let none = object(&outcomes[0]).get(&QString::from("error"));
        assert!(object(&none.expect("an object")).is_empty());
        assert_eq!(text_of(&outcomes[2], "state"), "skipped");

        let refused = fetch_object(&Err(ErrorText::plain("bank.not-registered")));
        assert_eq!(text_of(&refused, "code"), "bank.not-registered");
    }
}
