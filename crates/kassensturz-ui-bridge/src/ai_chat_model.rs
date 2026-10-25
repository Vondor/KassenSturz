// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chat panel's and the change-set review's object (M5.8, D-210 to
//! D-213).
//!
//! Like the switch and the settings (`ai_switch_model.rs`,
//! `ai_settings_model.rs`) the object exists in every build, but in a build
//! without AI (D-039) it holds no AI code, is never `available` and answers
//! every call with `ai.off`. The screens that use it are hidden while the AI
//! switch is off (D-013). Everything reaches QML as text and plain objects
//! (D-065); the screens own every sentence (D-034), and every text a model
//! wrote is shown as plain text (T-34). The model's answer arrives with
//! `finished`, the lines of the tool-call log with `logged`, a request that
//! waits for the user's look with `reviewing`. Sets the user's own AI tools
//! prepare through `kassensturz-mcp` arrive with `requested` (M5.10,
//! D-218) and are then the same as the chat's.

use core::pin::Pin;

use cxx_qt::CxxQtType;
#[cfg(feature = "ai")]
use cxx_qt::Threading;
use cxx_qt_lib::{QString, QVariant};

use crate::errors::ErrorText;
use crate::variant::error_object;
#[cfg(feature = "ai")]
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The conversation with the AI about the open book, and the
        /// changes it prepares for the user to confirm.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, available, READ, CONSTANT)]
        #[qproperty(bool, busy, READ, NOTIFY)]
        type AiChat = super::AiChatRust;
    }

    impl cxx_qt::Threading for AiChat {}

    extern "RustQt" {
        /// Asks a question. `key` is the key typed for it, empty to use
        /// the kept one. Answers `started` (an object); with `started`
        /// false and a `code`, the question can't be asked; without a
        /// `code` there was nothing to ask or one is running already. The
        /// answer comes with `finished`.
        #[qinvokable]
        fn ask(self: Pin<&mut AiChat>, question: &QString, key: &QString) -> QVariant;

        /// Asks the question of a built-in feature (M5.9, D-222):
        /// `feature` is `categories`, `month` or `rules`, `account_id` the
        /// account the category suggestions are for, empty for all of
        /// them. Answers like `ask`; an unknown feature or a wrong ID
        /// answers `started` false without a `code`.
        #[qinvokable]
        #[cxx_name = "askFeature"]
        fn ask_feature(
            self: Pin<&mut AiChat>,
            feature: &QString,
            account_id: &QString,
            key: &QString,
        ) -> QVariant;

        /// Stops the question that runs, also when it waits for the user's
        /// look at a request.
        #[qinvokable]
        fn stop(self: &AiChat);

        /// The user lets the request they look at go as it is shown.
        #[qinvokable]
        #[cxx_name = "reviewSend"]
        fn review_send(self: &AiChat);

        /// The user does not let it go: the question ends.
        #[qinvokable]
        #[cxx_name = "reviewStop"]
        fn review_stop(self: &AiChat);

        /// The user leaves a word or phrase out of the request; it is
        /// shown again with `reviewing`.
        #[qinvokable]
        #[cxx_name = "reviewLeaveOut"]
        fn review_leave_out(self: &AiChat, phrase: &QString);

        /// The changes of the set `id` for the review screen: an array of
        /// objects with a `kind` (`newCategory`, `newRule` or `categorize`)
        /// and its fields, or an error object.
        #[qinvokable]
        fn changes(self: &AiChat, id: &QString) -> QVariant;

        /// The user confirms the set `id` on the review screen: it is
        /// applied as one change that undo takes back. An object with
        /// `ok`, or an error object; nothing is applied then.
        #[qinvokable]
        fn confirm(self: Pin<&mut AiChat>, id: &QString) -> QVariant;

        /// The user throws the set `id` away.
        #[qinvokable]
        fn discard(self: Pin<&mut AiChat>, id: &QString);

        /// Starts a new conversation: forgets this one and every set that
        /// waits, and stops a question that runs.
        #[qinvokable]
        fn reset(self: Pin<&mut AiChat>);

        /// Looks for changes the user's own AI tools prepared for the open
        /// book (D-218). Does nothing unless AI and the MCP write tools are
        /// both on. New sets arrive with `requested`.
        #[qinvokable]
        #[cxx_name = "checkRequests"]
        fn check_requests(self: Pin<&mut AiChat>);
    }

    extern "RustQt" {
        /// A new line of the tool-call log: an object with a `kind` and
        /// its fields (see `entry_object`).
        #[qsignal]
        fn logged(self: Pin<&mut AiChat>, entry: QVariant);

        /// A request waits for the user's look: an object with `address`,
        /// `model`, `body` and `leftOut` (in how many places the phrase
        /// just left out was found).
        #[qsignal]
        fn reviewing(self: Pin<&mut AiChat>, request: QVariant);

        /// The question ended: an object with `text`, `unverified` (the
        /// numbers no function produced), `finish`, `overview` and `sets`
        /// (objects with an `id` and a `count`), or an error object.
        #[qsignal]
        fn finished(self: Pin<&mut AiChat>, result: QVariant);

        /// A confirmed set changed the book: every screen reads it again.
        #[qsignal]
        fn changed(self: Pin<&mut AiChat>);

        /// The user's own AI tools prepared sets: a list of objects with an
        /// `id` and a `count`, as in `finished`.
        #[qsignal]
        fn requested(self: Pin<&mut AiChat>, sets: QVariant);
    }

    impl cxx_qt::Initialize for AiChat {}
}

