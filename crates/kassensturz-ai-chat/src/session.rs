// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One conversation with the model: the loop of the read functions, the
//! fallback to the overview, the number check and the tool-call log (D-190).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use kassensturz_ai::{AiError, Finish, FunctionSpec, Message, PreparedRequest, Provider, ToolCall};
use kassensturz_ai_tools::{
    ChangeSet, DRAFT_ISSUE, FUNCTIONS, Figures, IssueContext, IssueDraft, PROPOSE_CHANGES,
    ToolError, Unverified, check_numbers, draft_issue, propose, unique_codes,
};
#[cfg(feature = "debug-log")]
use kassensturz_ai_tools::{MAX_LOG_ENTRIES, READ_DEBUG_LOG, read_debug_log};
#[cfg(feature = "debug-log")]
use kassensturz_core::debug_log::DebugLog;
use kassensturz_core::{Date, ErrorParam, ErrorReport, Report};
use kassensturz_platform::Secret;
use kassensturz_store::Book;
use serde_json::{Map, Value, json};

use crate::error::ChatError;
use crate::log::{LogEntry, MAX_ARGUMENT_CHARS, cut};
use crate::prompt::{self, BuildInfo};

/// The most requests one question may take. A model that is still calling
/// functions after them gets `ai-chat.too-many-rounds`; a question about
/// the book needs two or three.
pub const MAX_ROUNDS: usize = 8;

/// What the model is told when it asked for a function that isn't offered.
/// It goes as a message of its own because a function's answer must name a
/// call the provider knows, and such a call was never handed on (threat
/// T-31, D-190).
pub const NOT_OFFERED: &str = "KassenSturz: a function you asked for is not offered, so it \
                               was not run (error ai-tools.unknown-function). Call only the \
                               functions offered.";

/// What the user did with the changes the model prepared, which the model
/// hears at its next question (D-229).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The user confirmed them and they were made.
    Confirmed,
    /// The user threw them away; nothing was changed.
    Declined,
    /// The user confirmed them but they could not be made; nothing was
    /// changed.
    Failed,
    /// The user took the confirmed changes back with Undo.
    Undone,
}

impl Outcome {
    /// What the model is told. Fixed text from KassenSturz, nothing from
    /// the book.
    pub fn text(self) -> &'static str {
        match self {
            Self::Confirmed => {
                "KassenSturz: the user reviewed the changes you prepared in KassenSturz's chat \
                 sidebar and confirmed them. They were made in the user's book. Nothing \
                 waits for approval now."
            }
            Self::Declined => {
                "KassenSturz: the user reviewed the changes you prepared and refused them. \
                 Nothing was changed in the user's book, and nothing waits for approval now."
            }
            Self::Failed => {
                "KassenSturz: the user confirmed the changes you prepared, but they could not \
                 be made, so nothing was changed in the user's book. Nothing waits for \
                 approval now."
            }
            Self::Undone => {
                "KassenSturz: the user took the changes you prepared back with Undo. They are \
                 no longer in the user's book."
            }
        }
    }
}

/// How the model gets the book's figures (D-052).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// It calls the read functions.
    Functions,
    /// It can't call functions and gets the small overview with each
    /// request.
    Overview,
}

/// What the model answered to a question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// The answer, untrusted text from the model: shown as plain text and
    /// labelled as AI (threat T-34).
    pub text: String,
    /// Every number in it that no function produced, to be marked (D-188,
    /// threat T-26).
    pub unverified: Vec<Unverified>,
    /// Why the model stopped: [`Finish::Length`] means the answer was cut.
    pub finish: Finish,
    /// How the model got its figures.
    pub mode: Mode,
    /// The changes the model prepared for this question, in the order it
    /// called `propose_changes`, for the user to review (D-171, D-199).
    /// Nothing in them is applied: only the review screen's confirm does.
    pub changes: Vec<ChangeSet>,
    /// The issue draft the model prepared for this question, the last one
    /// if it made several, for the user to read (D-173, D-228). Nothing is
    /// sent: only the user opens the prefilled page.
    pub issue: Option<IssueDraft>,
}

