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
        /// objects with a `kind` (`newCategory`, `editCategory`, `newRule`,
        /// `editRule`, `moveRule`, `deleteRule`, `categorize`, `deleteCategory`,
        /// `newAccount` or `editAccount`)
        /// and its fields, each with the `slot` that names it, whether the
        /// user `edited` it or left it out (`leftOut`), the `fields` they may
        /// edit and the `form` (JSON text) an edit starts from (D-293), or an
        /// error object.
        #[qinvokable]
        fn changes(self: &AiChat, id: &QString) -> QVariant;

        /// The user corrects the change `slot` of the set `id` before
        /// confirming it (D-293): `fields` is a JSON object of the fields
        /// that change, in the words of `propose_changes`. The set's changes
        /// as `changes` gives them, or an error object; the set stays as it
        /// was then. Nothing of it goes to the AI.
        #[qinvokable]
        #[cxx_name = "editChange"]
        fn edit_change(
            self: Pin<&mut AiChat>,
            id: &QString,
            slot: &QString,
            fields: &QString,
        ) -> QVariant;

        /// The user leaves the change `slot` out of the set `id`; answers
        /// like `editChange`.
        #[qinvokable]
        #[cxx_name = "leaveOutChange"]
        fn leave_out_change(self: Pin<&mut AiChat>, id: &QString, slot: &QString) -> QVariant;

        /// The user takes the AI's version of the change `slot` of the set
        /// `id` again; answers like `editChange`.
        #[qinvokable]
        #[cxx_name = "backToAi"]
        fn back_to_ai(self: Pin<&mut AiChat>, id: &QString, slot: &QString) -> QVariant;

        /// The categories a change of the set `id` may be given: objects
        /// with `id` (one of the book's) or `key` (a new one of the set),
        /// `path`, `income` and `currency`, or an error object.
        #[qinvokable]
        #[cxx_name = "changeChoices"]
        fn change_choices(self: &AiChat, id: &QString) -> QVariant;

        /// The user confirms the set `id` on the review screen: it is
        /// applied as one change that undo takes back. An object with
        /// `ok`, or an error object; nothing is applied then.
        #[qinvokable]
        fn confirm(self: Pin<&mut AiChat>, id: &QString) -> QVariant;

        /// The user throws the set `id` away.
        #[qinvokable]
        fn discard(self: Pin<&mut AiChat>, id: &QString);

        /// The user took the changes they confirmed back with Undo: the
        /// model hears it with the next question.
        #[qinvokable]
        fn changes_undone(self: Pin<&mut AiChat>);

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

        /// The user did something with an issue draft (D-228): `opened`
        /// the prefilled page or `copied` its text. Noted in the developer
        /// mode's log (D-227); nothing else happens here, since the page
        /// opens in the user's own browser.
        #[qinvokable]
        #[cxx_name = "issueUsed"]
        fn issue_used(self: &AiChat, how: &QString);
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
        /// numbers no function produced), `finish`, `overview`, `sets`
        /// (objects with an `id` and a `count`) and, when the AI prepared
        /// one, `issue` (`title`, `body`, `link`, `wholeInLink`), `page`
        /// (`name` and `id`) when the AI asked to open a page of the app, or
        /// an error object.
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

    pub(super) fn off() -> QVariant {
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
    use kassensturz_ai_chat::{
        Answer, BuildInfo, ChatError, ChatSession, Feature, LogEntry, Mode, Outcome,
    };
    use kassensturz_ai_tools::requests::RequestFolder;
    use kassensturz_ai_tools::{
        ChangeError, ChangeSet, EditError, IssueDraft, PageTarget, ProposedChange, ToolError,
    };
    use kassensturz_core::{Date, Report};
    use kassensturz_platform::Secret;
    use kassensturz_store::StoreError;

    use super::*;
    use crate::ai_chat::{
        BasisView, CategoryChoice, ChangeRow, ChangeView, Command, Controls, Correction, Review,
        Screen, Sets, Waiting, answer_from, category_choices, code_text, confirm_in, corrected,
        hooks, outcome_of, rows_of,
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
        /// What the user did with prepared changes while a question ran.
        news: Vec<Outcome>,
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

    impl State {
        /// Tells the conversation what the user did with a set it prepared
        /// (D-229). While a question runs the conversation is on its
        /// thread, so the news waits and goes in when it comes back.
        fn tell_chat(&mut self, outcome: Outcome) {
            match self.session.as_mut() {
                Some(session) => session.note_outcome(outcome),
                None => self.news.push(outcome),
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

    /// A chart the model prepared, which waits in the chat as a preview card
    /// with *Keep*, *Change…* and *Leave out* (M6c.9, D-355, D-402).
    pub(crate) struct ChartSet {
        /// The set's ID: `AiChat.changes` gives its chart, `confirm` keeps
        /// it and `discard` leaves it out.
        pub(crate) id: u64,
        /// The numbers in its title that no function produced (D-188).
        pub(crate) unverified: Vec<String>,
    }

    /// Whether `set` is one new chart and nothing else, which the chat shows
    /// as a preview card instead of a line that leads to the review page
    /// (D-402).
    pub(crate) fn is_chart(set: &ChangeSet) -> bool {
        matches!(set.changes(), [ProposedChange::NewView(_)])
    }

    /// The model's answer as QML reads it: its text (untrusted, shown as
    /// plain text and labelled as AI, T-34), the numbers no function
    /// produced (D-188, T-26), the sets that wait for the user, the charts
    /// that wait for the user's look, the chart the model gave up on and the
    /// issue draft the user reads before opening its page (D-228).
    pub(crate) fn finished_object(
        answer: &Answer,
        sets: &[(u64, usize)],
        charts: &[ChartSet],
    ) -> QVariant {
        let finish = match answer.finish {
            Finish::Length => "length",
            Finish::Other => "other",
            _ => "done",
        };
        let mut fields = vec![
            ("text", text(&answer.text)),
            (
                "unverified",
                list(answer.unverified.iter().map(|number| text(&number.text))),
            ),
            ("finish", text(finish)),
            ("overview", QVariant::from(&(answer.mode == Mode::Overview))),
            ("contextCut", QVariant::from(&answer.context_cut)),
            ("sets", sets_object(sets)),
            ("charts", charts_object(charts)),
        ];
        if let Some(failed) = &answer.chart_failed {
            // The dataset it understood is one of the catalog's own ids,
            // empty when it named none the AI may use (D-402).
            fields.push((
                "chartFailed",
                map([("dataset", text(failed.dataset.unwrap_or_default()))]),
            ));
        }
        if let Some(issue) = &answer.issue {
            fields.push(("issue", issue_object(issue)));
        }
        if let Some(page) = &answer.page {
            fields.push(("page", page_object(page)));
        }
        map(fields)
    }

    /// An issue draft as QML reads it. The link is KassenSturz's own issue
    /// page with the draft in it; the text is shown as plain text.
    pub(crate) fn issue_object(issue: &IssueDraft) -> QVariant {
        map([
            ("title", text(&issue.title)),
            ("body", text(&issue.body)),
            ("link", text(&issue.link)),
            ("wholeInLink", QVariant::from(&issue.whole_in_link)),
        ])
    }

    /// A page the AI asked to open as QML reads it: its fixed `name` and the
    /// `id` of the account or Rule it is about, empty when there is none
    /// (D-265).
    pub(crate) fn page_object(page: &PageTarget) -> QVariant {
        map([
            ("name", text(page.name())),
            ("id", text(page.id().unwrap_or_default())),
        ])
    }

    /// Charts that wait in the chat as QML reads them: each with an `id` and
    /// the `unverified` numbers of its title.
    pub(crate) fn charts_object(charts: &[ChartSet]) -> QVariant {
        list(charts.iter().map(|chart| {
            map([
                ("id", text(&chart.id.to_string())),
                (
                    "unverified",
                    list(chart.unverified.iter().map(|number| text(number))),
                ),
            ])
        }))
    }

    /// Sets that wait for the user as QML reads them: each with an `id`
    /// and a `count`.
    pub(crate) fn sets_object(sets: &[(u64, usize)]) -> QVariant {
        list(
            sets.iter()
                .map(|(id, count)| map([("id", text(&id.to_string())), ("count", whole(*count))])),
        )
    }

    /// The changes of a set as QML reads them: each change's own fields,
    /// and what the user did with it (D-293).
    pub(crate) fn changes_object(rows: &[ChangeRow]) -> QVariant {
        list(rows.iter().map(|row| {
            let mut entries = view_entries(&row.view);
            entries.extend([
                ("slot", text(&row.slot)),
                ("edited", QVariant::from(&row.edited)),
                ("leftOut", QVariant::from(&row.left_out)),
                ("fields", list(row.fields.iter().map(|field| text(field)))),
                ("form", text(&row.form)),
            ]);
            map(entries)
        }))
    }

    /// The categories a change may be given as QML reads them.
    pub(crate) fn choices_object(choices: &[CategoryChoice]) -> QVariant {
        list(choices.iter().map(|choice| {
            map([
                ("id", text(&choice.id)),
                ("key", text(&choice.key)),
                ("path", text(&choice.path)),
                ("income", QVariant::from(&choice.income)),
                ("currency", text(&choice.currency)),
            ])
        }))
    }

    /// One change's own fields.
    fn view_entries(view: &ChangeView) -> Vec<(&'static str, QVariant)> {
        match view {
            ChangeView::NewCategory {
                name,
                path,
                income,
                currency,
            } => vec![
                ("kind", text("newCategory")),
                ("name", text(name)),
                ("path", text(path)),
                ("income", QVariant::from(income)),
                ("currency", text(currency)),
            ],
            ChangeView::NewRule {
                name,
                book_to,
                match_all,
                conditions,
            } => vec![
                ("kind", text("newRule")),
                ("name", text(name)),
                ("bookTo", text(book_to)),
                ("matchAll", QVariant::from(match_all)),
                ("conditions", list(conditions.iter().map(condition_object))),
            ],
            ChangeView::EditCategory { before, after } => vec![
                ("kind", text("editCategory")),
                ("before", text(before)),
                ("after", text(after)),
            ],
            ChangeView::EditRule {
                before_name,
                name,
                before_book_to,
                book_to,
                match_all,
                conditions,
            } => vec![
                ("kind", text("editRule")),
                ("beforeName", text(before_name)),
                ("name", text(name)),
                ("beforeBookTo", text(before_book_to)),
                ("bookTo", text(book_to)),
                ("matchAll", QVariant::from(match_all)),
                ("conditions", list(conditions.iter().map(condition_object))),
            ],
            ChangeView::MoveRule { name, from, to } => vec![
                ("kind", text("moveRule")),
                ("name", text(name)),
                ("from", text(from)),
                ("to", text(to)),
            ],
            ChangeView::DeleteRule {
                name,
                book_to,
                match_all,
                conditions,
            } => vec![
                ("kind", text("deleteRule")),
                ("name", text(name)),
                ("bookTo", text(book_to)),
                ("matchAll", QVariant::from(match_all)),
                ("conditions", list(conditions.iter().map(condition_object))),
            ],
            ChangeView::Categorize {
                date,
                payee,
                account,
                amount,
                before,
                after,
                currency,
            } => vec![
                ("kind", text("categorize")),
                ("date", text(date)),
                ("payee", text(payee)),
                ("account", text(account)),
                ("amount", text(amount)),
                ("before", text(before)),
                ("after", text(after)),
                ("currency", text(currency)),
            ],
            ChangeView::DeleteCategory { path } => {
                vec![("kind", text("deleteCategory")), ("path", text(path))]
            }
            ChangeView::NewAccount {
                name,
                account_type,
                currency,
            } => vec![
                ("kind", text("newAccount")),
                ("name", text(name)),
                ("accountType", text(account_type)),
                ("currency", text(currency)),
            ],
            ChangeView::EditAccount {
                before,
                after,
                was_closed,
                closed,
            } => vec![
                ("kind", text("editAccount")),
                ("before", text(before)),
                ("after", text(after)),
                ("wasClosed", QVariant::from(was_closed)),
                ("closed", QVariant::from(closed)),
            ],
            ChangeView::EditTransaction {
                date,
                payee,
                account,
                amount,
                lines,
            } => vec![
                ("kind", text("editTransaction")),
                ("date", text(date)),
                ("payee", text(payee)),
                ("account", text(account)),
                ("amount", text(amount)),
                (
                    "lines",
                    list(lines.iter().map(|line| {
                        map([
                            ("field", text(line.field)),
                            ("before", text(&line.before)),
                            ("after", text(&line.after)),
                        ])
                    })),
                ),
            ],
            ChangeView::NewTransaction {
                date,
                payee,
                account,
                amount,
                category,
                note,
            } => vec![
                ("kind", text("newTransaction")),
                ("date", text(date)),
                ("payee", text(payee)),
                ("account", text(account)),
                ("amount", text(amount)),
                ("category", text(category)),
                ("note", text(note)),
            ],
            ChangeView::DeleteTransaction {
                date,
                payee,
                account,
                amount,
                category,
            } => vec![
                ("kind", text("deleteTransaction")),
                ("date", text(date)),
                ("payee", text(payee)),
                ("account", text(account)),
                ("amount", text(amount)),
                ("category", text(category)),
            ],
            ChangeView::NewView {
                title,
                period,
                component,
                charts,
            } => vec![
                ("kind", text("newView")),
                ("title", text(title)),
                ("period", text(period)),
                ("component", text(component)),
                ("charts", list(charts.iter().map(|chart| text(chart)))),
            ],
            ChangeView::NewBudget {
                category,
                every,
                from,
                to,
                amount,
                carry_over,
                basis,
            } => {
                let mut fields = vec![
                    ("kind", text("newBudget")),
                    ("category", text(category)),
                    ("every", text(every)),
                    ("from", text(from)),
                    ("to", text(to)),
                    ("amount", text(amount)),
                    ("carryOver", QVariant::from(carry_over)),
                ];
                fields.extend(basis_fields(basis.as_ref()));
                fields
            }
            ChangeView::EditBudget {
                category,
                every,
                from,
                to,
                before,
                amount,
                carried_over,
                carry_over,
                basis,
            } => {
                let mut fields = vec![
                    ("kind", text("editBudget")),
                    ("category", text(category)),
                    ("every", text(every)),
                    ("from", text(from)),
                    ("to", text(to)),
                    ("before", text(before)),
                    ("amount", text(amount)),
                    ("carriedOver", QVariant::from(carried_over)),
                    ("carryOver", QVariant::from(carry_over)),
                ];
                fields.extend(basis_fields(basis.as_ref()));
                fields
            }
        }
    }

    /// How a budget's amount was computed, as fields of its change object:
    /// `basisPeriods` is 0 for an amount the user named, and the texts are
    /// then empty (D-395).
    fn basis_fields(basis: Option<&BasisView>) -> [(&'static str, QVariant); 6] {
        let none = String::new();
        let texts = basis.map_or([&none, &none, &none, &none, &none], |basis| {
            [
                &basis.from,
                &basis.to,
                &basis.spent,
                &basis.average,
                &basis.step,
            ]
        });
        [
            (
                "basisPeriods",
                QVariant::from(&i32::from(basis.map_or(0, |basis| basis.periods))),
            ),
            ("basisFrom", text(texts[0])),
            ("basisTo", text(texts[1])),
            ("basisSpent", text(texts[2])),
            ("basisAverage", text(texts[3])),
            ("basisStep", text(texts[4])),
        ]
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
        let mut conversation = earlier.unwrap_or_else(new_conversation);
        conversation.set_screen(crate::developer_mode::current_screen());
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

    /// A new conversation in this build, writing to the developer mode's
    /// log while it runs (D-227).
    fn new_conversation() -> ChatSession {
        let conversation =
            ChatSession::new(BuildInfo::new(app_version(), ai_settings::build_commit()));
        #[cfg(feature = "debug-log")]
        let conversation = conversation.with_debug_log(crate::developer_mode::log().clone());
        conversation
    }

    /// The question for the feature a screen names, if it names one and
    /// the account is empty or an ID.
    pub(crate) fn feature_question(feature: &str, account_id: &str, today: Date) -> Option<String> {
        Feature::from_key(feature)?.question_in(
            today,
            Some(account_id),
            crate::language::at_start().answer_language(),
        )
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
            for outcome in std::mem::take(&mut state.news) {
                state.tell_chat(outcome);
            }
            state.controls = None;
            let outcome = match result {
                Ok(mut answer) => {
                    let numbers = std::mem::take(&mut answer.title_numbers);
                    let mut sets = Vec::new();
                    let mut charts = Vec::new();
                    for (index, set) in answer.changes.drain(..).enumerate() {
                        if is_chart(&set) {
                            // A number the chat could not check is marked,
                            // never left unmarked (D-188, D-402).
                            let unverified = numbers.get(index).map_or_else(
                                || vec!["?".to_owned()],
                                |found| found.iter().map(|number| number.text.clone()).collect(),
                            );
                            charts.push(ChartSet {
                                id: state.sets.add(set),
                                unverified,
                            });
                        } else {
                            let count = set.changes().len();
                            sets.push((state.sets.add(set), count));
                        }
                    }
                    finished_object(&answer, &sets, &charts)
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
        match session::with_book(|book| rows_of(book, set, &app_locale())) {
            Some(Ok(rows)) => changes_object(&rows),
            Some(Err(cause)) => error(&book_unreadable(cause)),
            None => error(&ErrorText::plain("ai-changes.out-of-date")),
        }
    }

    pub(super) fn confirm(mut chat: Pin<&mut qobject::AiChat>, id: &str) -> QVariant {
        let waiting = id_of(id).and_then(|id| chat.as_mut().rust_mut().state.sets.take(id));
        let Some(Waiting { set, from }) = waiting else {
            return error(&ErrorText::plain("ai-changes.out-of-date"));
        };
        // A set one of the user's own AI tools prepared belongs to no
        // question of the chat (M5-B20).
        let say = |kind: &'static str, fields: &[(&'static str, &str)]| {
            if from.is_some() {
                crate::developer_mode::transcribe_in("mcp", kind, fields);
            } else {
                crate::developer_mode::transcribe(kind, fields);
            }
        };
        say("changes-confirmed", &[("set", &set.summary())]);
        // The chat hears that the user corrected the set, never how (D-298).
        let corrected = set.corrected();
        // The only call that applies a set: the confirm button's (D-200).
        // No book open any more means the set is out of date.
        let result = session::with_book_mut(|book| {
            confirm_in(book, set, kassensturz_core::ai_switch::permissions())
        })
        .unwrap_or(Err(ChangeError::OutOfDate));
        if let Some(from) = &from
            && let Some(inbox) = chat.as_mut().rust_mut().state.inbox.as_mut()
        {
            tell_confirmed(inbox, from, &result);
        }
        if from.is_none() {
            let outcome = outcome_of(&result, corrected);
            chat.as_mut().rust_mut().state.tell_chat(outcome);
        }
        match result {
            Ok(()) => {
                say("changes-applied", &[]);
                crate::note!(ChangesApplied);
                chat.as_mut().changed();
                map([("ok", QVariant::from(&true))])
            }
            Err(failure) => {
                let report = failure.report();
                say("changes-failed", &[("code", report.code)]);
                crate::note!(ChangesFailed, None, Some(report.code));
                error(&code_text(&report))
            }
        }
    }

    /// Corrects one change of a waiting set (D-293): the user's version is
    /// checked like the AI's and replaces the set only when it passes.
    pub(super) fn correct(
        mut chat: Pin<&mut qobject::AiChat>,
        id: &str,
        slot: &str,
        correction: Correction,
    ) -> QVariant {
        let out_of_date = || error(&ErrorText::plain("ai-changes.out-of-date"));
        let Some(id) = id_of(id) else {
            return out_of_date();
        };
        let Some(set) = chat.state.sets.get(id) else {
            return out_of_date();
        };
        let checked = session::with_book(|book| {
            let set = corrected(
                book,
                set,
                slot,
                &correction,
                kassensturz_core::ai_switch::permissions(),
            )?;
            let rows = rows_of(book, &set, &app_locale())
                .map_err(|cause| EditError::Refused(ToolError::Store(cause)))?;
            Ok::<_, EditError>((set, rows))
        });
        let how = match correction {
            Correction::Edit(_) => "edited",
            Correction::LeaveOut => "left out",
            Correction::BackToAi => "back to the AI's version",
        };
        match checked {
            None => out_of_date(),
            Some(Err(failure)) => {
                let report = failure.report();
                crate::developer_mode::transcribe(
                    "changes-edit-refused",
                    &[("slot", slot), ("code", report.code)],
                );
                error(&code_text(&report))
            }
            Some(Ok((set, rows))) => {
                crate::developer_mode::transcribe(
                    "changes-edited",
                    &[("slot", slot), ("how", how), ("set", &set.summary())],
                );
                chat.as_mut().rust_mut().state.sets.replace(id, set);
                changes_object(&rows)
            }
        }
    }

    pub(super) fn change_choices(chat: &qobject::AiChat, id: &str) -> QVariant {
        let Some(set) = id_of(id).and_then(|id| chat.state.sets.get(id)) else {
            return error(&ErrorText::plain("ai-changes.out-of-date"));
        };
        match session::with_book(|book| category_choices(book, set)) {
            Some(Ok(choices)) => choices_object(&choices),
            Some(Err(cause)) => error(&book_unreadable(cause)),
            None => error(&ErrorText::plain("ai-changes.out-of-date")),
        }
    }

    pub(super) fn discard(mut chat: Pin<&mut qobject::AiChat>, id: &str) {
        crate::note!(ChangesDeclined);
        let mut rust = chat.as_mut().rust_mut();
        let state = &mut rust.state;
        let Some(Waiting { set, from }) = id_of(id).and_then(|id| state.sets.take(id)) else {
            return;
        };
        let summary = set.summary();
        let fields = [("set", summary.as_str())];
        if from.is_some() {
            crate::developer_mode::transcribe_in("mcp", "changes-declined", &fields);
        } else {
            crate::developer_mode::transcribe("changes-declined", &fields);
        }
        match from {
            Some(from) => {
                if let Some(inbox) = state.inbox.as_mut() {
                    tell_declined(inbox, &from);
                }
            }
            None => state.tell_chat(Outcome::Declined),
        }
    }

    pub(super) fn changes_undone(mut chat: Pin<&mut qobject::AiChat>) {
        chat.as_mut().rust_mut().state.tell_chat(Outcome::Undone);
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
            session::with_book(|book| {
                take_requests(
                    inbox,
                    &path,
                    book,
                    sets,
                    kassensturz_core::ai_switch::permissions(),
                    app_locale().today(),
                )
            })
            .unwrap_or_default()
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

    fn edit_change(
        self: Pin<&mut Self>,
        id: &QString,
        slot: &QString,
        fields: &QString,
    ) -> QVariant {
        engine::correct(
            self,
            &id.to_string(),
            &slot.to_string(),
            crate::ai_chat::Correction::Edit(fields.to_string()),
        )
    }

    fn leave_out_change(self: Pin<&mut Self>, id: &QString, slot: &QString) -> QVariant {
        engine::correct(
            self,
            &id.to_string(),
            &slot.to_string(),
            crate::ai_chat::Correction::LeaveOut,
        )
    }

    fn back_to_ai(self: Pin<&mut Self>, id: &QString, slot: &QString) -> QVariant {
        engine::correct(
            self,
            &id.to_string(),
            &slot.to_string(),
            crate::ai_chat::Correction::BackToAi,
        )
    }

    fn change_choices(&self, id: &QString) -> QVariant {
        engine::change_choices(self, &id.to_string())
    }

    fn changes_undone(self: Pin<&mut Self>) {
        engine::changes_undone(self);
    }

    fn reset(self: Pin<&mut Self>) {
        engine::reset(self);
    }

    fn check_requests(self: Pin<&mut Self>) {
        engine::check_requests(self);
    }

    fn issue_used(&self, how: &QString) {
        match how.to_string().as_str() {
            "opened" => crate::note!(IssuePageOpened),
            "copied" => crate::note!(IssueTextCopied),
            _ => {}
        }
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

    fn edit_change(
        self: Pin<&mut Self>,
        _id: &QString,
        _slot: &QString,
        _fields: &QString,
    ) -> QVariant {
        engine::off()
    }

    fn leave_out_change(self: Pin<&mut Self>, _id: &QString, _slot: &QString) -> QVariant {
        engine::off()
    }

    fn back_to_ai(self: Pin<&mut Self>, _id: &QString, _slot: &QString) -> QVariant {
        engine::off()
    }

    fn change_choices(&self, _id: &QString) -> QVariant {
        engine::off()
    }

    fn changes_undone(self: Pin<&mut Self>) {}

    fn reset(self: Pin<&mut Self>) {}

    fn check_requests(self: Pin<&mut Self>) {}

    fn issue_used(&self, _how: &QString) {}
}

#[cfg(all(test, feature = "ai"))]
mod tests {
    use cxx_qt_lib::{QString, QVariant};
    use kassensturz_ai::Finish;
    use kassensturz_ai_chat::{Answer, LogEntry, Mode};

    use super::engine::{
        changes_object, choices_object, entry_object, finished_object, review_object, sets_object,
    };
    use crate::ai_chat::{BasisView, CategoryChoice, ChangeRow, ChangeView, EditLine, Review};
    use crate::rules::ConditionForm;
    use crate::variant::read_back::{
        elements, flag_of, keys, list_of, number_of, object_of, text_of,
    };

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
            context_cut: true,
            changes: Vec::new(),
            title_numbers: Vec::new(),
            chart_failed: None,
            issue: None,
            page: None,
        };
        let object = finished_object(&answer, &[(3, 2), (4, 1)], &[]);
        assert_eq!(
            keys(&object),
            [
                "charts",
                "contextCut",
                "finish",
                "overview",
                "sets",
                "text",
                "unverified"
            ]
        );
        assert!(flag_of(&object, "contextCut"));
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
    fn charts_and_a_given_up_chart_read_back_as_qml_reads_them() {
        use kassensturz_ai_chat::ChartFailed;

        use super::engine::{ChartSet, charts_object};
        let mut answer = Answer {
            text: "Here is your chart.".into(),
            unverified: Vec::new(),
            finish: Finish::Done,
            mode: Mode::Functions,
            context_cut: false,
            changes: Vec::new(),
            title_numbers: Vec::new(),
            chart_failed: None,
            issue: None,
            page: None,
        };
        let charts = [
            ChartSet {
                id: 5,
                unverified: vec!["12345.67".into()],
            },
            ChartSet {
                id: 6,
                unverified: Vec::new(),
            },
        ];
        let object = finished_object(&answer, &[(3, 2)], &charts);
        assert!(
            !keys(&object).contains(&"chartFailed".to_owned()),
            "{:?}",
            keys(&object)
        );
        let read = list_of(&object, "charts");
        assert_eq!(read.len(), 2);
        assert_eq!(keys(&read[0]), ["id", "unverified"]);
        assert_eq!(text_of(&read[0], "id"), "5");
        let numbers = list_of(&read[0], "unverified");
        assert_eq!(numbers.len(), 1);
        assert_eq!(
            numbers[0].value::<QString>().map(|n| n.to_string()),
            Some("12345.67".to_owned()),
            "a number is text"
        );
        assert!(list_of(&read[1], "unverified").is_empty());
        assert_eq!(
            elements(&charts_object(&[])).map(|found| found.len()),
            Some(0)
        );

        // A chart the chat gave up on carries the dataset it understood.
        answer.chart_failed = Some(ChartFailed {
            code: "ai-tools.bad-argument",
            dataset: Some("spending_by_category"),
        });
        let object = finished_object(&answer, &[], &[]);
        assert!(keys(&object).contains(&"chartFailed".to_owned()));
        let failed = object_of(&object, "chartFailed");
        assert_eq!(keys(&failed), ["dataset"]);
        assert_eq!(text_of(&failed, "dataset"), "spending_by_category");
        answer.chart_failed = Some(ChartFailed {
            code: "ai-tools.bad-argument",
            dataset: None,
        });
        let object = finished_object(&answer, &[], &[]);
        assert_eq!(text_of(&object_of(&object, "chartFailed"), "dataset"), "");
    }

    #[test]
    fn an_issue_draft_reads_back_as_qml_reads_it() {
        use kassensturz_ai_tools::{ISSUE_PAGE, IssueDraft};
        let issue = IssueDraft {
            title: "A balance looks wrong".into(),
            body: "### What happened\n\nIt failed.".into(),
            link: format!("{ISSUE_PAGE}?title=A"),
            whole_in_link: false,
        };
        let answer = Answer {
            text: "I prepared a report.".into(),
            unverified: Vec::new(),
            finish: Finish::Done,
            mode: Mode::Functions,
            context_cut: false,
            changes: Vec::new(),
            title_numbers: Vec::new(),
            chart_failed: None,
            issue: Some(issue.clone()),
            page: None,
        };
        let object = finished_object(&answer, &[], &[]);
        assert_eq!(
            keys(&object),
            [
                "charts",
                "contextCut",
                "finish",
                "issue",
                "overview",
                "sets",
                "text",
                "unverified"
            ]
        );
        let read = super::engine::issue_object(&issue);
        assert_eq!(keys(&read), ["body", "link", "title", "wholeInLink"]);
        assert_eq!(text_of(&read, "title"), issue.title);
        assert_eq!(text_of(&read, "body"), issue.body);
        assert_eq!(text_of(&read, "link"), issue.link);
        assert!(!flag_of(&read, "wholeInLink"));
    }

    #[test]
    fn a_page_the_ai_opens_reads_back_as_qml_reads_it() {
        use kassensturz_ai_tools::PageTarget;
        let page = PageTarget::Rules {
            select: Some("1b6f0c1e-0000-4000-8000-000000000001".into()),
        };
        let answer = Answer {
            text: "I opened your Rule.".into(),
            unverified: Vec::new(),
            finish: Finish::Done,
            mode: Mode::Functions,
            context_cut: false,
            changes: Vec::new(),
            title_numbers: Vec::new(),
            chart_failed: None,
            issue: None,
            page: Some(page.clone()),
        };
        let object = finished_object(&answer, &[], &[]);
        assert_eq!(
            keys(&object),
            [
                "charts",
                "contextCut",
                "finish",
                "overview",
                "page",
                "sets",
                "text",
                "unverified"
            ]
        );
        let read = super::engine::page_object(&page);
        assert_eq!(keys(&read), ["id", "name"]);
        assert_eq!(text_of(&read, "name"), "rules");
        assert_eq!(text_of(&read, "id"), "1b6f0c1e-0000-4000-8000-000000000001");
        // A page with no account or Rule has an empty ID, not a missing one.
        let read = super::engine::page_object(&PageTarget::Categories);
        assert_eq!(text_of(&read, "name"), "categories");
        assert_eq!(text_of(&read, "id"), "");
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
                currency: "EUR".into(),
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
            ChangeView::EditCategory {
                before: "Food".into(),
                after: "Survival › Food".into(),
            },
            ChangeView::EditRule {
                before_name: "Grocer".into(),
                name: "Supermarket".into(),
                before_book_to: "Food › Groceries".into(),
                book_to: "Food".into(),
                match_all: false,
                conditions: Vec::new(),
            },
            ChangeView::MoveRule {
                name: "Market".into(),
                from: "3".into(),
                to: "1".into(),
            },
            ChangeView::DeleteRule {
                name: "Pets".into(),
                book_to: String::new(),
                match_all: true,
                conditions: Vec::new(),
            },
            ChangeView::Categorize {
                date: "2026-10-03".into(),
                payee: "Corner Grocery".into(),
                account: "Everyday Checking".into(),
                amount: "-53,20 €".into(),
                before: String::new(),
                after: "Food › Bakery".into(),
                currency: "EUR".into(),
            },
            ChangeView::DeleteCategory {
                path: "Food › Bakery".into(),
            },
            ChangeView::NewAccount {
                name: "Holiday money".into(),
                account_type: "cash".into(),
                currency: "EUR".into(),
            },
            ChangeView::EditAccount {
                before: "Wallet".into(),
                after: "Old wallet".into(),
                was_closed: false,
                closed: true,
            },
            ChangeView::EditTransaction {
                date: "2026-10-03".into(),
                payee: "Corner Grocery".into(),
                account: "Everyday Checking".into(),
                amount: "-53,20 €".into(),
                lines: vec![
                    EditLine {
                        field: "amount",
                        before: "-53,20 €".into(),
                        after: "-60,00 €".into(),
                    },
                    EditLine {
                        field: "note",
                        before: String::new(),
                        after: "<b>weekly</b>".into(),
                    },
                ],
            },
            ChangeView::NewTransaction {
                date: "2026-10-27".into(),
                payee: "Dome Kiosk".into(),
                account: "Pocket Cash".into(),
                amount: "-12,34 €".into(),
                category: "Food › Snacks".into(),
                note: "<i>receipt kept</i>".into(),
            },
            ChangeView::DeleteTransaction {
                date: "2026-10-27".into(),
                payee: "Dome Kiosk".into(),
                account: "Pocket Cash".into(),
                amount: "-12,34 €".into(),
                category: String::new(),
            },
            ChangeView::NewView {
                title: "<b>Where it goes</b>".into(),
                period: "this-year".into(),
                component: r#"{"type":"chart.pie"}"#.into(),
                charts: vec!["chart.pie", "chart.bar"],
            },
            ChangeView::NewBudget {
                category: "Food › <b>Groceries</b>".into(),
                every: "month",
                from: "01.11.2026".into(),
                to: "30.11.2026".into(),
                amount: "390,00 €".into(),
                carry_over: true,
                basis: Some(BasisView {
                    periods: 6,
                    from: "01.05.2026".into(),
                    to: "31.10.2026".into(),
                    spent: "2.324,40 €".into(),
                    average: "387,40 €".into(),
                    step: "10,00 €".into(),
                }),
            },
            ChangeView::EditBudget {
                category: "Fun".into(),
                every: "quarter",
                from: "01.10.2026".into(),
                to: "31.12.2026".into(),
                before: "100,00 €".into(),
                amount: "120,00 €".into(),
                carried_over: false,
                carry_over: true,
                basis: None,
            },
        ];
        let rows: Vec<ChangeRow> = views
            .into_iter()
            .enumerate()
            .map(|(n, view)| ChangeRow {
                slot: format!("new_rules/{n}"),
                edited: n == 1,
                left_out: n == 2,
                fields: if n == 1 {
                    vec!["name", "match"]
                } else {
                    Vec::new()
                },
                form: if n == 1 {
                    r#"{"name":"<b>Bakery</b>"}"#.into()
                } else {
                    String::new()
                },
                view,
            })
            .collect();
        let object = changes_object(&rows);
        let items = elements(&object).expect("an array");
        assert_eq!(items.len(), 16);
        // What the user did with each change (D-293).
        assert_eq!(
            keys(&items[1]),
            [
                "bookTo",
                "conditions",
                "edited",
                "fields",
                "form",
                "kind",
                "leftOut",
                "matchAll",
                "name",
                "slot"
            ]
        );
        assert_eq!(text_of(&items[1], "slot"), "new_rules/1");
        assert!(flag_of(&items[1], "edited") && !flag_of(&items[1], "leftOut"));
        assert!(flag_of(&items[2], "leftOut") && !flag_of(&items[2], "edited"));
        let fields: Vec<String> = list_of(&items[1], "fields")
            .iter()
            .map(|field| {
                field
                    .value::<QString>()
                    .map(|text| text.to_string())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(fields, ["name", "match"]);
        assert_eq!(text_of(&items[1], "form"), r#"{"name":"<b>Bakery</b>"}"#);
        assert_eq!(text_of(&items[0], "form"), "");
        assert!(list_of(&items[0], "fields").is_empty());
        assert_eq!(
            own_keys(&items[0]),
            ["currency", "income", "kind", "name", "path"]
        );
        assert_eq!(text_of(&items[0], "currency"), "EUR");
        assert_eq!(text_of(&items[0], "kind"), "newCategory");
        assert!(!flag_of(&items[0], "income"));
        assert_eq!(
            own_keys(&items[1]),
            ["bookTo", "conditions", "kind", "matchAll", "name"]
        );
        assert!(flag_of(&items[1], "matchAll"));
        let conditions = list_of(&items[1], "conditions");
        assert_eq!(text_of(&conditions[0], "operator"), "contains");
        assert_eq!(text_of(&conditions[0], "value"), "<img src=x>");
        assert_eq!(own_keys(&items[2]), ["after", "before", "kind"]);
        assert_eq!(text_of(&items[2], "kind"), "editCategory");
        assert_eq!(text_of(&items[2], "after"), "Survival › Food");
        assert_eq!(
            own_keys(&items[3]),
            [
                "beforeBookTo",
                "beforeName",
                "bookTo",
                "conditions",
                "kind",
                "matchAll",
                "name"
            ]
        );
        assert_eq!(text_of(&items[3], "beforeName"), "Grocer");
        assert!(!flag_of(&items[3], "matchAll"));
        assert_eq!(own_keys(&items[4]), ["from", "kind", "name", "to"]);
        assert_eq!(text_of(&items[4], "to"), "1");
        assert_eq!(
            own_keys(&items[5]),
            ["bookTo", "conditions", "kind", "matchAll", "name"]
        );
        assert_eq!(text_of(&items[5], "kind"), "deleteRule");
        assert_eq!(
            own_keys(&items[6]),
            [
                "account", "after", "amount", "before", "currency", "date", "kind", "payee"
            ]
        );
        assert_eq!(text_of(&items[6], "amount"), "-53,20 €");
        assert_eq!(text_of(&items[6], "before"), "");
        assert_eq!(own_keys(&items[7]), ["kind", "path"]);
        assert_eq!(text_of(&items[7], "path"), "Food › Bakery");
        assert_eq!(
            own_keys(&items[8]),
            ["accountType", "currency", "kind", "name"]
        );
        assert_eq!(text_of(&items[8], "accountType"), "cash");
        assert_eq!(
            own_keys(&items[9]),
            ["after", "before", "closed", "kind", "wasClosed"]
        );
        assert!(flag_of(&items[9], "closed"));
        assert!(!flag_of(&items[9], "wasClosed"));
        assert_eq!(
            own_keys(&items[10]),
            ["account", "amount", "date", "kind", "lines", "payee"]
        );
        assert_eq!(text_of(&items[10], "kind"), "editTransaction");
        let lines = list_of(&items[10], "lines");
        assert_eq!(lines.len(), 2);
        assert_eq!(keys(&lines[0]), ["after", "before", "field"]);
        assert_eq!(text_of(&lines[0], "field"), "amount");
        assert_eq!(text_of(&lines[0], "after"), "-60,00 €");
        assert_eq!(text_of(&lines[1], "after"), "<b>weekly</b>");
        assert_eq!(
            own_keys(&items[11]),
            [
                "account", "amount", "category", "date", "kind", "note", "payee"
            ]
        );
        assert_eq!(text_of(&items[11], "kind"), "newTransaction");
        assert_eq!(text_of(&items[11], "note"), "<i>receipt kept</i>");
        assert_eq!(
            own_keys(&items[12]),
            ["account", "amount", "category", "date", "kind", "payee"]
        );
        assert_eq!(text_of(&items[12], "kind"), "deleteTransaction");
        assert_eq!(text_of(&items[12], "category"), "");
        assert_eq!(
            own_keys(&items[13]),
            ["charts", "component", "kind", "period", "title"]
        );
        assert_eq!(list_of(&items[13], "charts").len(), 2);
        assert_eq!(text_of(&items[13], "kind"), "newView");
        assert_eq!(text_of(&items[13], "title"), "<b>Where it goes</b>");
        assert_eq!(text_of(&items[13], "period"), "this-year");
        assert_eq!(text_of(&items[13], "component"), r#"{"type":"chart.pie"}"#);
        let basis = [
            "basisAverage",
            "basisFrom",
            "basisPeriods",
            "basisSpent",
            "basisStep",
            "basisTo",
        ];
        let mut expected: Vec<&str> = [
            "amount",
            "carryOver",
            "category",
            "every",
            "from",
            "kind",
            "to",
        ]
        .into_iter()
        .chain(basis)
        .collect();
        expected.sort_unstable();
        assert_eq!(own_keys(&items[14]), expected);
        assert_eq!(text_of(&items[14], "kind"), "newBudget");
        assert_eq!(text_of(&items[14], "category"), "Food › <b>Groceries</b>");
        assert_eq!(text_of(&items[14], "every"), "month");
        assert_eq!(text_of(&items[14], "amount"), "390,00 €");
        assert!(flag_of(&items[14], "carryOver"));
        assert_eq!(number_of(&items[14], "basisPeriods"), 6);
        assert_eq!(text_of(&items[14], "basisAverage"), "387,40 €");
        assert_eq!(text_of(&items[14], "basisStep"), "10,00 €");
        assert_eq!(text_of(&items[14], "basisFrom"), "01.05.2026");
        let mut expected: Vec<&str> = [
            "amount",
            "before",
            "carriedOver",
            "carryOver",
            "category",
            "every",
            "from",
            "kind",
            "to",
        ]
        .into_iter()
        .chain(basis)
        .collect();
        expected.sort_unstable();
        assert_eq!(own_keys(&items[15]), expected);
        assert_eq!(text_of(&items[15], "kind"), "editBudget");
        assert_eq!(text_of(&items[15], "before"), "100,00 €");
        assert!(!flag_of(&items[15], "carriedOver"));
        assert!(flag_of(&items[15], "carryOver"));
        assert_eq!(number_of(&items[15], "basisPeriods"), 0);
        assert_eq!(text_of(&items[15], "basisAverage"), "");
    }

    /// The keys of a change object without those every change has.
    fn own_keys(item: &QVariant) -> Vec<String> {
        keys(item)
            .into_iter()
            .filter(|key| !["edited", "fields", "form", "leftOut", "slot"].contains(&key.as_str()))
            .collect()
    }

    #[test]
    fn category_choices_read_back_as_qml_reads_them() {
        let object = choices_object(&[
            CategoryChoice {
                id: "1b0c".into(),
                key: String::new(),
                path: "Food › <b>Bakery</b>".into(),
                income: false,
                currency: "EUR".into(),
            },
            CategoryChoice {
                id: String::new(),
                key: "pets".into(),
                path: "Pets".into(),
                income: true,
                currency: "USD".into(),
            },
        ]);
        let items = elements(&object).expect("an array");
        assert_eq!(items.len(), 2);
        assert_eq!(keys(&items[0]), ["currency", "id", "income", "key", "path"]);
        assert_eq!(text_of(&items[0], "id"), "1b0c");
        assert_eq!(text_of(&items[0], "path"), "Food › <b>Bakery</b>");
        assert_eq!(text_of(&items[1], "key"), "pets");
        assert!(flag_of(&items[1], "income"));
        assert_eq!(text_of(&items[1], "currency"), "USD");
    }
}