/// The Rust side of [`qobject::AiChat`].
#[derive(Default)]
pub struct AiChatRust {
    available: bool,
    busy: bool,
    #[cfg(feature = "ai")]
    state: engine::State,
}

impl cxx_qt::Initialize for qobject::AiChat {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().available = cfg!(feature = "ai");
    }
}

/// The calls of the screens in a build without AI: all answer `ai.off`.
#[cfg(not(feature = "ai"))]
mod engine {
    use super::*;

    fn off() -> QVariant {
        error_object(&ErrorText::plain("ai.off"))
    }

    pub(super) fn ask(_chat: Pin<&mut qobject::AiChat>, _question: &str, _key: &str) -> QVariant {
        off()
    }

    pub(super) fn ask_feature(
        _chat: Pin<&mut qobject::AiChat>,
        _feature: &str,
        _account_id: &str,
        _key: &str,
    ) -> QVariant {
        off()
    }

    pub(super) fn changes(_chat: &qobject::AiChat, _id: &str) -> QVariant {
        off()
    }

    pub(super) fn confirm(_chat: Pin<&mut qobject::AiChat>, _id: &str) -> QVariant {
        off()
    }
}

#[cfg(feature = "ai")]
pub(crate) mod engine {
    use kassensturz_ai::{Finish, Place};
    use kassensturz_ai_chat::{Answer, BuildInfo, ChatError, ChatSession, Feature, LogEntry, Mode};
    use kassensturz_ai_tools::requests::RequestFolder;
    use kassensturz_ai_tools::{ChangeError, ToolError};
    use kassensturz_core::{Date, Report};
    use kassensturz_platform::Secret;
    use kassensturz_store::StoreError;

    use super::*;
    use crate::ai_chat::{
        ChangeView, Command, Controls, Review, Screen, Sets, Waiting, answer_from, code_text,
        confirm_in, hooks, view_of,
    };
    use crate::ai_requests::{Arrived, take_requests, tell_confirmed, tell_declined, tell_dropped};
    use crate::ai_settings::{self, Store};
    use crate::book_session::app_version;
    use crate::format::Locale;
    use crate::number_format::app_locale;
    use crate::rules_model::condition_object;
    use crate::session;

    /// What the object keeps between calls: the conversation, the buttons
    /// of the question that runs, the sets that wait for the user, a count
    /// that tells a late answer it was given up on (a new book, a new
    /// conversation or AI switched off), and the way requests from the
    /// user's own AI tools come in, once it was needed.
    #[derive(Default)]
    pub(crate) struct State {
        session: Option<ChatSession>,
        controls: Option<Controls>,
        sets: Sets,
        generation: u64,
        inbox: Option<RequestFolder>,
    }

    impl State {
        /// Forgets every set that waits, and tells the AI tools that sent
        /// some of them.
        fn forget_sets(&mut self) {
            let dropped = self.sets.clear();
            if let Some(inbox) = self.inbox.as_mut() {
                for from in &dropped {
                    tell_dropped(inbox, from);
                }
            }
        }
    }

    impl Drop for State {
        /// The app closes: a set an AI tool waits on is gone with it.
        fn drop(&mut self) {
            self.forget_sets();
        }
    }