/// What the front end does while a question runs. Every method has a
/// default, so a caller implements only what it needs.
pub trait Hooks {
    /// Called before each request goes, with the request exactly as it
    /// will be sent, so the user can see it and redact phrases from it
    /// (D-035). Answering `false` stops the question (`ai-chat.stopped`).
    fn review(&mut self, request: &mut PreparedRequest) -> bool {
        let _ = request;
        true
    }

    /// Called with every new line of the tool-call log as it happens.
    fn logged(&mut self, entry: &LogEntry) {
        let _ = entry;
    }

    /// Whether the user stopped the question.
    fn cancelled(&self) -> bool {
        false
    }
}

/// Hooks that let every request go and never stop.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHooks;

impl Hooks for NoHooks {}

/// A stop button for a question that runs on another thread: clones share
/// it.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A button not yet pressed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Presses it.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether it was pressed.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Hooks for Cancel {
    fn cancelled(&self) -> bool {
        self.is_cancelled()
    }
}

/// One conversation with the model about one book.
#[derive(Debug)]
pub struct ChatSession {
    build: BuildInfo,
    mode: Mode,
    /// Whether the provider ever took the functions, after which a refusal
    /// is an error, not a model that can't call them.
    functions_taken: bool,
    /// The conversation without KassenSturz's instructions, which are
    /// built for each request.
    history: Vec<Message>,
    /// Every figure the functions, the user and the build produced so far.
    figures: Figures,
    log: Vec<LogEntry>,
    /// What the user did with prepared changes since the last question, to
    /// tell the model with the next one.
    outcomes: Vec<Outcome>,
    /// The change sets the model prepared for the question under way.
    proposed: Vec<ChangeSet>,
    /// The issue draft the model prepared for the question under way.
    issue: Option<IssueDraft>,
    /// The debug log the conversation writes to and the model may read
    /// while AI and the log are on (D-173).
    #[cfg(feature = "debug-log")]
    debug_log: DebugLog,
    /// The screen the user asked from, for an issue draft.
    screen: Option<&'static str>,
}

impl ChatSession {
    /// A new, empty conversation in the build `build`.
    pub fn new(build: BuildInfo) -> Self {
        let mut figures = Figures::new();
        figures.add_text(&build.version);
        figures.add_text(&build.commit);
        Self {
            build,
            mode: Mode::Functions,
            functions_taken: false,
            history: Vec::new(),
            figures,
            log: Vec::new(),
            outcomes: Vec::new(),
            proposed: Vec::new(),
            issue: None,
            #[cfg(feature = "debug-log")]
            debug_log: DebugLog::none(),
            screen: None,
        }
    }

    /// The conversation, writing to `log` while the developer mode is on
    /// and letting the model read it while AI is on too (D-173, D-227).
    /// Without one, nothing is written and the model isn't offered the log.
    #[cfg(feature = "debug-log")]
    pub fn with_debug_log(mut self, log: DebugLog) -> Self {
        self.debug_log = log;
        self
    }

    /// The screen the user asks from, a name from the debug log's fixed
    /// list, for an issue draft.
    pub fn set_screen(&mut self, screen: Option<&'static str>) {
        self.screen = screen;
    }

    /// How the model gets its figures now.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The tool-call log so far.
    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    /// The conversation so far, without KassenSturz's instructions.
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Remembers what the user did with changes the model prepared, so the
    /// model hears it as a message from KassenSturz before the next
    /// question and doesn't go on saying they wait (D-229).
    pub fn note_outcome(&mut self, outcome: Outcome) {
        self.outcomes.push(outcome);
    }

    /// Asks `question` about `book` through `provider`, with `key` when the
    /// provider needs one; `today` is the day of this computer's clock. The
    /// model calls the read functions until it answers, at most
    /// [`MAX_ROUNDS`] requests. When the question ends without an answer
    /// the conversation is as it was before, and the log says why.
    pub fn ask(
        &mut self,
        book: &Book,
        provider: &dyn Provider,
        key: Option<&Secret>,
        question: &str,
        today: Date,
        hooks: &mut dyn Hooks,
    ) -> Result<Answer, ChatError> {
        // Kept even when the question fails: it happened.
        for outcome in std::mem::take(&mut self.outcomes) {
            self.history.push(Message::user(outcome.text()));
        }
        let start = self.history.len();
        #[cfg(feature = "debug-log")]
        self.debug_log.start_turn();
        let result = self.run(book, provider, key, question, today, hooks);
        if let Err(error) = &result {
            self.history.truncate(start);
            #[cfg(feature = "debug-log")]
            self.debug_log
                .transcript("failed", &[("code", error.report().code)]);
            self.note(
                LogEntry::Failed {
                    code: error.report().code,
                },
                hooks,
            );
        }
        result
    }

