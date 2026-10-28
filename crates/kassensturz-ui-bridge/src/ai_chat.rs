// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the chat panel and the change-set review do (M5.8, D-210 to D-213),
//! apart from Qt.
//!
//! A question runs on a thread of its own, since a provider can take
//! seconds, on a second, read-only connection to the open book (D-195): the
//! window stays usable and the book keeps its single writer. The thread
//! reaches the window only through [`Screen`] (the tool-call log, the
//! request waiting for the user's look) and gets the user's answers through
//! [`Command`]s. The change sets the model prepares are kept here, on the
//! window's side, until the user confirms or discards them; [`confirm_in`]
//! is the only place that makes a [`Confirmed`] (D-171, D-200), and it is
//! called only by the confirm button's invokable. This module is compiled
//! only in a build with AI (D-039).

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};

use kassensturz_ai::PreparedRequest;
use kassensturz_ai_chat::{
    Answer, Cancel, ChatError, ChatSession, DemoModel, Hooks, LogEntry, demo_provider,
};
use kassensturz_ai_tools::requests::RequestId;
use kassensturz_ai_tools::{ChangeError, ChangeSet, Confirmed, ProposedChange, ToolError};
use kassensturz_core::ai_switch::Permissions;
use kassensturz_core::{Date, ErrorParam, ErrorReport, MatchMode, Rule};
use kassensturz_platform::Secret;
use kassensturz_store::{Book, StoreError};

use crate::ai_settings::{Store, with_keys};
use crate::errors::ErrorText;
use crate::format::Locale;
use crate::register::AmountWriter;
use crate::rules::{ConditionForm, condition_form};

/// What the user decides about a request shown to them (D-210).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Send the request as it is on the screen now.
    Send,
    /// Don't send it, which ends the question (`ai-chat.stopped`).
    Stop,
    /// Leave this word or phrase out of the request, and show it again.
    LeaveOut(String),
}

/// A request waiting for the user's look.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    /// Where it goes, as the user wrote the provider's address.
    pub address: String,
    /// The model it is for.
    pub model: String,
    /// The exact body that will be sent (T-22).
    pub body: String,
    /// In how many places the phrase just left out was found; 0 when the
    /// request is shown for the first time.
    pub left_out: usize,
}

/// What a question running on its thread tells the window. The window's
/// implementation queues each call to the window's thread.
pub trait Screen: Send {
    /// A new line of the tool-call log (D-192).
    fn logged(&self, entry: LogEntry);
    /// A request waits for the user: the window shows it and answers with
    /// a [`Command`].
    fn reviewing(&self, review: Review);
}

/// The buttons of a question under way, kept by the window.
#[derive(Debug)]
pub struct Controls {
    cancel: Cancel,
    commands: Sender<Command>,
}

impl Controls {
    /// Answers the request that waits for a look.
    pub fn command(&self, command: Command) {
        // A question that ended has dropped its receiver: nothing to tell.
        let _ = self.commands.send(command);
    }

    /// Stops the question, also when it waits for the user's look.
    pub fn stop(&self) {
        self.cancel.cancel();
        self.command(Command::Stop);
    }
}

/// The hooks of a question: they show each request first when
/// `ask_first`, report the log, and stop at the Stop button.
pub struct WindowHooks {
    screen: Box<dyn Screen>,
    cancel: Cancel,
    commands: Receiver<Command>,
    ask_first: bool,
}

/// The hooks for a question, and the buttons that control them.
pub fn hooks(screen: Box<dyn Screen>, ask_first: bool) -> (Controls, WindowHooks) {
    let (sender, commands) = channel();
    let cancel = Cancel::new();
    (
        Controls {
            cancel: cancel.clone(),
            commands: sender,
        },
        WindowHooks {
            screen,
            cancel,
            commands,
            ask_first,
        },
    )
}