    /// The window as the thread of a question sees it: every call is queued
    /// to the window's thread, and a window that is closing has no one left
    /// to tell.
    struct Window(cxx_qt::CxxQtThread<qobject::AiChat>);

    impl Screen for Window {
        fn logged(&self, entry: LogEntry) {
            let _ = self.0.queue(move |chat| chat.logged(entry_object(&entry)));
        }

        fn reviewing(&self, review: Review) {
            let _ = self
                .0
                .queue(move |chat| chat.reviewing(review_object(&review)));
        }
    }

    fn whole(count: usize) -> QVariant {
        QVariant::from(&i32::try_from(count).unwrap_or(i32::MAX))
    }

    /// A line of the tool-call log as QML reads it (D-192). The texts in it
    /// come from the user, the book or the model: plain text only (T-34).
    pub(crate) fn entry_object(entry: &LogEntry) -> QVariant {
        match entry {
            LogEntry::Asked { question } => {
                map([("kind", text("asked")), ("question", text(question))])
            }
            LogEntry::Sent { round, body } => map([
                ("kind", text("sent")),
                ("round", whole(*round)),
                ("body", text(body)),
            ]),
            LogEntry::Called {
                function,
                arguments,
                result,
                error,
            } => map([
                ("kind", text("called")),
                ("function", text(function)),
                ("arguments", text(arguments)),
                ("result", text(result)),
                ("error", text(error.unwrap_or(""))),
            ]),
            LogEntry::NotOffered { calls } => {
                map([("kind", text("notOffered")), ("calls", whole(*calls))])
            }
            LogEntry::Overview => map([("kind", text("overview"))]),
            LogEntry::Answered { unverified } => map([
                ("kind", text("answered")),
                ("unverified", whole(*unverified)),
            ]),
            LogEntry::Failed { code } => map([("kind", text("failed")), ("code", text(code))]),
        }
    }

    /// A request that waits for the user's look as QML reads it.
    pub(crate) fn review_object(review: &Review) -> QVariant {
        map([
            ("address", text(&review.address)),
            ("model", text(&review.model)),
            ("body", text(&review.body)),
            ("leftOut", whole(review.left_out)),
        ])
    }

    /// The model's answer as QML reads it: its text (untrusted, shown as
    /// plain text and labelled as AI, T-34), the numbers no function
    /// produced (D-188, T-26) and the sets that wait for the user.
    pub(crate) fn finished_object(answer: &Answer, sets: &[(u64, usize)]) -> QVariant {
        let finish = match answer.finish {
            Finish::Length => "length",
            Finish::Other => "other",
            _ => "done",
        };
        map([
            ("text", text(&answer.text)),
            (
                "unverified",
                list(answer.unverified.iter().map(|number| text(&number.text))),
            ),
            ("finish", text(finish)),
            ("overview", QVariant::from(&(answer.mode == Mode::Overview))),
            ("sets", sets_object(sets)),
        ])
    }

    /// Sets that wait for the user as QML reads them: each with an `id`
    /// and a `count`.
    pub(crate) fn sets_object(sets: &[(u64, usize)]) -> QVariant {
        list(
            sets.iter()
                .map(|(id, count)| map([("id", text(&id.to_string())), ("count", whole(*count))])),
        )
    }

    /// The changes of a set as QML reads them.
    pub(crate) fn changes_object(views: &[ChangeView]) -> QVariant {
        list(views.iter().map(|view| match view {
            ChangeView::NewCategory { name, path, income } => map([
                ("kind", text("newCategory")),
                ("name", text(name)),
                ("path", text(path)),
                ("income", QVariant::from(income)),
            ]),
            ChangeView::NewRule {
                name,
                book_to,
                match_all,
                conditions,
            } => map([
                ("kind", text("newRule")),
                ("name", text(name)),
                ("bookTo", text(book_to)),
                ("matchAll", QVariant::from(match_all)),
                ("conditions", list(conditions.iter().map(condition_object))),
            ]),
            ChangeView::Categorize {
                date,
                payee,
                account,
                amount,
                before,
                after,
            } => map([
                ("kind", text("categorize")),
                ("date", text(date)),
                ("payee", text(payee)),
                ("account", text(account)),
                ("amount", text(amount)),
                ("before", text(before)),
                ("after", text(after)),
            ]),
        }))
    }

    fn error(error: &ErrorText) -> QVariant {
        error_object(error)
    }