    fn run(
        &mut self,
        book: &Book,
        provider: &dyn Provider,
        key: Option<&Secret>,
        question: &str,
        today: Date,
        hooks: &mut dyn Hooks,
    ) -> Result<Answer, ChatError> {
        if hooks.cancelled() {
            return Err(ChatError::Cancelled);
        }
        self.note(
            LogEntry::Asked {
                question: question.to_owned(),
            },
            hooks,
        );
        self.figures.add_text(question);
        self.history.push(Message::user(question));
        self.proposed.clear();
        self.issue = None;
        let mut overview = None;
        if self.mode == Mode::Overview {
            overview = Some(self.overview(book, today)?);
        }
        let mut round = 0;
        while round < MAX_ROUNDS {
            round += 1;
            if hooks.cancelled() {
                return Err(ChatError::Cancelled);
            }
            // Asked again for each request: the user may turn the log on
            // or off while a question runs.
            let log_readable = self.log_readable();
            let (system, functions) = match &overview {
                Some(overview) => (
                    prompt::with_overview(&self.build, today, overview),
                    Vec::new(),
                ),
                None => (
                    prompt::with_functions(&self.build, today, log_readable),
                    function_specs(log_readable),
                ),
            };
            let offered = !functions.is_empty();
            let mut messages = Vec::with_capacity(self.history.len() + 1);
            messages.push(Message::system(system));
            messages.extend(self.history.iter().cloned());
            let mut request = provider.prepare(messages, functions)?;
            if !hooks.review(&mut request) {
                return Err(ChatError::Stopped);
            }
            self.note(
                LogEntry::Sent {
                    round,
                    body: request.preview().to_owned(),
                },
                hooks,
            );
            let sent = provider.send(&request, key);
            self.transcribe_reply(provider, sent.as_ref().err());
            let reply = match sent {
                Ok(reply) => reply,
                // A model that can't call functions: the provider refuses
                // the request that offers them (D-190).
                Err(AiError::ProviderFailed { status: 400 })
                    if offered && !self.functions_taken =>
                {
                    self.mode = Mode::Overview;
                    self.note(LogEntry::Overview, hooks);
                    overview = Some(self.overview(book, today)?);
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            // Whatever came back while the user stopped is thrown away.
            if hooks.cancelled() {
                return Err(ChatError::Cancelled);
            }
            if offered {
                self.functions_taken = true;
            }
            if reply.calls.is_empty() && reply.ignored_calls.is_empty() {
                let unverified = check_numbers(&reply.text, &self.figures);
                #[cfg(feature = "debug-log")]
                self.debug_log.transcript(
                    "answer",
                    &[
                        ("text", &reply.text),
                        ("unverified", &unverified.len().to_string()),
                    ],
                );
                self.note(
                    LogEntry::Answered {
                        unverified: unverified.len(),
                    },
                    hooks,
                );
                self.history
                    .push(Message::assistant(reply.text.clone(), Vec::new()));
                return Ok(Answer {
                    text: reply.text,
                    unverified,
                    finish: reply.finish,
                    mode: self.mode,
                    changes: std::mem::take(&mut self.proposed),
                    issue: self.issue.take(),
                });
            }
            self.history
                .push(Message::assistant(reply.text, reply.calls.clone()));
            for call in &reply.calls {
                let message = self.run_call(book, call, today, hooks);
                self.history.push(message);
            }
            if !reply.ignored_calls.is_empty() {
                self.note(
                    LogEntry::NotOffered {
                        calls: reply.ignored_calls.len(),
                    },
                    hooks,
                );
                self.history.push(Message::user(NOT_OFFERED));
            }
        }
        Err(ChatError::TooManyRounds)
    }

    /// Runs one call of the model and answers with the function's result,
    /// or its error code, for the model.
    fn run_call(
        &mut self,
        book: &Book,
        call: &ToolCall,
        today: Date,
        hooks: &mut dyn Hooks,
    ) -> Message {
        let outcome = serde_json::from_str::<Value>(&call.arguments)
            .map_err(|_| ToolError::BadArgument {
                argument: "arguments",
            })
            .and_then(|arguments| {
                if call.name == PROPOSE_CHANGES.name {
                    // Prepared for the user, never applied here (T-27).
                    propose(book, &arguments).map(|proposal| {
                        self.proposed.push(proposal.changes);
                        proposal.output
                    })
                } else if call.name == DRAFT_ISSUE.name {
                    // Shown to the user, never sent from here (T-33).
                    draft_issue(book, &arguments, &self.issue_context()).map(|(draft, output)| {
                        self.issue = Some(draft);
                        output
                    })
                } else {
                    self.read(book, &call.name, &arguments, today)
                }
            });
        let (result, error) = match outcome {
            Ok(output) => {
                self.figures.extend(&output.figures);
                (prompt::data_json(&output.value), None)
            }
            Err(error) => {
                let report = error.report();
                (prompt::data_json(&error_json(&report)), Some(report.code))
            }
        };
        self.note(
            LogEntry::Called {
                function: call.name.clone(),
                arguments: cut(&call.arguments, MAX_ARGUMENT_CHARS),
                result: result.clone(),
                error,
            },
            hooks,
        );
        Message::tool_result(call.id.clone(), result)
    }

    /// The overview for a model that can't call functions, with its figures
    /// added to those the answer may name.
    fn overview(&mut self, book: &Book, today: Date) -> Result<Value, ChatError> {
        let output = kassensturz_ai_tools::overview(book, today).map_err(ChatError::Book)?;
        self.figures.extend(&output.figures);
        Ok(output.value)
    }

    /// Whether the model may read the debug log now: only while AI and the
    /// log are both on (D-173), never in a build without the log.
    fn log_readable(&self) -> bool {
        #[cfg(feature = "debug-log")]
        return self.debug_log.readable_by_ai();
        #[cfg(not(feature = "debug-log"))]
        false
    }

    /// Runs the read function `name`: one of the book's, or the one that
    /// reads the debug log.
    fn read(
        &self,
        book: &Book,
        name: &str,
        arguments: &Value,
        today: Date,
    ) -> Result<kassensturz_ai_tools::Output, ToolError> {
        #[cfg(feature = "debug-log")]
        if name == READ_DEBUG_LOG.name {
            return read_debug_log(&self.debug_log, arguments);
        }
        kassensturz_ai_tools::call(book, name, arguments, today)
    }

    /// What KassenSturz adds to an issue draft: the build, the screen, and
    /// the error codes of this conversation and, while AI may read it, of
    /// the debug log.
    fn issue_context(&self) -> IssueContext {
        #[cfg(feature = "debug-log")]
        let (log_read, logged) = {
            let readable = self.debug_log.readable_by_ai();
            let codes: Vec<String> = if readable {
                self.debug_log
                    .read(MAX_LOG_ENTRIES)
                    .into_iter()
                    .filter_map(|entry| entry.code)
                    .collect()
            } else {
                Vec::new()
            };
            (readable, codes)
        };
        #[cfg(not(feature = "debug-log"))]
        let (log_read, logged) = (false, Vec::<String>::new());
        let conversation = self.log.iter().filter_map(|entry| match entry {
            LogEntry::Called { error, .. } => *error,
            LogEntry::Failed { code } => Some(*code),
            _ => None,
        });
        IssueContext {
            version: self.build.version.clone(),
            commit: self.build.commit.clone(),
            screen: self.screen,
            codes: unique_codes(logged.iter().map(String::as_str).chain(conversation)),
            log_read,
        }
    }

    /// The provider's reply as it came, and why it couldn't be read when it
    /// couldn't, for the developer mode's transcript (D-235).
    fn transcribe_reply(&self, provider: &dyn Provider, error: Option<&AiError>) {
        #[cfg(feature = "debug-log")]
        {
            let raw = provider.last_reply_text().unwrap_or_default();
            let code = error.map_or("", |error| error.report().code);
            self.debug_log
                .transcript("reply", &[("raw", &raw), ("error", code)]);
        }
        #[cfg(not(feature = "debug-log"))]
        let _ = (provider, error);
    }

    fn note(&mut self, entry: LogEntry, hooks: &mut dyn Hooks) {
        #[cfg(feature = "debug-log")]
        {
            let (event, detail, code) = entry.debug_event();
            self.debug_log.record(event, detail, code);
            entry.transcribe(&self.debug_log);
        }
        hooks.logged(&entry);
        self.log.push(entry);
    }
}

/// The first request of a question, as the provider would get it for
/// `question` asked without any earlier conversation, but with nothing
/// sent: the screens show it as an example of what a request holds
/// (M5.7). Its parts are the ones [`ChatSession::ask`] builds, and a test
/// holds the two together.
pub fn sample_request(
    provider: &dyn Provider,
    build: &BuildInfo,
    today: Date,
    question: &str,
) -> Result<PreparedRequest, AiError> {
    provider.prepare(
        vec![
            Message::system(prompt::with_functions(build, today, false)),
            Message::user(question),
        ],
        function_specs(false),
    )
}

/// The read functions, the one that prepares changes and the one that
/// drafts an issue, as the provider gets them, and the one that reads the
/// debug log while the model may read it.
fn function_specs(log_readable: bool) -> Vec<FunctionSpec> {
    let offered = FUNCTIONS.iter().chain([&PROPOSE_CHANGES, &DRAFT_ISSUE]);
    #[cfg(feature = "debug-log")]
    let offered = offered.chain(log_readable.then_some(&READ_DEBUG_LOG));
    #[cfg(not(feature = "debug-log"))]
    let _ = log_readable;
    offered
        .map(|function| FunctionSpec {
            name: function.name.to_owned(),
            description: function.description.to_owned(),
            parameters: serde_json::from_str(function.parameters)
                .unwrap_or_else(|_| json!({"type": "object"})),
        })
        .collect()
}

/// A function's error for the model: its code and the fixed names it
/// carries, never anything the model sent (D-034, X-8).
fn error_json(report: &ErrorReport) -> Value {
    let mut out = Map::new();
    out.insert("error".into(), json!(report.code));
    for (name, value) in &report.params {
        match value {
            ErrorParam::Keyword(keyword) => {
                out.insert((*name).into(), json!(keyword));
            }
            ErrorParam::Number(number) => {
                out.insert((*name).into(), json!(number));
            }
            _ => {}
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_is_offered_with_its_parameters() {
        let specs = function_specs(false);
        assert_eq!(specs.len(), FUNCTIONS.len() + 2);
        assert_eq!(specs[FUNCTIONS.len()].name, PROPOSE_CHANGES.name);
        assert_eq!(specs[FUNCTIONS.len() + 1].name, DRAFT_ISSUE.name);
        for (spec, function) in specs
            .iter()
            .zip(FUNCTIONS.iter().chain([&PROPOSE_CHANGES, &DRAFT_ISSUE]))
        {
            assert_eq!(spec.name, function.name);
            assert_eq!(
                spec.parameters,
                serde_json::from_str::<Value>(function.parameters).unwrap(),
                "{}'s parameters are JSON",
                function.name
            );
        }
        // The debug log is offered only while the model may read it, and
        // never in a build without it.
        let with_log = function_specs(true);
        #[cfg(feature = "debug-log")]
        {
            assert_eq!(with_log.len(), specs.len() + 1);
            assert_eq!(with_log[specs.len()].name, READ_DEBUG_LOG.name);
            assert_eq!(
                with_log[specs.len()].parameters,
                serde_json::from_str::<Value>(READ_DEBUG_LOG.parameters).unwrap()
            );
        }
        #[cfg(not(feature = "debug-log"))]
        assert_eq!(with_log.len(), specs.len());
        assert_eq!(with_log[..specs.len()], specs[..]);
    }
    #[test]
    fn an_error_carries_only_its_code_and_fixed_names() {
        let report = ToolError::BadArgument { argument: "from" }.report();
        assert_eq!(
            error_json(&report),
            json!({"error": "ai-tools.bad-argument", "argument": "from"})
        );
    }

    #[test]
    fn a_cancel_is_shared_by_its_clones() {
        let cancel = Cancel::new();
        let other = cancel.clone();
        assert!(!other.cancelled());
        cancel.cancel();
        assert!(other.cancelled());
    }
}