impl Hooks for WindowHooks {
    fn review(&mut self, request: &mut PreparedRequest) -> bool {
        if !self.ask_first {
            return true;
        }
        let mut left_out = 0;
        loop {
            self.screen.reviewing(Review {
                address: request.address().as_str().to_owned(),
                model: request.model().to_owned(),
                body: request.preview().to_owned(),
                left_out,
            });
            // A closed window or a question given up on drops the sender.
            match self.commands.recv() {
                Ok(Command::Send) => return !self.cancel.is_cancelled(),
                Ok(Command::LeaveOut(phrase)) => {
                    left_out = request.redact(&phrase).unwrap_or(0);
                }
                Ok(Command::Stop) | Err(_) => return false,
            }
        }
    }

    fn logged(&mut self, entry: &LogEntry) {
        self.screen.logged(entry.clone());
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

/// Asks `question` about the book in the file at `book_path`, which is
/// opened for reading only. The provider is the saved one, or the fake
/// model when `demo` is set (D-193). `typed` is the key the user typed for
/// this question; without one the kept key is used, if the provider needs
/// one.
#[allow(clippy::too_many_arguments)]
pub fn answer_from(
    book_path: &Path,
    session: &mut ChatSession,
    store: &Store,
    demo: Option<DemoModel>,
    typed: Option<Secret>,
    question: &str,
    today: Date,
    hooks: &mut dyn Hooks,
) -> Result<Answer, ChatError> {
    let book = Book::open_to_read(book_path).map_err(|e| ChatError::Book(ToolError::Store(e)))?;
    answer(&book, session, store, demo, typed, question, today, hooks)
}

/// As [`answer_from`], for a book already open.
#[allow(clippy::too_many_arguments)]
pub fn answer(
    book: &Book,
    session: &mut ChatSession,
    store: &Store,
    demo: Option<DemoModel>,
    typed: Option<Secret>,
    question: &str,
    today: Date,
    hooks: &mut dyn Hooks,
) -> Result<Answer, ChatError> {
    // The one holder of keys (M5.7), so a key remembered until the app
    // closes is the key the chat uses.
    let key = with_keys(|keys| store.key_for(keys, typed))?;
    match demo {
        Some(model) => session.ask(
            book,
            &demo_provider(store.config().clone(), model),
            key.as_ref(),
            question,
            today,
            hooks,
        ),
        None => session.ask(book, &store.client(), key.as_ref(), question, today, hooks),
    }
}

/// An error report as the screens get it: its code and its parameters as
/// text.
pub fn code_text(report: &ErrorReport) -> ErrorText {
    ErrorText {
        code: report.code,
        params: report
            .params
            .iter()
            .map(|(name, param)| {
                let text = match param {
                    ErrorParam::Number(number) => number.to_string(),
                    ErrorParam::Text(text) => text.clone(),
                    other => format!("{other:?}"),
                };
                (*name, text)
            })
            .collect(),
    }
}

/// The change sets a model prepared that the user has not yet confirmed or
/// discarded, by the ID the screens use for them: the chat's, and those the
/// user's own AI tools sent (M5.10, D-218).
#[derive(Debug, Default)]
pub struct Sets {
    sets: HashMap<u64, Waiting>,
    next: u64,
}

/// A set that waits for the user, and the request it came with, if one of
/// the user's own AI tools sent it.
#[derive(Debug)]
pub struct Waiting {
    /// The set.
    pub set: ChangeSet,
    /// The request to answer once the user has decided.
    pub from: Option<RequestId>,
}

impl Sets {
    /// Keeps `set` from the chat for review; returns its ID.
    pub fn add(&mut self, set: ChangeSet) -> u64 {
        self.keep(set, None)
    }

    /// Keeps `set` from the request `from` for review; returns its ID.
    pub fn add_requested(&mut self, set: ChangeSet, from: RequestId) -> u64 {
        self.keep(set, Some(from))
    }

    fn keep(&mut self, set: ChangeSet, from: Option<RequestId>) -> u64 {
        self.next += 1;
        self.sets.insert(self.next, Waiting { set, from });
        self.next
    }

    /// The set with this ID, if it is still waiting.
    pub fn get(&self, id: u64) -> Option<&ChangeSet> {
        self.sets.get(&id).map(|waiting| &waiting.set)
    }

    /// Takes the set out, to apply it or throw it away.
    pub fn take(&mut self, id: u64) -> Option<Waiting> {
        self.sets.remove(&id)
    }

    /// Throws every waiting set away; returns the requests they came with,
    /// which still need an answer.
    pub fn clear(&mut self) -> Vec<RequestId> {
        self.sets
            .drain()
            .filter_map(|(_, waiting)| waiting.from)
            .collect()
    }

    /// How many sets wait.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Whether none waits.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }
}

/// One change of a set as the review screen shows it. Every text in it came
/// from the model or the book: shown as plain text (T-08, T-34).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeView {
    /// A new category.
    NewCategory {
        /// Its name.
        name: String,
        /// Where it goes: the names above it and its own, e.g. "Food › Bakery".
        path: String,
        /// Whether it is for money coming in.
        income: bool,
    },
    /// A new Rule.
    NewRule {
        /// Its name.
        name: String,
        /// The category it books to, as a path.
        book_to: String,
        /// Whether all conditions must hold.
        match_all: bool,
        /// Its conditions, as the Rule editor holds them.
        conditions: Vec<ConditionForm>,
    },
    /// A category renamed, moved or both.
    EditCategory {
        /// Its path before, e.g. "Food › Bakery".
        before: String,
        /// Its path after.
        after: String,
    },
    /// An existing Rule changed.
    EditRule {
        /// Its name before.
        before_name: String,
        /// Its name after, the same when it isn't renamed.
        name: String,
        /// The category it booked to before, as a path, empty for none.
        before_book_to: String,
        /// The category it books to after, as a path, empty for none.
        book_to: String,
        /// Whether all conditions must hold after the change.
        match_all: bool,
        /// Its conditions after the change.
        conditions: Vec<ConditionForm>,
    },
    /// A top-level Rule moved.
    MoveRule {
        /// Its name.
        name: String,
        /// Its place before, counted from 1, as text.
        from: String,
        /// Its place after, as text.
        to: String,
    },
    /// A Rule deleted.
    DeleteRule {
        /// Its name.
        name: String,
        /// The category it books to, as a path, empty for none.
        book_to: String,
        /// Whether all conditions must hold.
        match_all: bool,
        /// Its conditions.
        conditions: Vec<ConditionForm>,
    },
    /// A transaction that gets a category.
    Categorize {
        /// Its day, in the user's format.
        date: String,
        /// Who was paid or paid, empty for none.
        payee: String,
        /// The account the money moved in or out of.
        account: String,
        /// What it did to the account, exact, in the user's format (D-065).
        amount: String,
        /// Its category now, empty for none.
        before: String,
        /// Its category after the change.
        after: String,
    },
    /// An empty category deleted.
    DeleteCategory {
        /// Its path, e.g. "Food › Bakery".
        path: String,
    },
    /// A new account.
    NewAccount {
        /// Its name.
        name: String,
        /// Its type, as `AccountTypes.qml` names it.
        account_type: String,
        /// Its currency code.
        currency: String,
    },
    /// An account renamed, closed or opened again.
    EditAccount {
        /// Its name before.
        before: String,
        /// Its name after, the same when it isn't renamed.
        after: String,
        /// Whether it was closed before.
        was_closed: bool,
        /// Whether it is closed after.
        closed: bool,
    },
}

/// The changes of `set` as the review screen shows them, with amounts and
/// dates written for the user (D-065). Reads `book` only.
pub fn view_of(
    book: &Book,
    set: &ChangeSet,
    locale: &impl Locale,
) -> Result<Vec<ChangeView>, StoreError> {
    set.changes()
        .iter()
        .map(|change| {
            Ok(match change {
                ProposedChange::NewCategory(new) => ChangeView::NewCategory {
                    name: new.path.last().cloned().unwrap_or_default(),
                    path: new.path.join(" › "),
                    income: new.income,
                },
                ProposedChange::NewRule(new) => {
                    new_rule_view(book, &new.rule, &new.category, locale)?
                }
                ProposedChange::EditCategory(edit) => ChangeView::EditCategory {
                    before: edit.before.join(" › "),
                    after: edit.after.join(" › "),
                },
                ProposedChange::EditRule(edit) => ChangeView::EditRule {
                    before_name: edit.before.name.clone(),
                    name: edit.rule.name.clone(),
                    before_book_to: edit
                        .before_category
                        .as_ref()
                        .map(|path| path.join(" › "))
                        .unwrap_or_default(),
                    book_to: edit
                        .category
                        .as_ref()
                        .map(|path| path.join(" › "))
                        .unwrap_or_default(),
                    match_all: edit.rule.match_mode == MatchMode::All,
                    conditions: condition_forms(book, &edit.rule, locale)?,
                },
                ProposedChange::MoveRule(moved) => ChangeView::MoveRule {
                    name: moved.name.clone(),
                    from: moved.from.to_string(),
                    to: moved.to.to_string(),
                },
                ProposedChange::DeleteRule(delete) => ChangeView::DeleteRule {
                    name: delete.rule.name.clone(),
                    book_to: delete
                        .category
                        .as_ref()
                        .map(|path| path.join(" › "))
                        .unwrap_or_default(),
                    match_all: delete.rule.match_mode == MatchMode::All,
                    conditions: condition_forms(book, &delete.rule, locale)?,
                },
                ProposedChange::Categorize(change) => ChangeView::Categorize {
                    date: locale.short_date(change.date),
                    payee: change.payee.clone().unwrap_or_default(),
                    account: change.account.clone(),
                    amount: AmountWriter::new(book, change.amount.currency(), locale)?
                        .text(change.amount)
                        .exact,
                    before: change
                        .before
                        .as_ref()
                        .map(|path| path.join(" › "))
                        .unwrap_or_default(),
                    after: change.after.join(" › "),
                },
                ProposedChange::DeleteCategory(delete) => ChangeView::DeleteCategory {
                    path: delete.path.join(" › "),
                },
                ProposedChange::NewAccount(new) => ChangeView::NewAccount {
                    name: new.name.clone(),
                    account_type: crate::accounts::type_key(new.account_type).to_owned(),
                    currency: new.currency.as_str().to_owned(),
                },
                ProposedChange::EditAccount(edit) => ChangeView::EditAccount {
                    before: edit.before.clone(),
                    after: edit.after.clone(),
                    was_closed: edit.was_closed,
                    closed: edit.closed,
                },
            })
        })
        .collect()
}

fn new_rule_view(
    book: &Book,
    rule: &Rule,
    category: &[String],
    locale: &impl Locale,
) -> Result<ChangeView, StoreError> {
    Ok(ChangeView::NewRule {
        name: rule.name.clone(),
        book_to: category.join(" › "),
        match_all: rule.match_mode == MatchMode::All,
        conditions: condition_forms(book, rule, locale)?,
    })
}

/// The conditions of `rule` as the Rule editor holds them.
fn condition_forms(
    book: &Book,
    rule: &Rule,
    locale: &impl Locale,
) -> Result<Vec<ConditionForm>, StoreError> {
    rule.conditions
        .iter()
        .map(|condition| condition_form(book, condition, locale))
        .collect()
}

/// Applies `set` to `book` as the user's own action: the one place in the
/// app that makes a [`Confirmed`]. Only the confirm button of the review
/// screen reaches it (D-171, D-200; M5.12 checks that no other code does).
pub fn confirm_in(
    book: &mut Book,
    set: ChangeSet,
    allowed: Permissions,
) -> Result<(), ChangeError> {
    // The user may have turned a kind of change off since the set was
    // prepared (D-239).
    set.check_allowed(allowed)?;
    set.apply(book, Confirmed::by_the_user())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use kassensturz_ai::{KeyUse, ProviderSettings};
    use kassensturz_ai_chat::{BuildInfo, DEMO_CATEGORY, Mode};
    use kassensturz_core::Report;
    use kassensturz_store::Action;

    use super::*;
    use crate::register::tests::TestLocale;
    use crate::test_folder::TestFolder;

    const TODAY: Date = Date::constant(2026, 10, 25);

    /// Records what the question tells the window.
    #[derive(Clone, Default)]
    struct Recorder {
        log: Arc<Mutex<Vec<LogEntry>>>,
        reviews: Arc<Mutex<Vec<Review>>>,
    }

    impl Screen for Recorder {
        fn logged(&self, entry: LogEntry) {
            self.log.lock().unwrap().push(entry);
        }
        fn reviewing(&self, review: Review) {
            self.reviews.lock().unwrap().push(review);
        }
    }

    /// A settings file with AI on and the fake model's address.
    fn store(folder: &TestFolder) -> Store {
        let path = folder.join("kassensturzrc");
        let provider = ProviderSettings::new(
            kassensturz_ai::SCRIPTED_ADDRESS,
            kassensturz_ai::SCRIPTED_MODEL,
            KeyUse::None,
        )
        .unwrap();
        let mut text = String::from("[AI]\nEnabled=true\n");
        for (key, value) in provider.entries() {
            text.push_str(&format!("{key}={value}\n"));
        }
        std::fs::write(&path, text).unwrap();
        Store::at(&path)
    }

    /// The synthetic Space Colony book as the importer, the pipeline and
    /// the store fill it (AGENTS.md §9.4).
    fn space_colony() -> Book {
        use kassensturz_import::{Imported, KmySqlite, Limits, import};
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kassensturz-import/tests/golden/kmy-sqlite/space-colony-v15.sqlite");
        let bytes = std::fs::read(path).expect("the Space Colony book is in the repository");
        let Imported::Book(imported) =
            import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
        else {
            panic!("a book");
        };
        let mut book = Book::create_in_memory().expect("a book in memory");
        kassensturz_pipeline::fill_book_from(&mut book, &imported, &HashMap::new())
            .expect("filled");
        book
    }

    fn ask(
        book: &Book,
        store: &Store,
        chat: &mut ChatSession,
        question: &str,
        hooks: &mut dyn Hooks,
    ) -> Result<Answer, ChatError> {
        answer(
            book,
            chat,
            store,
            Some(DemoModel::Functions),
            None,
            question,
            TODAY,
            hooks,
        )
    }

    fn session() -> ChatSession {
        ChatSession::new(BuildInfo::new("0.261025.0", "abc1234"))
            .with_permissions(Permissions::all())
    }

    #[test]
    fn a_question_the_user_does_not_review_goes_at_once_and_is_logged() {
        let folder = TestFolder::new();
        let store = store(&folder);
        let book = space_colony();
        let screen = Recorder::default();
        let (_controls, mut hooks) = hooks(Box::new(screen.clone()), false);
        let mut chat = session();

        let answer = ask(&book, &store, &mut chat, "List my accounts.", &mut hooks).unwrap();

        assert_eq!(answer.mode, Mode::Functions);
        assert!(answer.text.contains("Everyday Checking"), "{}", answer.text);
        assert!(screen.reviews.lock().unwrap().is_empty());
        let log = screen.log.lock().unwrap();
        // What the window got is what the session kept (D-192).
        assert_eq!(log.as_slice(), chat.log());
        assert!(matches!(log[0], LogEntry::Asked { .. }));
        assert!(
            log.iter()
                .any(|entry| matches!(entry, LogEntry::Called { function, .. } if function == "list_accounts"))
        );
    }

    #[test]
    fn a_reviewed_request_waits_for_the_user_and_leaves_out_what_they_say() {
        let folder = TestFolder::new();
        let store = store(&folder);
        let book = space_colony();
        let screen = Recorder::default();
        let (controls, mut hooks) = hooks(Box::new(screen.clone()), true);
        let mut chat = session();
        // The user answers before the question starts: leave a word out,
        // then send, for each of the two requests of the question.
        controls.command(Command::LeaveOut("Everyday".into()));
        controls.command(Command::Send);
        controls.command(Command::Send);

        let answer = ask(
            &book,
            &store,
            &mut chat,
            "List my accounts, Everyday.",
            &mut hooks,
        )
        .unwrap();

        let reviews = screen.reviews.lock().unwrap();
        assert_eq!(
            reviews.len(),
            3,
            "shown, shown again after leaving out, second request"
        );
        assert_eq!(reviews[0].left_out, 0);
        assert!(reviews[0].body.contains("Everyday"));
        assert_eq!(reviews[1].left_out, 1);
        assert!(!reviews[1].body.contains("Everyday"));
        assert_eq!(reviews[0].model, kassensturz_ai::SCRIPTED_MODEL);
        assert_eq!(reviews[0].address, kassensturz_ai::SCRIPTED_ADDRESS);
        // What the log says was sent is what the user saw last.
        let sent: Vec<&str> = chat
            .log()
            .iter()
            .filter_map(|entry| match entry {
                LogEntry::Sent { body, .. } => Some(body.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(sent[0], reviews[1].body);
        assert_eq!(sent[1], reviews[2].body);
        assert!(!answer.text.is_empty());
    }

    #[test]
    fn not_sending_a_reviewed_request_ends_the_question_and_sends_nothing() {
        let folder = TestFolder::new();
        let store = store(&folder);
        let book = space_colony();
        let screen = Recorder::default();
        let (controls, mut hooks) = hooks(Box::new(screen.clone()), true);
        let mut chat = session();
        controls.stop();

        let error = ask(&book, &store, &mut chat, "List my accounts.", &mut hooks).unwrap_err();

        assert!(matches!(error, ChatError::Cancelled), "{error:?}");
        assert!(
            !chat
                .log()
                .iter()
                .any(|entry| matches!(entry, LogEntry::Sent { .. }))
        );

        // Declining a request after looking at it is `stopped`.
        let (controls, mut hooks) = super::hooks(Box::new(Recorder::default()), true);
        controls.command(Command::Stop);
        let error = ask(&book, &store, &mut chat, "List my accounts.", &mut hooks).unwrap_err();
        assert!(matches!(error, ChatError::Stopped), "{error:?}");
    }

    #[test]
    fn a_sorting_question_prepares_a_set_that_confirm_applies_and_undo_takes_back() {
        let folder = TestFolder::new();
        let store = store(&folder);
        let mut book = space_colony();
        let screen = Recorder::default();
        let (_controls, mut hooks) = hooks(Box::new(screen), false);
        let mut chat = session();

        let mut answer = ask(
            &book,
            &store,
            &mut chat,
            "Please sort my transactions.",
            &mut hooks,
        )
        .unwrap();
        assert_eq!(answer.changes.len(), 1);
        let mut sets = Sets::default();
        let id = sets.add(answer.changes.remove(0));
        assert_eq!(sets.len(), 1);

        // The review shows the set with real figures from the real book.
        let views = view_of(&book, sets.get(id).unwrap(), &TestLocale).unwrap();
        assert!(views.iter().any(|view| matches!(
            view,
            ChangeView::NewCategory { name, income: false, .. } if name == DEMO_CATEGORY
        )));
        let categorized: Vec<_> = views
            .iter()
            .filter_map(|view| match view {
                ChangeView::Categorize {
                    date,
                    amount,
                    after,
                    account,
                    ..
                } => Some((date, amount, after, account)),
                _ => None,
            })
            .collect();
        assert!(!categorized.is_empty());
        for (date, amount, after, account) in &categorized {
            assert_eq!(date.len(), 10, "{date}");
            assert!(amount.ends_with(" €") && amount.contains(','), "{amount}");
            assert_eq!(after.as_str(), DEMO_CATEGORY);
            assert!(!account.is_empty());
        }
        let before = book.history().unwrap().len();

        let set = sets.take(id).unwrap().set;
        assert!(sets.is_empty());
        confirm_in(&mut book, set, Permissions::all()).unwrap();

        // One user action in the history, which one undo takes back.
        assert_eq!(book.history().unwrap().len(), before + 1);
        let applied = book.change_to_undo().unwrap().unwrap();
        assert_eq!(applied.action, Action::ApplyAiChanges);
        assert!(book.undo().unwrap().is_some());
        assert_eq!(
            book.change_to_undo().unwrap().map(|c| c.id),
            book.history().unwrap().get(1).map(|c| c.id)
        );
        // The category is gone again.
        assert!(
            !book
                .categories()
                .unwrap()
                .iter()
                .any(|entry| entry.path.last().is_some_and(|name| name == DEMO_CATEGORY))
        );
    }

    #[test]
    fn account_changes_are_shown_with_their_words_and_applied_by_confirm() {
        let mut book = space_colony();
        let currency = book.accounts(false).unwrap()[0].currency;
        let first = kassensturz_core::Account {
            id: kassensturz_core::AccountId::generate(),
            name: "Wallet".into(),
            kind: kassensturz_core::AccountKind::Asset,
            currency,
            account_type: Some(kassensturz_core::AccountType::Cash),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        };
        book.add_account(&first).unwrap();
        let call = serde_json::json!({
            "new_accounts": [{"name": "Holiday money", "type": "cash"}],
            "edit_accounts": [{"account_id": first.id.to_string(), "name": "Renamed by the AI", "closed": true}],
        });
        let proposal = kassensturz_ai_tools::propose(&book, &call, Permissions::all()).unwrap();
        let views = view_of(&book, &proposal.changes, &TestLocale).unwrap();
        assert!(views.iter().any(|view| matches!(
            view,
            ChangeView::NewAccount { name, account_type, currency }
                if name == "Holiday money" && account_type == "cash" && currency.len() == 3
        )));
        assert!(views.iter().any(|view| matches!(
            view,
            ChangeView::EditAccount { after, closed: true, was_closed: false, .. }
                if after == "Renamed by the AI"
        )));
        confirm_in(&mut book, proposal.changes, Permissions::all()).unwrap();
        assert!(book.account(first.id).unwrap().closed);
        assert!(book.undo().unwrap().is_some());
        assert!(!book.account(first.id).unwrap().closed);
    }

    #[test]
    fn a_set_for_a_book_that_changed_is_not_applied() {
        let folder = TestFolder::new();
        let store = store(&folder);
        let mut book = space_colony();
        let (_controls, mut hooks) = hooks(Box::new(Recorder::default()), false);
        let mut chat = session();
        let mut answer = ask(
            &book,
            &store,
            &mut chat,
            "Please sort my transactions.",
            &mut hooks,
        )
        .unwrap();
        let set = answer.changes.remove(0);
        // The user removes a categorized transaction meanwhile.
        let Some(ProposedChange::Categorize(change)) = set
            .changes()
            .iter()
            .find(|change| matches!(change, ProposedChange::Categorize(_)))
        else {
            panic!("a transaction in the set");
        };
        book.remove_transaction(change.transaction).unwrap();
        let history = book.history().unwrap().len();

        let error = confirm_in(&mut book, set, Permissions::all()).unwrap_err();

        assert!(matches!(error, ChangeError::OutOfDate), "{error:?}");
        assert_eq!(
            book.history().unwrap().len(),
            history,
            "nothing was applied"
        );
    }

    #[test]
    fn only_the_confirm_function_makes_a_confirmation() {
        // M5.12 holds the whole workspace to this; here the app's own
        // sources: one call, in `confirm_in` (D-171).
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for (number, line) in text.lines().enumerate() {
                    if line.contains("by_the_user(") && !line.trim_start().starts_with("//") {
                        found.push(format!(
                            "{}:{}",
                            path.file_name().unwrap().to_string_lossy(),
                            number + 1
                        ));
                    }
                }
            }
        }
        // The call, and this test's own search text.
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found.iter().all(|place| place.starts_with("ai_chat.rs")),
            "{found:?}"
        );
    }

    #[test]
    fn errors_reach_the_screens_as_codes_with_text_parameters() {
        let text = code_text(&ChatError::TooManyRounds.report());
        assert_eq!(text.code, "ai-chat.too-many-rounds");
        assert!(text.params.is_empty());
        let text = code_text(
            &ChatError::Ai(kassensturz_ai::AiError::ProviderFailed { status: 500 }).report(),
        );
        assert_eq!(text.code, "ai.provider-failed");
        assert_eq!(text.params, [("status", "500".to_owned())]);
    }
}