    fn book_unreadable(cause: StoreError) -> ErrorText {
        code_text(&ChatError::Book(ToolError::Store(cause)).report())
    }

    pub(super) fn ask(mut chat: Pin<&mut qobject::AiChat>, question: &str, key: &str) -> QVariant {
        let not_started = || map([("started", QVariant::from(&false))]);
        let question = question.trim();
        if chat.busy || question.is_empty() {
            return not_started();
        }
        let store = Store::user();
        if !store.config().read().enabled() {
            return error(&ErrorText::plain("ai.off"));
        }
        let Some(path) = session::path() else {
            return error(&ErrorText::plain("ai-chat.book-unreadable"));
        };
        // The fake model stands for a provider on another computer, so the
        // review of each request can be tried with it (D-210).
        let demo = ai_settings::demo_model();
        let ask_first = demo.is_some() || store.form().place == Some(Place::AnotherComputer);
        let today = app_locale().today();
        let typed = (!key.is_empty()).then(|| Secret::new(key.to_owned()));
        let question = question.to_owned();
        let (controls, mut hooks) = hooks(Box::new(Window(chat.qt_thread())), ask_first);
        let (earlier, generation) = {
            let mut rust = chat.as_mut().rust_mut();
            rust.state.controls = Some(controls);
            rust.busy = true;
            (rust.state.session.take(), rust.state.generation)
        };
        let mut conversation = earlier.unwrap_or_else(|| {
            ChatSession::new(BuildInfo::new(app_version(), ai_settings::build_commit()))
        });
        chat.as_mut().busy_changed();
        let thread = chat.qt_thread();
        std::thread::spawn(move || {
            // Read on this thread, from a second, read-only connection to
            // the book (D-195); the window's own book is never touched.
            let store = Store::user();
            let result = answer_from(
                &path,
                &mut conversation,
                &store,
                demo,
                typed,
                &question,
                today,
                &mut hooks,
            );
            let _ = thread.queue(move |chat| finish(chat, generation, conversation, result));
        });
        map([("started", QVariant::from(&true))])
    }

    /// The question for the feature a screen names, if it names one and
    /// the account is empty or an ID.
    pub(crate) fn feature_question(feature: &str, account_id: &str, today: Date) -> Option<String> {
        Feature::from_key(feature)?.question(today, Some(account_id))
    }

    /// Asks the question of the built-in feature `feature` (M5.9, D-222),
    /// the way a typed one is asked.
    pub(super) fn ask_feature(
        chat: Pin<&mut qobject::AiChat>,
        feature: &str,
        account_id: &str,
        key: &str,
    ) -> QVariant {
        match feature_question(feature, account_id, app_locale().today()) {
            Some(question) => ask(chat, &question, key),
            None => map([("started", QVariant::from(&false))]),
        }
    }

    /// The question ended on its thread: keeps the conversation and the
    /// sets it prepared, and tells the screens.
    fn finish(
        mut chat: Pin<&mut qobject::AiChat>,
        generation: u64,
        conversation: ChatSession,
        result: Result<Answer, ChatError>,
    ) {
        if chat.state.generation != generation {
            // A new book, a new conversation or AI off meanwhile: the
            // answer is thrown away.
            return;
        }
        let outcome = {
            let mut rust = chat.as_mut().rust_mut();
            let state = &mut rust.state;
            state.session = Some(conversation);
            state.controls = None;
            let outcome = match result {
                Ok(mut answer) => {
                    let sets: Vec<(u64, usize)> = answer
                        .changes
                        .drain(..)
                        .map(|set| {
                            let count = set.changes().len();
                            (state.sets.add(set), count)
                        })
                        .collect();
                    finished_object(&answer, &sets)
                }
                Err(failure) => error(&code_text(&failure.report())),
            };
            rust.busy = false;
            outcome
        };
        chat.as_mut().busy_changed();
        chat.finished(outcome);
    }

    pub(super) fn command(chat: &qobject::AiChat, command: Command) {
        if let Some(controls) = &chat.state.controls {
            controls.command(command);
        }
    }

    pub(super) fn stop(chat: &qobject::AiChat) {
        if let Some(controls) = &chat.state.controls {
            controls.stop();
        }
    }

    fn id_of(id: &str) -> Option<u64> {
        id.parse().ok()
    }

    pub(super) fn changes(chat: &qobject::AiChat, id: &str) -> QVariant {
        let Some(set) = id_of(id).and_then(|id| chat.state.sets.get(id)) else {
            return error(&ErrorText::plain("ai-changes.out-of-date"));
        };
        match session::with_book(|book| view_of(book, set, &app_locale())) {
            Some(Ok(views)) => changes_object(&views),
            Some(Err(cause)) => error(&book_unreadable(cause)),
            None => error(&ErrorText::plain("ai-changes.out-of-date")),
        }
    }

    pub(super) fn confirm(mut chat: Pin<&mut qobject::AiChat>, id: &str) -> QVariant {
        let waiting = id_of(id).and_then(|id| chat.as_mut().rust_mut().state.sets.take(id));
        let Some(Waiting { set, from }) = waiting else {
            return error(&ErrorText::plain("ai-changes.out-of-date"));
        };
        // The only call that applies a set: the confirm button's (D-200).
        // No book open any more means the set is out of date.
        let result = session::with_book_mut(|book| confirm_in(book, set))
            .unwrap_or(Err(ChangeError::OutOfDate));
        if let Some(from) = from
            && let Some(inbox) = chat.as_mut().rust_mut().state.inbox.as_mut()
        {
            tell_confirmed(inbox, &from, &result);
        }
        match result {
            Ok(()) => {
                chat.as_mut().changed();
                map([("ok", QVariant::from(&true))])
            }
            Err(failure) => error(&code_text(&failure.report())),
        }
    }

    pub(super) fn discard(mut chat: Pin<&mut qobject::AiChat>, id: &str) {
        let mut rust = chat.as_mut().rust_mut();
        let state = &mut rust.state;
        if let Some(Waiting {
            from: Some(from), ..
        }) = id_of(id).and_then(|id| state.sets.take(id))
            && let Some(inbox) = state.inbox.as_mut()
        {
            tell_declined(inbox, &from);
        }
    }

    pub(super) fn check_requests(mut chat: Pin<&mut qobject::AiChat>) {
        // The folder is looked at only while AI and the write tools are
        // both on (D-175, D-218), and only with a book open.
        if !kassensturz_core::ai_switch::write_tools_enabled() {
            return;
        }
        let Some(path) = session::path() else {
            return;
        };
        let arrived: Vec<Arrived> = {
            let mut rust = chat.as_mut().rust_mut();
            let State { sets, inbox, .. } = &mut rust.state;
            if inbox.is_none() {
                *inbox = RequestFolder::default_place()
                    .and_then(|place| RequestFolder::open(place).ok());
            }
            let Some(inbox) = inbox.as_mut() else {
                return;
            };
            session::with_book(|book| take_requests(inbox, &path, book, sets)).unwrap_or_default()
        };
        if !arrived.is_empty() {
            let sets: Vec<(u64, usize)> = arrived.iter().map(|set| (set.id, set.count)).collect();
            chat.requested(sets_object(&sets));
        }
    }

    pub(super) fn reset(mut chat: Pin<&mut qobject::AiChat>) {
        let was_busy = {
            let mut rust = chat.as_mut().rust_mut();
            rust.state.generation += 1;
            if let Some(controls) = rust.state.controls.take() {
                controls.stop();
            }
            rust.state.session = None;
            rust.state.forget_sets();
            std::mem::replace(&mut rust.busy, false)
        };
        if was_busy {
            chat.as_mut().busy_changed();
        }
    }

    pub(super) fn leave_out(chat: &qobject::AiChat, phrase: &str) {
        command(chat, Command::LeaveOut(phrase.to_owned()));
    }
}

impl qobject::AiChat {
    fn ask(self: Pin<&mut Self>, question: &QString, key: &QString) -> QVariant {
        engine::ask(self, &question.to_string(), &key.to_string())
    }

    fn ask_feature(
        self: Pin<&mut Self>,
        feature: &QString,
        account_id: &QString,
        key: &QString,
    ) -> QVariant {
        engine::ask_feature(
            self,
            &feature.to_string(),
            &account_id.to_string(),
            &key.to_string(),
        )
    }

    fn changes(&self, id: &QString) -> QVariant {
        engine::changes(self, &id.to_string())
    }

    fn confirm(self: Pin<&mut Self>, id: &QString) -> QVariant {
        engine::confirm(self, &id.to_string())
    }
}

#[cfg(feature = "ai")]
impl qobject::AiChat {
    fn stop(&self) {
        engine::stop(self);
    }

    fn review_send(&self) {
        engine::command(self, crate::ai_chat::Command::Send);
    }

    fn review_stop(&self) {
        engine::command(self, crate::ai_chat::Command::Stop);
    }

    fn review_leave_out(&self, phrase: &QString) {
        engine::leave_out(self, &phrase.to_string());
    }

    fn discard(self: Pin<&mut Self>, id: &QString) {
        engine::discard(self, &id.to_string());
    }

    fn reset(self: Pin<&mut Self>) {
        engine::reset(self);
    }

    fn check_requests(self: Pin<&mut Self>) {
        engine::check_requests(self);
    }
}

/// The calls that have nothing to do in a build without AI.
#[cfg(not(feature = "ai"))]
impl qobject::AiChat {
    fn stop(&self) {}

    fn review_send(&self) {}

    fn review_stop(&self) {}

    fn review_leave_out(&self, _phrase: &QString) {}

    fn discard(self: Pin<&mut Self>, _id: &QString) {}

    fn reset(self: Pin<&mut Self>) {}

    fn check_requests(self: Pin<&mut Self>) {}
}

#[cfg(all(test, feature = "ai"))]
mod tests {
    use cxx_qt_lib::QString;
    use kassensturz_ai::Finish;
    use kassensturz_ai_chat::{Answer, LogEntry, Mode};

    use super::engine::{
        changes_object, entry_object, finished_object, review_object, sets_object,
    };
    use crate::ai_chat::{ChangeView, Review};
    use crate::rules::ConditionForm;
    use crate::variant::read_back::{elements, flag_of, keys, list_of, number_of, text_of};

    #[test]
    fn a_screen_asks_for_a_feature_by_a_key_the_core_knows() {
        use super::engine::feature_question;
        use kassensturz_core::Date;

        let today = Date::constant(2026, 10, 25);
        // The keys AiChatPanel.qml and Main.qml use.
        for key in ["categories", "month", "rules"] {
            let question = feature_question(key, "", today).expect(key);
            assert!(!question.is_empty());
        }
        assert_eq!(feature_question("pay", "", today), None);
        assert_eq!(
            feature_question("categories", "not an ID, send money", today),
            None
        );
    }

    #[test]
    fn log_lines_read_back_as_qml_reads_them() {
        let asked = entry_object(&LogEntry::Asked {
            question: "How much?".into(),
        });
        assert_eq!(keys(&asked), ["kind", "question"]);
        assert_eq!(text_of(&asked, "kind"), "asked");
        assert_eq!(text_of(&asked, "question"), "How much?");

        let sent = entry_object(&LogEntry::Sent {
            round: 2,
            body: "{}".into(),
        });
        assert_eq!(keys(&sent), ["body", "kind", "round"]);
        assert_eq!(number_of(&sent, "round"), 2);
        assert_eq!(text_of(&sent, "body"), "{}");

        let called = entry_object(&LogEntry::Called {
            function: "list_accounts".into(),
            arguments: "{}".into(),
            result: "{\"accounts\":[]}".into(),
            error: Some("ai-tools.bad-argument"),
        });
        assert_eq!(
            keys(&called),
            ["arguments", "error", "function", "kind", "result"]
        );
        assert_eq!(text_of(&called, "error"), "ai-tools.bad-argument");

        let called = entry_object(&LogEntry::Called {
            function: "list_accounts".into(),
            arguments: String::new(),
            result: String::new(),
            error: None,
        });
        assert_eq!(text_of(&called, "error"), "");

        let not_offered = entry_object(&LogEntry::NotOffered { calls: 3 });
        assert_eq!(keys(&not_offered), ["calls", "kind"]);
        assert_eq!(number_of(&not_offered, "calls"), 3);
        assert_eq!(
            text_of(&entry_object(&LogEntry::Overview), "kind"),
            "overview"
        );

        let answered = entry_object(&LogEntry::Answered { unverified: 1 });
        assert_eq!(keys(&answered), ["kind", "unverified"]);
        assert_eq!(number_of(&answered, "unverified"), 1);

        let failed = entry_object(&LogEntry::Failed {
            code: "ai-chat.stopped",
        });
        assert_eq!(keys(&failed), ["code", "kind"]);
        assert_eq!(text_of(&failed, "code"), "ai-chat.stopped");
    }

    #[test]
    fn a_request_to_look_at_reads_back_as_qml_reads_it() {
        let request = review_object(&Review {
            address: "https://api.example.com/v1".into(),
            model: "big".into(),
            body: "{\"messages\":[]}".into(),
            left_out: 2,
        });
        assert_eq!(keys(&request), ["address", "body", "leftOut", "model"]);
        assert_eq!(text_of(&request, "address"), "https://api.example.com/v1");
        assert_eq!(text_of(&request, "model"), "big");
        assert_eq!(text_of(&request, "body"), "{\"messages\":[]}");
        assert_eq!(number_of(&request, "leftOut"), 2);
    }

    #[test]
    fn an_answer_reads_back_with_its_unverified_numbers_and_sets() {
        use kassensturz_ai_tools::{Figures, check_numbers};
        let text = "Your balance is 4711.08 and rent 1234.50.";
        let mut figures = Figures::new();
        figures.add_text("1234.50");
        let answer = Answer {
            text: text.into(),
            unverified: check_numbers(text, &figures),
            finish: Finish::Length,
            mode: Mode::Overview,
            changes: Vec::new(),
        };
        let object = finished_object(&answer, &[(3, 2), (4, 1)]);
        assert_eq!(
            keys(&object),
            ["finish", "overview", "sets", "text", "unverified"]
        );
        assert_eq!(text_of(&object, "text"), text);
        assert_eq!(text_of(&object, "finish"), "length");
        assert!(flag_of(&object, "overview"));
        let unverified = list_of(&object, "unverified");
        assert_eq!(unverified.len(), 1);
        assert_eq!(
            unverified[0].value::<QString>().map(|n| n.to_string()),
            Some("4711.08".to_owned()),
            "a number is text"
        );
        let sets = list_of(&object, "sets");
        assert_eq!(sets.len(), 2);
        assert_eq!(keys(&sets[0]), ["count", "id"]);
        assert_eq!(text_of(&sets[0], "id"), "3");
        assert_eq!(number_of(&sets[0], "count"), 2);
    }

    #[test]
    fn sets_from_ai_tools_read_back_as_qml_reads_them() {
        let object = sets_object(&[(7, 3)]);
        let sets = elements(&object).expect("an array");
        assert_eq!(sets.len(), 1);
        assert_eq!(keys(&sets[0]), ["count", "id"]);
        assert_eq!(text_of(&sets[0], "id"), "7");
        assert_eq!(number_of(&sets[0], "count"), 3);
    }

    #[test]
    fn the_changes_of_a_set_read_back_as_qml_reads_them() {
        let views = [
            ChangeView::NewCategory {
                name: "Bakery".into(),
                path: "Food › Bakery".into(),
                income: false,
            },
            ChangeView::NewRule {
                name: "Bakery".into(),
                book_to: "Food › Bakery".into(),
                match_all: true,
                conditions: vec![ConditionForm {
                    kind: "text".into(),
                    field: "payee".into(),
                    operator: "contains".into(),
                    value: "<img src=x>".into(),
                    ..ConditionForm::default()
                }],
            },
            ChangeView::Categorize {
                date: "2026-10-03".into(),
                payee: "Corner Grocery".into(),
                account: "Everyday Checking".into(),
                amount: "-53,20 €".into(),
                before: String::new(),
                after: "Food › Bakery".into(),
            },
        ];
        let object = changes_object(&views);
        let items = elements(&object).expect("an array");
        assert_eq!(items.len(), 3);
        assert_eq!(keys(&items[0]), ["income", "kind", "name", "path"]);
        assert_eq!(text_of(&items[0], "kind"), "newCategory");
        assert!(!flag_of(&items[0], "income"));
        assert_eq!(
            keys(&items[1]),
            ["bookTo", "conditions", "kind", "matchAll", "name"]
        );
        assert!(flag_of(&items[1], "matchAll"));
        let conditions = list_of(&items[1], "conditions");
        assert_eq!(text_of(&conditions[0], "operator"), "contains");
        assert_eq!(text_of(&conditions[0], "value"), "<img src=x>");
        assert_eq!(
            keys(&items[2]),
            [
                "account", "after", "amount", "before", "date", "kind", "payee"
            ]
        );
        assert_eq!(text_of(&items[2], "amount"), "-53,20 €");
        assert_eq!(text_of(&items[2], "before"), "");
    }
}
